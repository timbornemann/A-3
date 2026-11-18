//! ADR-0122/0123 Win32 boundary for the private loopback verification job and bind.

use a3_application::ProcessRunFailure;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::process::Command;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, GENERIC_READ, GENERIC_WRITE, GetLastError, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
    WaitForSingleObject,
};

const MAX_WIDE_UNITS: usize = 32_767;
const ATTRIBUTE_COUNT: u32 = 2;
const MAX_ATTRIBUTE_BYTES: usize = 4096;
const CLEANUP_POLL: Duration = Duration::from_millis(10);
const QUOTE: u16 = b'"' as u16;
const BACKSLASH: u16 = b'\\' as u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowsJobSpawnFailure {
    NotStarted,
    Unknown,
}

/// A signaled direct child can report success, an ordinary application failure, or another code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowsObservedExit {
    /// The signaled child reported code zero.
    Zero,
    /// Observed code in the recipe's 1–255 range. Its internal cause is not inferred.
    ApplicationFailure(std::num::NonZeroU8),
    /// The observed value is outside the ordinary application-code range.
    OutOfRange(u32),
}

/// The non-inheritable job is retained until cleanup and listener checks are complete.
#[derive(Debug)]
pub(super) struct WindowsJobChild {
    process: Option<OwnedHandle>,
    job: OwnedHandle,
    pid: u32,
    stop_requested: bool,
}

impl WindowsJobChild {
    pub(super) fn spawn(command: &Command) -> Result<Self, WindowsJobSpawnFailure> {
        let job = create_job()?;
        let (process, pid) = launch(command, &job)?;
        Ok(Self {
            process: Some(process),
            job,
            pid,
            stop_requested: false,
        })
    }

    pub(super) const fn id(&self) -> u32 {
        self.pid
    }

    pub(super) fn is_running(&self) -> Result<bool, ProcessRunFailure> {
        let Some(process) = &self.process else {
            return Ok(false);
        };
        // SAFETY: the owned process handle is live throughout this non-blocking wait.
        match unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(ProcessRunFailure::InvalidResult),
        }
    }

    /// Observes the natural exit of the exact direct child before requesting any termination.
    pub(super) fn poll_exit_before_stop(
        &self,
    ) -> Result<Option<WindowsObservedExit>, ProcessRunFailure> {
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;

        if self.stop_requested {
            return Err(ProcessRunFailure::InvalidResult);
        }
        let process = self
            .process
            .as_ref()
            .ok_or(ProcessRunFailure::InvalidResult)?;
        if self.is_running()? {
            return Ok(None);
        }
        let mut code = 0;
        // SAFETY: the still-owned direct process handle was observed signaled by the wait above.
        // The initialized DWORD is writable and the owner has not requested termination.
        let read = unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) };
        if read == 0 {
            return Err(ProcessRunFailure::InvalidResult);
        }
        let observed = match u8::try_from(code) {
            Ok(code) => match std::num::NonZeroU8::new(code) {
                Some(code) => WindowsObservedExit::ApplicationFailure(code),
                None => WindowsObservedExit::Zero,
            },
            Err(_) => WindowsObservedExit::OutOfRange(code),
        };
        Ok(Some(observed))
    }

    pub(super) fn stop(&mut self, deadline: Instant) -> Result<(), ProcessRunFailure> {
        cleanup_remaining(deadline)?;
        self.stop_requested = true;
        // SAFETY: the job is private to this owner and its valid handle stays live for the call.
        let terminated = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } != 0;
        cleanup_remaining(deadline)?;
        while self
            .is_running()
            .map_err(|_| ProcessRunFailure::TerminationUnavailable)?
        {
            if !terminated {
                return Err(ProcessRunFailure::TerminationUnavailable);
            }
            wait_to_recheck(deadline)?;
        }
        // A terminated process may still count as active until every owned reference is closed.
        drop(self.process.take());
        loop {
            cleanup_remaining(deadline)?;
            let active = self.active_processes()?;
            cleanup_remaining(deadline)?;
            if active == 0 {
                return Ok(());
            }
            if !terminated {
                return Err(ProcessRunFailure::TerminationUnavailable);
            }
            wait_to_recheck(deadline)?;
        }
    }

    fn active_processes(&self) -> Result<u32, ProcessRunFailure> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the job handle is owned, and the writable buffer has the exact size and
        // alignment of the selected accounting class. The optional result-length pointer is null.
        let result = unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        };
        if result == 0 {
            return Err(ProcessRunFailure::TerminationUnavailable);
        }
        Ok(accounting.ActiveProcesses)
    }
}

/// Bound-only lifetime of a literal IPv4 loopback endpoint. There is no listen operation.
pub(super) struct ExclusiveLoopbackBind {
    _socket: tokio::net::TcpSocket,
}

impl ExclusiveLoopbackBind {
    pub(super) fn bind(port: std::num::NonZeroU16) -> std::io::Result<Self> {
        use std::net::{Ipv4Addr, SocketAddrV4};
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::{
            SO_EXCLUSIVEADDRUSE, SOL_SOCKET, WSAGetLastError, setsockopt,
        };

        // Tokio's pinned socket2 path creates a non-inheritable Winsock handle.
        let socket = tokio::net::TcpSocket::new_v4()?;
        let enabled: i32 = 1;
        // SAFETY: the live socket is borrowed, still unbound, and the option reads exactly one
        // initialized Win32 BOOL for this call. Rust retains sole ownership of the socket.
        let configured = unsafe {
            setsockopt(
                socket.as_raw_socket() as usize,
                SOL_SOCKET,
                SO_EXCLUSIVEADDRUSE,
                (&raw const enabled).cast(),
                size_of::<i32>() as i32,
            )
        };
        if configured != 0 {
            // SAFETY: Winsock returns the calling thread's error for the immediately failed call.
            let error = unsafe { WSAGetLastError() };
            return Err(std::io::Error::from_raw_os_error(error));
        }
        socket.bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.get()).into())?;
        Ok(Self { _socket: socket })
    }
}

