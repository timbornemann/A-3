use crate::process_environment::ProcessHostEnvironment;
use crate::process_launch::prepare_command;
use crate::process_output::{
    OutputCollectionError, OutputCollector, ReaderMessage, join_reader, spawn_reader,
};
use a3_application::{
    AuthorizedProcessSpec, ProcessEventSink, ProcessRunControl, ProcessRunFailure,
    ProcessRunFuture, ProcessRunner,
};
use a3_domain::{
    ProcessDuration, ProcessEvent, ProcessEventKind, ProcessEventSequence, ProcessExit,
    ProcessRunResult, ProcessStream, ProcessTermination, ProjectIdentity,
};
use command_group::{CommandGroup, GroupChild};
use std::io;
use std::process::{Command, ExitStatus};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const OUTPUT_CHANNEL_CAPACITY: usize = 32;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Direct argv workspace adapter with an empty-by-default host environment and owned process group.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceProcessRunner {
    environment: ProcessHostEnvironment,
}

impl WorkspaceProcessRunner {
    /// Creates a runner whose child environment can use only values from this explicit snapshot.
    #[must_use]
    pub const fn new(environment: ProcessHostEnvironment) -> Self {
        Self { environment }
    }

    fn run_sync(
        &self,
        project: &ProjectIdentity,
        authorized: AuthorizedProcessSpec,
        control: &dyn ProcessRunControl,
        events: &dyn ProcessEventSink,
    ) -> Result<ProcessRunResult, ProcessRunFailure> {
        if control.is_cancelled() {
            return Err(ProcessRunFailure::Cancelled);
        }
        let mut command = prepare_command(project, authorized.specification(), &self.environment)?;
        let (specification, policy_decision_id) = authorized.into_parts();
        if control.is_cancelled() {
            return Err(ProcessRunFailure::Cancelled);
        }

        let started = Instant::now();
        let mut child = spawn_process_group(&mut command)?;
        let stdout = child
            .inner()
            .stdout
            .take()
            .ok_or(ProcessRunFailure::OutputUnavailable);
        let stderr = child
            .inner()
            .stderr
            .take()
            .ok_or(ProcessRunFailure::OutputUnavailable);
        let (stdout, stderr) = match (stdout, stderr) {
            (Ok(stdout), Ok(stderr)) => (stdout, stderr),
            _ => {
                stop_group(&mut child)?;
                return Err(ProcessRunFailure::OutputUnavailable);
            }
        };

        let (sender, receiver) = mpsc::sync_channel(OUTPUT_CHANNEL_CAPACITY);
        let stdout_reader = match spawn_reader(ProcessStream::Stdout, stdout, sender.clone()) {
            Ok(handle) => handle,
            Err(_) => {
                drop(receiver);
                stop_group(&mut child)?;
                return Err(ProcessRunFailure::OutputUnavailable);
            }
        };
        let stderr_reader = match spawn_reader(ProcessStream::Stderr, stderr, sender.clone()) {
            Ok(handle) => handle,
            Err(_) => {
                drop(receiver);
                let stop_result = stop_group(&mut child);
                let join_result = join_reader(stdout_reader);
                stop_result?;
                join_result.map_err(map_output_error)?;
                return Err(ProcessRunFailure::OutputUnavailable);
            }
        };
        drop(sender);
        let readers = vec![stdout_reader, stderr_reader];

        let mut event_emitter = EventEmitter::new(specification.specification_id(), events);
        if event_emitter.emit(ProcessEventKind::Started).is_err() {
            cleanup_failed_run(&mut child, receiver, readers)?;
            return Err(ProcessRunFailure::EventUnavailable);
        }

        let mut stdout =
            OutputCollector::new(ProcessStream::Stdout, specification.stdout_limit().get());
        let mut stderr =
            OutputCollector::new(ProcessStream::Stderr, specification.stderr_limit().get());
        let mut stdout_done = false;
        let mut stderr_done = false;
        let timeout = Duration::from_millis(specification.timeout().as_millis());
        let (termination, termination_failed) = loop {
            if control.is_cancelled() || control.wait_cancelled_timeout(Duration::ZERO) {
                let failed = stop_group(&mut child).is_err();
                break (ProcessTermination::Cancelled, failed);
            }
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                let failed = stop_group(&mut child).is_err();
                break (ProcessTermination::TimedOut, failed);
            }
            let wait = PROCESS_POLL_INTERVAL.min(timeout.saturating_sub(elapsed));
            match receiver.recv_timeout(wait) {
                Ok(ReaderMessage::Chunk(stream, bytes)) => {
                    let collector = match stream {
                        ProcessStream::Stdout => &mut stdout,
                        ProcessStream::Stderr => &mut stderr,
                    };
                    let mut emit = |kind| event_emitter.emit_output(kind);
                    if let Err(error) = collector.ingest(&bytes, &mut emit) {
                        cleanup_failed_run(&mut child, receiver, readers)?;
                        return Err(map_output_error(error));
                    }
                }
                Ok(ReaderMessage::Eof(ProcessStream::Stdout)) if !stdout_done => {
                    stdout_done = true;
                }
                Ok(ReaderMessage::Eof(ProcessStream::Stderr)) if !stderr_done => {
                    stderr_done = true;
                }
                Ok(ReaderMessage::Eof(_)) => {
                    cleanup_failed_run(&mut child, receiver, readers)?;
                    return Err(ProcessRunFailure::OutputUnavailable);
                }
                Err(RecvTimeoutError::Disconnected) if stdout_done && stderr_done => {
                    let _cancelled = control.wait_cancelled_timeout(wait);
                }
                Ok(ReaderMessage::Failed) | Err(RecvTimeoutError::Disconnected) => {
                    cleanup_failed_run(&mut child, receiver, readers)?;
                    return Err(ProcessRunFailure::OutputUnavailable);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let exit = match ProcessExit::new(status.code(), status.success()) {
                        Ok(exit) => exit,
                        Err(_) => {
                            cleanup_failed_run(&mut child, receiver, readers)?;
                            return Err(ProcessRunFailure::InvalidResult);
                        }
                    };
                    break (ProcessTermination::Exited(exit), false);
                }
                Ok(None) => {}
                Err(_) => {
                    cleanup_failed_run(&mut child, receiver, readers)?;
                    return Err(ProcessRunFailure::TerminationUnavailable);
                }
            }
        };

        drain_readers(
            receiver,
            readers,
            &mut stdout,
            &mut stderr,
            &mut event_emitter,
            stdout_done,
            stderr_done,
        )?;
        if termination_failed {
            return Err(ProcessRunFailure::TerminationUnavailable);
        }
        let mut emit = |kind| event_emitter.emit_output(kind);
        let stdout = stdout.finish(&mut emit).map_err(map_output_error)?;
        let stderr = stderr.finish(&mut emit).map_err(map_output_error)?;
        let duration_millis = u64::try_from(started.elapsed().as_millis())
            .map_err(|_| ProcessRunFailure::InvalidResult)?;
        let result = ProcessRunResult::new(
            specification.specification_id(),
            policy_decision_id,
            termination,
            ProcessDuration::from_millis(duration_millis),
            stdout,
            stderr,
        )
        .map_err(|_| ProcessRunFailure::InvalidResult)?;
        event_emitter
            .emit(ProcessEventKind::Terminated(termination))
            .map_err(|_| ProcessRunFailure::EventUnavailable)?;
        Ok(result)
    }
}

impl ProcessRunner for WorkspaceProcessRunner {
    fn run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedProcessSpec,
        control: &'a dyn ProcessRunControl,
        events: &'a dyn ProcessEventSink,
    ) -> ProcessRunFuture<'a> {
        Box::pin(async move { self.run_sync(project, authorized, control, events) })
    }
}

fn spawn_process_group(command: &mut Command) -> Result<GroupChild, ProcessRunFailure> {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        command
            .group()
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| ProcessRunFailure::SpawnUnavailable)
    }

    #[cfg(not(windows))]
    {
        command
            .group_spawn()
            .map_err(|_| ProcessRunFailure::SpawnUnavailable)
    }
}