fn cleanup_remaining(deadline: Instant) -> Result<Duration, ProcessRunFailure> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ProcessRunFailure::TerminationUnavailable)
}

fn wait_to_recheck(deadline: Instant) -> Result<(), ProcessRunFailure> {
    std::thread::sleep(CLEANUP_POLL.min(cleanup_remaining(deadline)?));
    cleanup_remaining(deadline)?;
    Ok(())
}

fn create_job() -> Result<OwnedHandle, WindowsJobSpawnFailure> {
    // SAFETY: null attributes create a non-inheritable handle, and a null name creates a private job.
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    // SAFETY: successful CreateJobObjectW transfers this single, valid handle to the caller.
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // No breakaway or silent breakaway. A worker exit closes the only job handle.
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if !set_job_limits(&job, &limits) {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    Ok(job)
}

fn set_job_limits(job: &OwnedHandle, limits: &JOBOBJECT_EXTENDED_LIMIT_INFORMATION) -> bool {
    // SAFETY: the valid job is borrowed for the call and the input structure matches this class.
    unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0
    }
}

fn open_null() -> Result<OwnedHandle, WindowsJobSpawnFailure> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    // Absolute Win32 device path, independent of the workspace and CWD.
    let device: Vec<u16> = r"\\.\NUL".encode_utf16().chain([0]).collect();
    // SAFETY: the path is NUL-terminated, the attributes are initialized, and this open has no
    // template. The one inheritable handle is the harmless NUL device, used for all three streams.
    let handle = unsafe {
        CreateFileW(
            device.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &attributes,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    // SAFETY: successful CreateFileW transfers sole ownership of a valid, non-sentinel handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

#[repr(align(16))]
#[derive(Clone, Copy)]
struct AttributeBlock {
    _bytes: [u8; 16],
}

struct AttributeList(Vec<AttributeBlock>);

impl AttributeList {
    fn new() -> Result<Self, WindowsJobSpawnFailure> {
        let mut bytes = 0;
        // SAFETY: Win32 uses a null list as a size query and writes only to this valid size pointer.
        let (queried, error) = unsafe {
            let queried =
                InitializeProcThreadAttributeList(null_mut(), ATTRIBUTE_COUNT, 0, &mut bytes);
            (queried, GetLastError())
        };
        if queried != 0
            || error != ERROR_INSUFFICIENT_BUFFER
            || !(1..=MAX_ATTRIBUTE_BYTES).contains(&bytes)
        {
            return Err(WindowsJobSpawnFailure::NotStarted);
        }
        let mut storage =
            vec![AttributeBlock { _bytes: [0; 16] }; bytes.div_ceil(size_of::<AttributeBlock>())];
        // SAFETY: the aligned allocation is writable for at least `bytes` and never reallocated.
        let initialized = unsafe {
            InitializeProcThreadAttributeList(
                storage.as_mut_ptr().cast(),
                ATTRIBUTE_COUNT,
                0,
                &mut bytes,
            )
        };
        if initialized == 0 {
            return Err(WindowsJobSpawnFailure::NotStarted);
        }
        Ok(Self(storage))
    }

    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: construction returned only after successful initialization. The allocation
        // remains live until this destructor returns and the initialized list is deleted once.
        unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
    }
}

fn launch(
    command: &Command,
    job: &OwnedHandle,
) -> Result<(OwnedHandle, u32), WindowsJobSpawnFailure> {
    use WindowsJobSpawnFailure::{NotStarted, Unknown};
    let program = Path::new(command.get_program());
    let directory = command.get_current_dir().ok_or(NotStarted)?;
    if !program.is_absolute()
        || !directory.is_absolute()
        || !program
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(NotStarted);
    }
    let mut executable = wide(program.as_os_str())?;
    let mut arguments = command_line(command, &executable)?;
    executable.push(0);
    let mut working_directory = wide(directory.as_os_str())?;
    working_directory.push(0);
    let environment = environment_block(command)?;
    let stdio = open_null()?;
    let std_handle = stdio.as_raw_handle();
    let job_handles = [job.as_raw_handle()];
    let inherited = [std_handle];
    let mut attributes = AttributeList::new()?;
    let list = attributes.as_mut_ptr();
    // SAFETY: the list is initialized for two attributes. These fixed-size handle arrays and
    // the borrowed owners are live until after DeleteProcThreadAttributeList. The OS requires
    // null reserved arguments and a list containing only actual inheritable handles for stdio.
    let ready = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            job_handles.as_ptr().cast(),
            size_of_val(&job_handles),
            null_mut(),
            null(),
        ) != 0
            && UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_ptr().cast(),
                size_of_val(&inherited),
                null_mut(),
                null(),
            ) != 0
    };
    if !ready {
        return Err(NotStarted);
    }
    let startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: std_handle,
            hStdOutput: std_handle,
            hStdError: std_handle,
            ..STARTUPINFOW::default()
        },
        lpAttributeList: list,
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: both fixed paths and the writable argv are NUL-terminated, the explicit UTF-16
    // environment ends with two NULs, and STARTUPINFOEXW embeds STARTUPINFOW as its first field.
    // All buffers, handle values and the initialized list remain live through the synchronous
    // call. The non-inheritable job is assigned during creation, with only NUL inherited.
    let created = unsafe {
        CreateProcessW(
            executable.as_ptr(),
            arguments.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            environment.as_ptr().cast(),
            working_directory.as_ptr(),
            (&raw const startup).cast(),
            &mut info,
        )
    };
    if created == 0 {
        return Err(Unknown);
    }
    // SAFETY: successful CreateProcessW transfers the returned process and thread handles once.
    // Adopt each valid handle even if a separate result field is unexpectedly invalid.
    let (process, thread) = unsafe {
        (
            (!info.hProcess.is_null() && info.hProcess != INVALID_HANDLE_VALUE)
                .then(|| OwnedHandle::from_raw_handle(info.hProcess)),
            (!info.hThread.is_null() && info.hThread != INVALID_HANDLE_VALUE)
                .then(|| OwnedHandle::from_raw_handle(info.hThread)),
        )
    };
    match (process, thread, info.dwProcessId) {
        (Some(process), Some(_thread), pid) if pid != 0 => Ok((process, pid)),
        _ => Err(Unknown),
    }
}

fn wide(value: &OsStr) -> Result<Vec<u16>, WindowsJobSpawnFailure> {
    let units: Vec<_> = value.encode_wide().take(MAX_WIDE_UNITS).collect();
    if units.len() == MAX_WIDE_UNITS || units.contains(&0) {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    Ok(units)
}

fn command_line(command: &Command, program: &[u16]) -> Result<Vec<u16>, WindowsJobSpawnFailure> {
    if program.contains(&QUOTE) {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    let mut line = Vec::new();
    // The CRT parses argv[0] as an executable path. Subsequent arguments use backslash escaping.
    line.push(QUOTE);
    line.extend_from_slice(program);
    line.push(QUOTE);
    for argument in command.get_args() {
        line.extend([b' ' as u16, QUOTE]);
        let mut backslashes = 0;
        for unit in wide(argument)? {
            if unit == BACKSLASH {
                backslashes += 1;
                continue;
            }
            let escaped = if unit == QUOTE {
                backslashes * 2 + 1
            } else {
                backslashes
            };
            line.extend(std::iter::repeat_n(BACKSLASH, escaped));
            line.push(unit);
            backslashes = 0;
        }
        line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
        line.push(QUOTE);
        if line.len() >= MAX_WIDE_UNITS {
            return Err(WindowsJobSpawnFailure::NotStarted);
        }
    }
    if line.len() >= MAX_WIDE_UNITS {
        return Err(WindowsJobSpawnFailure::NotStarted);
    }
    line.push(0);
    Ok(line)
}

fn environment_block(command: &Command) -> Result<Vec<u16>, WindowsJobSpawnFailure> {
    use WindowsJobSpawnFailure::NotStarted;
    // The prepared command holds only explicit entries. ProcessSpec limits names to ASCII, so
    // ASCII case folding is the same ordering as Windows' case-insensitive environment ordering.
    let mut entries = Vec::new();
    let mut size = 1;
    for (name, value) in command.get_envs() {
        let Some(value) = value else {
            continue;
        };
        let name = name
            .to_str()
            .filter(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
            .ok_or(NotStarted)?;
        let value = wide(value)?;
        size += name.len() + value.len() + 2;
        if size > MAX_WIDE_UNITS {
            return Err(NotStarted);
        }
        entries.push((name.to_ascii_uppercase(), name, value));
    }
    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(NotStarted);
    }
    let mut environment = Vec::with_capacity(size.max(2));
    for (_, name, value) in entries {
        environment.extend(name.encode_utf16());
        environment.push(b'=' as u16);
        environment.extend(value);
        environment.push(0);
    }
    if environment.is_empty() {
        environment.push(0);
    }
    environment.push(0);
    Ok(environment)
}

#[cfg(test)]
mod job_tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::num::{NonZeroU8, NonZeroU16};
    use std::path::PathBuf;
    use std::process::{Child, Stdio};
    use windows_sys::Win32::Foundation::{DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessId, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        TerminateProcess,
    };

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
    const FIXTURE: &str = "loopback_service::windows_ffi::job_tests::job_fixture";
    const ROLE: &str = "A3_LOOPBACK_JOB_FIXTURE_ROLE";
    const MARKER: &str = "grandchild-running";
    const REJECTED_MARKER: &str = "rejected-child-ran";
    const CRASH_OWNER: &str = "worker-owned-child-handle";
    const CRASH_SUCCESS: &str = "worker-completed-normal-stop";
    const CRASH_EXIT_CODE: u32 = 0xA3C4;
    const TEST_WAIT: Duration = Duration::from_secs(10);
    const BIND_SCRIPT: &str = "bind_fixture.py";
    const BIND_SOURCE: &str = r"import argparse
import socket

parser = argparse.ArgumentParser()
parser.add_argument('--host', required=True)
parser.add_argument('--port', required=True, type=int)
args = parser.parse_args()
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as service:
    service.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    service.bind((args.host, args.port))
";

    #[test]
    fn job_fixture() -> TestResult {
        match std::env::var(ROLE).as_deref() {
            Ok("direct") => {
                let cwd = std::env::current_dir()?;
                let child = fixture_command(&cwd, "descendant")?.spawn()?;
                // Releasing this handle leaves the still-live descendant in the private job.
                drop(child);
                wait_until(|| Ok(Path::new(MARKER).is_file()))?;
            }
            Ok("descendant") => {
                std::fs::write(MARKER, b"running")?;
                std::thread::sleep(Duration::from_secs(30));
            }
            Ok("rejected") => std::fs::write(REJECTED_MARKER, b"ran")?,
            Ok("crash-worker") => {
                let directory = std::env::current_dir()?;
                let command = fixture_command(&directory, "descendant")?;
                let mut child =
                    WindowsJobChild::spawn(&command).map_err(|_| "owned child spawn failed")?;
                let process = child.process.as_ref().ok_or("missing owned process")?;
                let mut owner = [0; 12];
                owner[..4].copy_from_slice(&child.id().to_le_bytes());
                owner[4..]
                    .copy_from_slice(&(process.as_raw_handle() as usize as u64).to_le_bytes());
                std::fs::write(CRASH_OWNER, owner)?;
                let deadline = Instant::now() + Duration::from_secs(45);
                while child.is_running()? {
                    wait_to_recheck(deadline)?;
                }
                child.stop(Instant::now() + TEST_WAIT)?;
                std::fs::write(CRASH_SUCCESS, b"normal stop completed")?;
            }
            _ => {}
        }
        Ok(())
    }

    #[test]
    fn stop_releases_the_direct_child_and_catches_the_off_port_descendant() -> TestResult {
        let fixture = TestDirectory::new()?;
        let command = fixture_command(&fixture.0, "direct")?;
        let mut child = WindowsJobChild::spawn(&command).map_err(|_| "owned job spawn failed")?;
        assert_ne!(child.id(), 0);
        wait_until(|| Ok(fixture.0.join(MARKER).is_file() && !child.is_running()?))?;
        // The signaled direct child still contributes while this owner holds its handle.
        // A second association proves the marker's descendant remains in this job.
        assert_eq!(child.active_processes()?, 2);
        child.stop(Instant::now() + TEST_WAIT)?;
        assert!(child.process.is_none());
        assert_eq!(child.active_processes()?, 0);
        Ok(())
    }

    #[test]
    fn creation_cannot_escape_a_job_that_denies_assignment() -> TestResult {
        let fixture = TestDirectory::new()?;
        let command = fixture_command(&fixture.0, "descendant")?;
        let mut child = WindowsJobChild::spawn(&command).map_err(|_| "owned job spawn failed")?;
        wait_until(|| Ok(fixture.0.join(MARKER).is_file() && child.is_running()?))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        assert!(set_job_limits(&child.job, &limits));
        let rejected = fixture_command(&fixture.0, "rejected")?;
        assert!(matches!(
            launch(&rejected, &child.job),
            Err(WindowsJobSpawnFailure::Unknown)
        ));
        child.stop(Instant::now() + TEST_WAIT)?;
        assert_eq!(child.active_processes()?, 0);
        assert!(!fixture.0.join(REJECTED_MARKER).exists());
        Ok(())
    }

    #[test]
    fn externally_killed_worker_closes_the_job_and_terminates_the_owned_child() -> TestResult {
        let fixture = TestDirectory::new()?;
        let mut worker = CrashWorker(fixture_command(&fixture.0, "crash-worker")?.spawn()?);
        let mut owner = None;
        wait_until(|| {
            if worker.0.try_wait()?.is_some() {
                return Err("worker exited before publishing its child".into());
            }
            match std::fs::read(fixture.0.join(CRASH_OWNER)) {
                Ok(bytes) => owner = <[u8; 12]>::try_from(bytes).ok(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            Ok(owner.is_some())
        })?;
        let candidate =
            CrashCandidate::duplicate(&worker.0, owner.ok_or("worker child handle missing")?)?;
        assert!(worker.0.try_wait()?.is_none());
        wait_until(|| Ok(fixture.0.join(MARKER).is_file()))?;
        assert!(candidate.is_running()?);
        assert!(!fixture.0.join(CRASH_SUCCESS).exists());

        // SAFETY: this is the handle owned by std::process::Child for this test's worker.
        let killed = unsafe { TerminateProcess(worker.0.as_raw_handle(), CRASH_EXIT_CODE) };
        assert_ne!(killed, 0);
        wait_until(|| Ok(worker.0.try_wait()?.is_some()))?;
        let status = worker.0.try_wait()?.ok_or("worker did not terminate")?;
        assert_eq!(status.code(), Some(CRASH_EXIT_CODE as i32));
        wait_until(|| Ok(!candidate.is_running()?))?;
        assert!(!fixture.0.join(CRASH_SUCCESS).exists());
        Ok(())
    }

    #[test]
    fn an_expired_cleanup_deadline_leaves_no_stop_proof() -> TestResult {
        let fixture = TestDirectory::new()?;
        let command = fixture_command(&fixture.0, "descendant")?;
        let mut child = WindowsJobChild::spawn(&command).map_err(|_| "owned spawn failed")?;
        let candidate = CrashCandidate(
            child
                .process
                .as_ref()
                .ok_or("missing process")?
                .try_clone()?,
        );
        assert!(candidate.is_running()?);
        assert_eq!(
            child.stop(Instant::now()),
            Err(ProcessRunFailure::TerminationUnavailable)
        );
        drop(child);
        wait_until(|| Ok(!candidate.is_running()?))
    }

    struct CrashWorker(Child);

    impl Drop for CrashWorker {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = wait_until(|| Ok(self.0.try_wait()?.is_some()));
            }
        }
    }

    struct CrashCandidate(OwnedHandle);

    impl CrashCandidate {
        fn duplicate(worker: &Child, owner: [u8; 12]) -> TestResult<Self> {
            let (pid, handle) = owner.split_first_chunk::<4>().ok_or("missing process ID")?;
            let pid = u32::from_le_bytes(*pid);
            let source = usize::try_from(u64::from_le_bytes(handle.try_into()?))?;
            if pid == 0 || source == 0 || source == usize::MAX {
                return Err("invalid worker child identity".into());
            }
            let mut raw = null_mut();
            // SAFETY: the exact source process is held by Child. Win32 interprets the value
            // as a worker-local handle and duplicates it into our writable output. No PID
            // lookup or dereference occurs. The observer cannot keep the job handle open.
            let duplicated = unsafe {
                DuplicateHandle(
                    worker.as_raw_handle(),
                    source as HANDLE,
                    GetCurrentProcess(),
                    &mut raw,
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    0,
                )
            };
            if duplicated == 0 || raw.is_null() || raw == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error().into());
            }
            // SAFETY: successful DuplicateHandle transfers one valid owned handle to this test.
            let process = unsafe { OwnedHandle::from_raw_handle(raw) };
            // SAFETY: the duplicate stays open during the identity query.
            if unsafe { GetProcessId(process.as_raw_handle()) } != pid {
                return Err("duplicated handle did not refer to the worker's child".into());
            }
            Ok(Self(process))
        }

        fn is_running(&self) -> TestResult<bool> {
            // SAFETY: the independently owned process handle stays live for this zero-time wait.
            match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(false),
                WAIT_TIMEOUT => Ok(true),
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
    }

    #[test]
    fn exact_command_line_preserves_quotes_backslashes_and_empty_arguments() -> TestResult {
        let mut command = Command::new(r"D:\exact path\app.exe");
        command.args(["", "two words", r#"a\"b"#, r"tail\"]);
        let argv = command_line(
            &command,
            &wide(command.get_program()).map_err(|_| "invalid path")?,
        )
        .map_err(|_| "invalid argv")?;
        let (last, data) = argv.split_last().ok_or("argv missing")?;
        assert_eq!(*last, 0);
        assert_eq!(
            String::from_utf16(data)?,
            r#""D:\exact path\app.exe" "" "two words" "a\\\"b" "tail\\""#
        );
        Ok(())
    }

    #[test]
    fn adr0123_exclusive_bound_port_forces_real_exit_and_is_released_after_job_cleanup()
    -> TestResult {
        let fixture = TestDirectory::new()?;
        std::fs::write(fixture.0.join(BIND_SCRIPT), BIND_SOURCE)?;
        let candidate = tokio::net::TcpSocket::new_v4()?;
        candidate.bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into())?;
        let port = NonZeroU16::new(candidate.local_addr()?.port()).ok_or("unbound test socket")?;
        drop(candidate);
        let endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.get());
        let holder = ExclusiveLoopbackBind::bind(port)?;
        assert!(listener_absent(endpoint, TEST_WAIT)?);
        assert_reuse_rejected(endpoint)?;

        let mut bind = fixture_python(&fixture.0)?;
        bind.args([
            BIND_SCRIPT,
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ]);
        let exit = observe_python_exit_and_empty_job(&bind)?;
        assert_eq!(
            exit,
            WindowsObservedExit::ApplicationFailure(NonZeroU8::MIN)
        );
        // Even after complete job cleanup, the held bound-only socket remains exclusive.
        assert_reuse_rejected(endpoint)?;
        drop(holder);

        let rebound = ExclusiveLoopbackBind::bind(port)?;
        drop(rebound);
        assert_eq!(
            observe_python_exit_and_empty_job(&bind)?,
            WindowsObservedExit::Zero
        );

        let mut invalid = fixture_python(&fixture.0)?;
        invalid.args([BIND_SCRIPT, "--host", "127.0.0.1", "--port", "not-a-port"]);
        assert!(matches!(observe_python_exit_and_empty_job(&invalid)?,
            WindowsObservedExit::ApplicationFailure(code) if code.get() == 2));
        Ok(())
    }

    #[test]
    fn adr0123_signaled_large_code_and_owner_termination_do_not_supply_failure_exit() -> TestResult
    {
        let fixture = TestDirectory::new()?;
        let mut high = fixture_python(&fixture.0)?;
        high.args(["-c", "raise SystemExit(259)"]);
        assert_eq!(
            observe_python_exit_and_empty_job(&high)?,
            WindowsObservedExit::OutOfRange(259)
        );

        let mut waiting = fixture_python(&fixture.0)?;
        waiting.args(["-c", "import time; time.sleep(30)"]);
        let mut child =
            WindowsJobChild::spawn(&waiting).map_err(|_| "owned Python spawn failed")?;
        assert_eq!(child.poll_exit_before_stop()?, None);
        child.stop(Instant::now() + TEST_WAIT)?;
        assert_eq!(child.active_processes()?, 0);
        assert_eq!(
            child.poll_exit_before_stop(),
            Err(ProcessRunFailure::InvalidResult)
        );
        Ok(())
    }

    fn assert_reuse_rejected(endpoint: SocketAddrV4) -> TestResult {
        use windows_sys::Win32::Networking::WinSock::{WSAEACCES, WSAEADDRINUSE};

        let contender = tokio::net::TcpSocket::new_v4()?;
        contender.set_reuseaddr(true)?;
        let error = contender
            .bind(endpoint.into())
            .err()
            .ok_or("SO_REUSEADDR took over held port")?;
        assert!(matches!(
            error.raw_os_error(),
            Some(WSAEACCES | WSAEADDRINUSE)
        ));
        Ok(())
    }

    fn observe_python_exit_and_empty_job(command: &Command) -> TestResult<WindowsObservedExit> {
        let mut child = WindowsJobChild::spawn(command).map_err(|_| "owned Python spawn failed")?;
        let deadline = Instant::now() + TEST_WAIT;
        let exit = loop {
            if let Some(exit) = child.poll_exit_before_stop()? {
                break exit;
            }
            wait_to_recheck(deadline)?;
        };
        child.stop(Instant::now() + TEST_WAIT)?;
        assert_eq!(child.active_processes()?, 0);
        Ok(exit)
    }

    fn fixture_python(directory: &Path) -> TestResult<Command> {
        let path = std::env::var_os("PATH").ok_or("PATH missing for Python fixture")?;
        let executable = std::env::split_paths(&path)
            .filter(|path| path.is_absolute())
            .map(|directory| directory.join("python.exe"))
            .find(|path| path.is_file())
            .ok_or("python.exe not found for fixture")?
            .canonicalize()?;
        let system_root = std::env::var_os("SystemRoot").ok_or("SystemRoot missing for fixture")?;
        let mut command = Command::new(executable);
        command
            .arg("-B")
            .current_dir(directory)
            .env_clear()
            .env("SYSTEMROOT", system_root);
        Ok(command)
    }

    fn fixture_command(directory: &Path, role: &str) -> TestResult<Command> {
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot missing for fixture")?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", FIXTURE, "--test-threads=1"])
            .env_clear()
            .env("SystemRoot", root)
            .env(ROLE, role)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Ok(command)
    }

    fn wait_until(mut predicate: impl FnMut() -> TestResult<bool>) -> TestResult {
        let deadline = Instant::now() + TEST_WAIT;
        while !predicate()? {
            wait_to_recheck(deadline)?;
        }
        Ok(())
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> TestResult<Self> {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("a3 loopback job {}-{stamp}", std::process::id()));
            std::fs::create_dir(&directory)?;
            Ok(Self(directory))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.join(MARKER));
            let _ = std::fs::remove_file(self.0.join(REJECTED_MARKER));
            let _ = std::fs::remove_file(self.0.join(CRASH_OWNER));
            let _ = std::fs::remove_file(self.0.join(CRASH_SUCCESS));
            let _ = std::fs::remove_file(self.0.join(BIND_SCRIPT));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
}