fn stop_group(child: &mut GroupChild) -> Result<ExitStatus, ProcessRunFailure> {
    let kill_failed = match child.kill() {
        Ok(()) => false,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
            ) =>
        {
            false
        }
        Err(_) => true,
    };
    let status = child
        .wait()
        .map_err(|_| ProcessRunFailure::TerminationUnavailable)?;
    if kill_failed {
        return Err(ProcessRunFailure::TerminationUnavailable);
    }
    Ok(status)
}

fn cleanup_failed_run(
    child: &mut GroupChild,
    receiver: Receiver<ReaderMessage>,
    readers: Vec<JoinHandle<()>>,
) -> Result<(), ProcessRunFailure> {
    let stop_result = stop_group(child);
    drop(receiver);
    let join_result = readers
        .into_iter()
        .try_for_each(join_reader)
        .map_err(map_output_error);
    stop_result?;
    join_result
}

fn drain_readers(
    receiver: Receiver<ReaderMessage>,
    readers: Vec<JoinHandle<()>>,
    stdout: &mut OutputCollector,
    stderr: &mut OutputCollector,
    emitter: &mut EventEmitter<'_>,
    mut stdout_done: bool,
    mut stderr_done: bool,
) -> Result<(), ProcessRunFailure> {
    while !stdout_done || !stderr_done {
        match receiver.recv() {
            Ok(ReaderMessage::Chunk(stream, bytes)) => {
                let collector = match stream {
                    ProcessStream::Stdout => &mut *stdout,
                    ProcessStream::Stderr => &mut *stderr,
                };
                let mut emit = |kind| emitter.emit_output(kind);
                if let Err(error) = collector.ingest(&bytes, &mut emit) {
                    drop(receiver);
                    readers
                        .into_iter()
                        .try_for_each(join_reader)
                        .map_err(map_output_error)?;
                    return Err(map_output_error(error));
                }
            }
            Ok(ReaderMessage::Eof(ProcessStream::Stdout)) if !stdout_done => stdout_done = true,
            Ok(ReaderMessage::Eof(ProcessStream::Stderr)) if !stderr_done => stderr_done = true,
            Ok(ReaderMessage::Eof(_)) | Ok(ReaderMessage::Failed) | Err(_) => {
                drop(receiver);
                readers
                    .into_iter()
                    .try_for_each(join_reader)
                    .map_err(map_output_error)?;
                return Err(ProcessRunFailure::OutputUnavailable);
            }
        }
    }
    drop(receiver);
    readers
        .into_iter()
        .try_for_each(join_reader)
        .map_err(map_output_error)
}

fn map_output_error(error: OutputCollectionError) -> ProcessRunFailure {
    match error {
        OutputCollectionError::ReaderFailed => ProcessRunFailure::OutputUnavailable,
        OutputCollectionError::EventUnavailable => ProcessRunFailure::EventUnavailable,
        OutputCollectionError::InvalidResult => ProcessRunFailure::InvalidResult,
    }
}

struct EventEmitter<'a> {
    specification_id: a3_domain::PolicyResourceId,
    next_sequence: u64,
    sink: &'a dyn ProcessEventSink,
}

impl<'a> EventEmitter<'a> {
    fn new(specification_id: a3_domain::PolicyResourceId, sink: &'a dyn ProcessEventSink) -> Self {
        Self {
            specification_id,
            next_sequence: 1,
            sink,
        }
    }

    fn emit(
        &mut self,
        kind: ProcessEventKind,
    ) -> Result<(), a3_application::ProcessEventSinkError> {
        let sequence = ProcessEventSequence::new(self.next_sequence)
            .map_err(|_| a3_application::ProcessEventSinkError)?;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(a3_application::ProcessEventSinkError)?;
        self.sink
            .emit(ProcessEvent::new(self.specification_id, sequence, kind))
    }

    fn emit_output(&mut self, kind: ProcessEventKind) -> Result<(), OutputCollectionError> {
        self.emit(kind)
            .map_err(|_| OutputCollectionError::EventUnavailable)
    }
}