// Windows TCP ownership for the closed loopback adapter.

const TCP_TABLE_WORD_BYTES: usize = std::mem::size_of::<u32>();
const TCP_TABLE_INITIAL_BYTES: usize = 16 * 1024;
const TCP_TABLE_MAX_BYTES: usize = 4 * 1024 * 1024;
const TCP_TABLE_MAX_CALLS: usize = 3;

#[derive(Clone, Copy)]
enum TcpOwnerFamily {
    V4,
    V6,
}

#[derive(Clone, Copy)]
struct TcpOwnerQuery {
    owner: Option<u32>,
    local: std::net::SocketAddrV4,
    peer: Option<std::net::SocketAddrV4>,
}

pub(super) fn observe(
    owner_pid: u32,
    local: std::net::SocketAddrV4,
    peer: Option<std::net::SocketAddrV4>,
    remaining: std::time::Duration,
) -> Result<super::Ownership, a3_application::ProcessRunFailure> {
    if owner_pid == 0 {
        return Err(a3_application::ProcessRunFailure::Denied);
    }
    tcp_owner_query(
        TcpOwnerQuery {
            owner: Some(owner_pid),
            local,
            peer,
        },
        remaining,
    )
}

pub(super) fn listener_absent(
    local: std::net::SocketAddrV4,
    remaining: std::time::Duration,
) -> Result<bool, a3_application::ProcessRunFailure> {
    Ok(tcp_owner_query(
        TcpOwnerQuery {
            owner: None,
            local,
            peer: None,
        },
        remaining,
    )? == super::Ownership::Absent)
}

fn tcp_owner_query(
    query: TcpOwnerQuery,
    remaining: std::time::Duration,
) -> Result<super::Ownership, a3_application::ProcessRunFailure> {
    use a3_application::ProcessRunFailure;
    use std::{net::Ipv4Addr, time::Instant};

    if *query.local.ip() != Ipv4Addr::LOCALHOST
        || query.local.port() == 0
        || query
            .peer
            .is_some_and(|peer| !peer.ip().is_loopback() || peer.port() == 0 || peer == query.local)
    {
        return Err(ProcessRunFailure::Denied);
    }
    let deadline = Instant::now()
        .checked_add(remaining)
        .ok_or(ProcessRunFailure::InvalidResult)?;
    let families: &[_] = if query.peer.is_some() {
        &[TcpOwnerFamily::V4]
    } else {
        &[TcpOwnerFamily::V4, TcpOwnerFamily::V6]
    };
    let mut ownership = super::Ownership::Absent;
    for &family in families {
        let (words, reported_bytes) = tcp_owner_table(family, deadline)?;
        tcp_scan_owner_table(
            family,
            &words,
            reported_bytes,
            query,
            deadline,
            &mut ownership,
        )?;
    }
    Ok(ownership)
}

fn tcp_owner_table(
    family: TcpOwnerFamily,
    deadline: std::time::Instant,
) -> Result<(Vec<u32>, usize), a3_application::ProcessRunFailure> {
    use a3_application::ProcessRunFailure;
    use windows_sys::Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        NetworkManagement::IpHelper::{GetExtendedTcpTable, TCP_TABLE_OWNER_PID_ALL},
        Networking::WinSock::{AF_INET, AF_INET6},
    };

    let af = match family {
        TcpOwnerFamily::V4 => AF_INET,
        TcpOwnerFamily::V6 => AF_INET6,
    };
    let mut required_bytes = TCP_TABLE_INITIAL_BYTES;
    let mut words = Vec::<u32>::new();
    for _ in 0..TCP_TABLE_MAX_CALLS {
        tcp_check_deadline(deadline)?;
        let word_count = required_bytes.div_ceil(TCP_TABLE_WORD_BYTES);
        words
            .try_reserve_exact(word_count - words.len())
            .map_err(|_| ProcessRunFailure::OutputUnavailable)?;
        words.resize(word_count, 0);
        let capacity_bytes = word_count * TCP_TABLE_WORD_BYTES;
        let mut reported_bytes =
            u32::try_from(capacity_bytes).map_err(|_| ProcessRunFailure::InvalidResult)?;
        tcp_check_deadline(deadline)?;
        // SAFETY: `words` is initialized writable DWORD-aligned storage of `reported_bytes`
        // bytes. Both OWNER_PID table layouts are DWORD-aligned. The call synchronously
        // writes only within the supplied buffer and size pointer, neither is retained.
        let status = unsafe {
            GetExtendedTcpTable(
                words.as_mut_ptr().cast(),
                &mut reported_bytes,
                0,
                u32::from(af),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        tcp_check_deadline(deadline)?;
        let reported_bytes =
            usize::try_from(reported_bytes).map_err(|_| ProcessRunFailure::InvalidResult)?;
        match status {
            ERROR_SUCCESS if (TCP_TABLE_WORD_BYTES..=capacity_bytes).contains(&reported_bytes) => {
                return Ok((words, reported_bytes));
            }
            ERROR_INSUFFICIENT_BUFFER
                if (capacity_bytes + 1..=TCP_TABLE_MAX_BYTES).contains(&reported_bytes) =>
            {
                required_bytes = reported_bytes;
            }
            ERROR_INSUFFICIENT_BUFFER => return Err(ProcessRunFailure::OutputUnavailable),
            _ => return Err(ProcessRunFailure::InvalidResult),
        }
    }
    Err(ProcessRunFailure::OutputUnavailable)
}

fn tcp_scan_owner_table(
    family: TcpOwnerFamily,
    words: &[u32],
    reported_bytes: usize,
    query: TcpOwnerQuery,
    deadline: std::time::Instant,
    ownership: &mut super::Ownership,
) -> Result<(), a3_application::ProcessRunFailure> {
    use a3_application::ProcessRunFailure;
    use std::{
        mem::{align_of, offset_of, size_of},
        net::{Ipv4Addr, Ipv6Addr},
    };
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        MIB_TCP_STATE_ESTAB, MIB_TCP_STATE_LISTEN, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID,
        MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
    };

    const {
        assert!(align_of::<MIB_TCPTABLE_OWNER_PID>() == align_of::<u32>());
        assert!(align_of::<MIB_TCP6TABLE_OWNER_PID>() == align_of::<u32>());
        assert!(offset_of!(MIB_TCPTABLE_OWNER_PID, dwNumEntries) == 0);
        assert!(offset_of!(MIB_TCP6TABLE_OWNER_PID, dwNumEntries) == 0);
    }
    let (row_start, row_bytes) = match family {
        TcpOwnerFamily::V4 => (
            offset_of!(MIB_TCPTABLE_OWNER_PID, table),
            size_of::<MIB_TCPROW_OWNER_PID>(),
        ),
        TcpOwnerFamily::V6 => (
            offset_of!(MIB_TCP6TABLE_OWNER_PID, table),
            size_of::<MIB_TCP6ROW_OWNER_PID>(),
        ),
    };
    let rows = usize::try_from(*words.first().ok_or(ProcessRunFailure::InvalidResult)?)
        .map_err(|_| ProcessRunFailure::InvalidResult)?;
    let end = rows
        .checked_mul(row_bytes)
        .and_then(|bytes| row_start.checked_add(bytes))
        .filter(|&end| end <= reported_bytes)
        .ok_or(ProcessRunFailure::InvalidResult)?;
    let data = words
        .get(tcp_word_offset(row_start)..tcp_word_offset(end))
        .ok_or(ProcessRunFailure::InvalidResult)?;

    for row in data.chunks_exact(tcp_word_offset(row_bytes)) {
        tcp_check_deadline(deadline)?;
        let (exact, pid) = match family {
            TcpOwnerFamily::V4 => {
                let field = |offset| row[tcp_word_offset(offset)];
                let state = field(offset_of!(MIB_TCPROW_OWNER_PID, dwState));
                let local = Ipv4Addr::from(
                    field(offset_of!(MIB_TCPROW_OWNER_PID, dwLocalAddr)).to_ne_bytes(),
                );
                if tcp_port(field(offset_of!(MIB_TCPROW_OWNER_PID, dwLocalPort)))
                    != query.local.port()
                {
                    continue;
                }
                let exact = local == *query.local.ip();
                match query.peer {
                    None if state == MIB_TCP_STATE_LISTEN as u32
                        && (exact || local.is_unspecified()) => {}
                    Some(peer)
                        if state == MIB_TCP_STATE_ESTAB as u32
                            && exact
                            && Ipv4Addr::from(
                                field(offset_of!(MIB_TCPROW_OWNER_PID, dwRemoteAddr)).to_ne_bytes(),
                            ) == *peer.ip()
                            && tcp_port(field(offset_of!(MIB_TCPROW_OWNER_PID, dwRemotePort)))
                                == peer.port() => {}
                    _ => continue,
                }
                (exact, field(offset_of!(MIB_TCPROW_OWNER_PID, dwOwningPid)))
            }
            TcpOwnerFamily::V6 => {
                if query.peer.is_some()
                    || row[tcp_word_offset(offset_of!(MIB_TCP6ROW_OWNER_PID, dwState))]
                        != MIB_TCP_STATE_LISTEN as u32
                    || tcp_port(
                        row[tcp_word_offset(offset_of!(MIB_TCP6ROW_OWNER_PID, dwLocalPort))],
                    ) != query.local.port()
                {
                    continue;
                }
                let first = tcp_word_offset(offset_of!(MIB_TCP6ROW_OWNER_PID, ucLocalAddr));
                let mut bytes = [0u8; 16];
                for (output, word) in bytes
                    .chunks_exact_mut(TCP_TABLE_WORD_BYTES)
                    .zip(&row[first..first + 4])
                {
                    output.copy_from_slice(&word.to_ne_bytes());
                }
                let address = Ipv6Addr::from(bytes);
                if !address.is_unspecified()
                    && !address.to_ipv4_mapped().is_some_and(|mapped| {
                        mapped.is_unspecified() || mapped == *query.local.ip()
                    })
                {
                    continue;
                }
                // A possible IPv6 collision never proves a listener on the exact IPv4 endpoint.
                (
                    false,
                    row[tcp_word_offset(offset_of!(MIB_TCP6ROW_OWNER_PID, dwOwningPid))],
                )
            }
        };
        *ownership = if *ownership == super::Ownership::Absent && exact && query.owner == Some(pid)
        {
            super::Ownership::Owned
        } else {
            super::Ownership::Foreign
        };
    }
    tcp_check_deadline(deadline)
}

const fn tcp_word_offset(bytes: usize) -> usize {
    assert!(bytes.is_multiple_of(TCP_TABLE_WORD_BYTES));
    bytes / TCP_TABLE_WORD_BYTES
}

fn tcp_port(word: u32) -> u16 {
    let [first, second, ..] = word.to_ne_bytes();
    u16::from_be_bytes([first, second])
}

fn tcp_check_deadline(
    deadline: std::time::Instant,
) -> Result<(), a3_application::ProcessRunFailure> {
    if std::time::Instant::now() >= deadline {
        return Err(a3_application::ProcessRunFailure::OutputUnavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tcp_owner_tests {
    use super::super::Ownership;
    use super::{TcpOwnerFamily, TcpOwnerQuery, listener_absent, observe, tcp_scan_owner_table};
    use a3_application::ProcessRunFailure;
    use std::{
        error::Error,
        net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    const OWNER: u32 = 41;
    const LOCAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 18765);
    const PEER: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 52993);

    fn port(port: u16) -> u32 {
        let [a, b] = port.to_be_bytes();
        // High bytes may be unrelated to the port.
        u32::from_ne_bytes([a, b, 0xab, 0xcd])
    }

    fn ipv4_row(state: u32, local: SocketAddrV4, peer: SocketAddrV4, pid: u32) -> [u32; 6] {
        [
            state,
            u32::from_ne_bytes(local.ip().octets()),
            port(local.port()),
            u32::from_ne_bytes(peer.ip().octets()),
            port(peer.port()),
            pid,
        ]
    }

    fn ipv6_listener(address: Ipv6Addr) -> [u32; 14] {
        let mut row = [0; 14];
        let bytes = address.octets();
        for (word, octets) in row.iter_mut().take(4).zip(bytes.chunks_exact(4)) {
            *word = u32::from_ne_bytes([octets[0], octets[1], octets[2], octets[3]]);
        }
        row[5] = port(LOCAL.port());
        row[12] = 2;
        row[13] = OWNER;
        row
    }

    fn scan<const WORDS: usize>(
        family: TcpOwnerFamily,
        rows: &[[u32; WORDS]],
        peer: Option<SocketAddrV4>,
        initial: Ownership,
    ) -> Result<Ownership, ProcessRunFailure> {
        let mut words =
            vec![u32::try_from(rows.len()).map_err(|_| ProcessRunFailure::InvalidResult)?];
        words.extend(rows.iter().flatten());
        let mut ownership = initial;
        tcp_scan_owner_table(
            family,
            &words,
            std::mem::size_of_val(words.as_slice()),
            TcpOwnerQuery {
                owner: Some(OWNER),
                local: LOCAL,
                peer,
            },
            Instant::now() + Duration::from_secs(1),
            &mut ownership,
        )?;
        Ok(ownership)
    }

    #[test]
    fn tcp_ipv4_connection_requires_server_direction_and_exact_owner()
    -> Result<(), ProcessRunFailure> {
        use TcpOwnerFamily::V4;
        let server = ipv4_row(5, LOCAL, PEER, OWNER);
        let client = ipv4_row(5, PEER, LOCAL, OWNER);
        assert_eq!(
            scan(V4, &[client], Some(PEER), Ownership::Absent)?,
            Ownership::Absent
        );
        assert_eq!(
            scan(V4, &[client, server], Some(PEER), Ownership::Absent)?,
            Ownership::Owned
        );
        let foreign = ipv4_row(5, LOCAL, PEER, OWNER + 1);
        assert_eq!(
            scan(V4, &[foreign], Some(PEER), Ownership::Absent)?,
            Ownership::Foreign
        );
        assert_eq!(
            scan(V4, &[server, server], Some(PEER), Ownership::Absent)?,
            Ownership::Foreign
        );
        let pending = ipv4_row(4, LOCAL, PEER, OWNER);
        assert_eq!(
            scan(V4, &[pending], Some(PEER), Ownership::Absent)?,
            Ownership::Absent
        );
        Ok(())
    }

    #[test]
    fn tcp_listener_rejects_wildcards_and_ipv4_mapped_ipv6_collisions()
    -> Result<(), ProcessRunFailure> {
        let exact = ipv4_row(2, LOCAL, PEER, OWNER);
        assert_eq!(
            scan(TcpOwnerFamily::V4, &[exact], None, Ownership::Absent)?,
            Ownership::Owned
        );
        let wildcard = ipv4_row(
            2,
            SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, LOCAL.port()),
            PEER,
            OWNER,
        );
        assert_eq!(
            scan(TcpOwnerFamily::V4, &[wildcard], None, Ownership::Absent)?,
            Ownership::Foreign
        );
        for collision in [
            Ipv6Addr::UNSPECIFIED,
            Ipv4Addr::UNSPECIFIED.to_ipv6_mapped(),
            Ipv4Addr::LOCALHOST.to_ipv6_mapped(),
        ] {
            assert_eq!(
                scan(
                    TcpOwnerFamily::V6,
                    &[ipv6_listener(collision)],
                    None,
                    Ownership::Owned
                )?,
                Ownership::Foreign
            );
        }
        assert_eq!(
            scan(
                TcpOwnerFamily::V6,
                &[ipv6_listener(Ipv6Addr::LOCALHOST)],
                None,
                Ownership::Absent
            )?,
            Ownership::Absent
        );
        Ok(())
    }

    #[test]
    fn tcp_truncated_count_and_expired_table_fail_closed() {
        let mut ownership = Ownership::Absent;
        let query = TcpOwnerQuery {
            owner: Some(OWNER),
            local: LOCAL,
            peer: None,
        };
        for (words, reported_bytes, deadline) in [
            (
                &[1, 0, 0, 0, 0, 0, 0][..],
                27,
                Instant::now() + Duration::from_secs(1),
            ),
            (&[u32::MAX][..], 4, Instant::now() + Duration::from_secs(1)),
            (&[0][..], 4, Instant::now()),
        ] {
            assert!(
                tcp_scan_owner_table(
                    TcpOwnerFamily::V4,
                    words,
                    reported_bytes,
                    query,
                    deadline,
                    &mut ownership
                )
                .is_err()
            );
        }
    }

    #[test]
    fn tcp_real_windows_table_attributes_listener_and_accepted_ipv4_pair()
    -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let SocketAddr::V4(local) = listener.local_addr()? else {
            return Err("expected an IPv4 loopback listener".into());
        };
        let limit = Duration::from_secs(2);
        let pid = std::process::id();
        assert!(!listener_absent(local, limit)?);
        assert_eq!(observe(pid, local, None, limit)?, Ownership::Owned);
        assert_eq!(
            observe(pid ^ u32::MAX, local, None, limit)?,
            Ownership::Foreign
        );

        let client = TcpStream::connect_timeout(&SocketAddr::V4(local), limit)?;
        let (_server, accepted_peer) = listener.accept()?;
        let SocketAddr::V4(peer) = client.local_addr()? else {
            return Err("expected an IPv4 loopback peer".into());
        };
        assert_eq!(accepted_peer, SocketAddr::V4(peer));
        assert_eq!(observe(pid, local, Some(peer), limit)?, Ownership::Owned);
        Ok(())
    }
}
