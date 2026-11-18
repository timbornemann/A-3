use super::Ownership;
use a3_application::ProcessRunFailure;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4},
    path::Path,
    time::{Duration, Instant},
};

const MAX_TABLE_BYTES: usize = 4 * 1024 * 1024;
const MAX_STAT_BYTES: usize = 4 * 1024;
const MAX_FILE_DESCRIPTORS: usize = 4_096;
const MAX_MATCHING_SOCKETS: usize = 64;

pub(super) fn configure_probe_close(stream: &tokio::net::TcpStream) -> io::Result<()> {
    // The caller already proved this private client's server-side tuple is owned.
    // Abortive disposal avoids leaving our server port in TIME_WAIT, which would
    // prevent the next non-reusing exclusive bind after complete process cleanup.
    stream.set_zero_linger()
}

pub(super) fn poll_probe_connection(
    connection: &mut super::http::ProbeConnection,
    context: &mut std::task::Context<'_>,
) -> std::task::Poll<Result<(), hyper::Error>> {
    // A prior FIN defeats zero-linger disposal. Hyper still drives all HTTP I/O
    // and reports errors; the separate response future must collect the complete
    // bounded body before success. Socket disposal never proves process cleanup.
    connection.poll_without_shutdown(context)
}

/// A non-reaped direct child remains the identity anchor of its process group.
pub(super) enum LinuxObservedExit {
    Code(u32),
    Signal,
    OwnerLost,
}

pub(super) fn poll_exit_before_stop(
    child_id: u32,
) -> Result<Option<LinuxObservedExit>, ProcessRunFailure> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};

    let pid = i32::try_from(child_id)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or(ProcessRunFailure::InvalidResult)?;
    match waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    ) {
        Ok(Some(status)) => Ok(Some(match status.exit_status() {
            Some(code) => LinuxObservedExit::Code(
                u32::try_from(code).map_err(|_| ProcessRunFailure::InvalidResult)?,
            ),
            None => LinuxObservedExit::Signal,
        })),
        Ok(None) | Err(rustix::io::Errno::INTR) => Ok(None),
        Err(rustix::io::Errno::CHILD) => Ok(Some(LinuxObservedExit::OwnerLost)),
        Err(_) => Err(ProcessRunFailure::InvalidResult),
    }
}

/// Owns a bound endpoint without accepting connections or allowing address reuse.
pub(super) struct ExclusiveLoopbackBind {
    _socket: tokio::net::TcpSocket,
}

impl ExclusiveLoopbackBind {
    pub(super) fn bind(port: std::num::NonZeroU16) -> io::Result<Self> {
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.get()).into())?;
        Ok(Self { _socket: socket })
    }
}

pub(super) fn observe(
    owner_pid: u32,
    local: SocketAddrV4,
    peer: Option<SocketAddrV4>,
    remaining: Duration,
) -> Result<Ownership, ProcessRunFailure> {
    let deadline = deadline(local, peer, remaining)?;
    if owner_pid == 0 {
        return Err(ProcessRunFailure::InvalidResult);
    }
    let process = format!("/proc/{owner_pid}");
    let stat = Path::new(&process).join("stat");
    let start = read_child_start(&stat, owner_pid, deadline)?;
    let matches = inspect(local, peer, deadline)?;
    let ownership = if matches.foreign || (matches.pending && !matches.inodes.is_empty()) {
        Ownership::Foreign
    } else if matches.inodes.is_empty() {
        Ownership::Absent
    } else if owns_sockets(&Path::new(&process).join("fd"), matches.inodes, deadline)? {
        Ownership::Owned
    } else {
        Ownership::Foreign
    };
    if read_child_start(&stat, owner_pid, deadline)? != start {
        return Err(ProcessRunFailure::InvalidResult);
    }
    Ok(ownership)
}

pub(super) fn listener_absent(
    local: SocketAddrV4,
    remaining: Duration,
) -> Result<bool, ProcessRunFailure> {
    let matches = inspect(local, None, deadline(local, None, remaining)?)?;
    Ok(!matches.foreign && matches.inodes.is_empty())
}

fn deadline(
    local: SocketAddrV4,
    peer: Option<SocketAddrV4>,
    remaining: Duration,
) -> Result<Instant, ProcessRunFailure> {
    if !local.ip().is_loopback()
        || local.port() == 0
        || peer.is_some_and(|address| !address.ip().is_loopback() || address.port() == 0)
    {
        return Err(ProcessRunFailure::InvalidResult);
    }
    Instant::now()
        .checked_add(remaining)
        .ok_or(ProcessRunFailure::InvalidResult)
}

#[derive(Default)]
struct Matches {
    inodes: BTreeSet<u64>,
    foreign: bool,
    pending: bool,
}

#[derive(Clone, Copy)]
enum Family {
    V4,
    V6,
}

fn inspect(
    local: SocketAddrV4,
    peer: Option<SocketAddrV4>,
    deadline: Instant,
) -> Result<Matches, ProcessRunFailure> {
    let mut matches = Matches::default();
    // The client connects in our network namespace. The child could have changed its own.
    for (path, family) in [
        ("/proc/self/net/tcp", Family::V4),
        ("/proc/self/net/tcp6", Family::V6),
    ] {
        let data = match read_bounded(Path::new(path), MAX_TABLE_BYTES, deadline) {
            Ok(data) => data,
            Err(error)
                if matches!(family, Family::V6) && error.kind() == io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(_) => return Err(ProcessRunFailure::InvalidResult),
        };
        parse_table(&data, family, local, peer, deadline, &mut matches)
            .ok_or(ProcessRunFailure::InvalidResult)?;
    }
    Ok(matches)
}

fn parse_table(
    data: &[u8],
    family: Family,
    local: SocketAddrV4,
    peer: Option<SocketAddrV4>,
    deadline: Instant,
    matches: &mut Matches,
) -> Option<()> {
    let mut lines = std::str::from_utf8(data).ok()?.lines();
    let mut header = lines.next()?.split_ascii_whitespace();
    if header.next()? != "sl"
        || header.next()? != "local_address"
        || !matches!(header.next()?, "rem_address" | "remote_address")
        || header.next()? != "st"
    {
        return None;
    }
    for line in lines {
        check_time(deadline).ok()?;
        let mut fields = line.split_ascii_whitespace();
        fields.next()?.strip_suffix(':')?.parse::<u32>().ok()?;
        let (address, port) = parse_endpoint(fields.next()?, family)?;
        let (remote, remote_port) = parse_endpoint(fields.next()?, family)?;
        let state = fields.next()?;
        if state.len() != 2 {
            return None;
        }
        let state = u8::from_str_radix(state, 16).ok()?;
        let (relevant, exact) = match peer {
            None => (
                state == 0x0a
                    && port == local.port()
                    && (address.is_unspecified()
                        || ipv4(address)
                            .is_some_and(|ip| ip.is_unspecified() || ip == *local.ip())),
                address == IpAddr::V4(*local.ip()),
            ),
            Some(peer) => (
                state == 0x01
                    && port == local.port()
                    && ipv4(address) == Some(*local.ip())
                    && remote_port == peer.port()
                    && ipv4(remote) == Some(*peer.ip()),
                matches!(family, Family::V4),
            ),
        };
        if !relevant {
            continue;
        }
        let inode = fields.nth(5)?.parse::<u64>().ok()?;
        if !exact || (inode == 0 && peer.is_none()) {
            matches.foreign = true;
        } else if inode == 0 {
            // Linux reports an established server tuple before accept installs its FD.
            matches.pending = true;
        } else if matches.inodes.insert(inode) && matches.inodes.len() > MAX_MATCHING_SOCKETS {
            return None;
        }
    }
    check_time(deadline).ok()
}

fn parse_endpoint(field: &str, family: Family) -> Option<(IpAddr, u16)> {
    let (address, port) = field.split_once(':')?;
    if port.len() != 4 {
        return None;
    }
    let port = u16::from_str_radix(port, 16).ok()?;
    let address = match family {
        Family::V4 if address.len() == 8 => IpAddr::V4(Ipv4Addr::from(
            u32::from_str_radix(address, 16).ok()?.to_ne_bytes(),
        )),
        Family::V6 if address.len() == 32 => {
            // tcp6 exposes four native-endian 32-bit words in network word order.
            let mut bytes = [0; 16];
            for (word, destination) in bytes.chunks_exact_mut(4).enumerate() {
                let offset = word * 8;
                let value = u32::from_str_radix(address.get(offset..offset + 8)?, 16).ok()?;
                destination.copy_from_slice(&value.to_ne_bytes());
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    Some((address, port))
}

fn ipv4(address: IpAddr) -> Option<Ipv4Addr> {
    match address {
        IpAddr::V4(address) => Some(address),
        IpAddr::V6(address) => address.to_ipv4_mapped(),
    }
}

fn read_child_start(path: &Path, pid: u32, deadline: Instant) -> Result<u64, ProcessRunFailure> {
    let data = read_bounded(path, MAX_STAT_BYTES, deadline)
        .map_err(|_| ProcessRunFailure::InvalidResult)?;
    child_start(&data, pid, std::process::id()).ok_or(ProcessRunFailure::InvalidResult)
}

fn child_start(data: &[u8], pid: u32, parent_pid: u32) -> Option<u64> {
    // The comm field can contain parentheses, newlines, and non-UTF-8 bytes.
    let open = data.iter().position(|&byte| byte == b'(')?;
    let close = data.iter().rposition(|&byte| byte == b')')?;
    if close <= open
        || std::str::from_utf8(data.get(..open)?)
            .ok()?
            .trim_end()
            .parse::<u32>()
            .ok()?
            != pid
    {
        return None;
    }
    let mut tail = std::str::from_utf8(data.get(close + 1..)?)
        .ok()?
        .split_ascii_whitespace();
    let state = tail.next()?;
    if state.len() != 1
        || matches!(state, "Z" | "X" | "x")
        || tail.next()?.parse::<u32>().ok()? != parent_pid
        || tail.next()?.parse::<u32>().ok()? != pid
    {
        return None;
    }
    tail.nth(16)?.parse::<u64>().ok()
}

fn owns_sockets(
    path: &Path,
    mut missing: BTreeSet<u64>,
    deadline: Instant,
) -> Result<bool, ProcessRunFailure> {
    let entries = fs::read_dir(path).map_err(|_| ProcessRunFailure::InvalidResult)?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_FILE_DESCRIPTORS || check_time(deadline).is_err() {
            return Err(ProcessRunFailure::InvalidResult);
        }
        let entry = entry.map_err(|_| ProcessRunFailure::InvalidResult)?;
        let target = match fs::read_link(entry.path()) {
            Ok(target) => target,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(ProcessRunFailure::InvalidResult),
        };
        if let Some(inode) = target.to_str().and_then(socket_inode) {
            missing.remove(&inode);
        }
    }
    Ok(missing.is_empty())
}

fn socket_inode(target: &str) -> Option<u64> {
    let digits = target.strip_prefix("socket:[")?.strip_suffix(']')?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok().filter(|inode| *inode != 0)
}

fn read_bounded(path: &Path, limit: usize, deadline: Instant) -> io::Result<Vec<u8>> {
    check_time(deadline)?;
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        check_time(deadline)?;
        let maximum = buffer.len().min(limit + 1 - bytes.len());
        let count = file.read(&mut buffer[..maximum])?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > limit {
            return Err(io::ErrorKind::InvalidData.into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn check_time(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n";
    const SERVER: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 18765);
    const CLIENT: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 43210);

    fn endpoint(address: SocketAddrV4) -> String {
        format!(
            "{:08X}:{:04X}",
            u32::from_ne_bytes(address.ip().octets()),
            address.port()
        )
    }

    fn row(local: &str, peer: &str, state: &str, inode: u64) -> String {
        format!("0: {local} {peer} {state} 00000000:00000000 00:00000000 00000000 1000 0 {inode}\n")
    }

    fn scan(rows: &str, family: Family, peer: Option<SocketAddrV4>) -> Option<Matches> {
        let mut found = Matches::default();
        parse_table(
            format!("{HEADER}{rows}").as_bytes(),
            family,
            SERVER,
            peer,
            Instant::now() + Duration::from_secs(2),
            &mut found,
        )?;
        Some(found)
    }

    #[test]
    fn listener_distinguishes_exact_ip_and_competing_wildcard() -> Result<(), &'static str> {
        let exact = row(&endpoint(SERVER), "00000000:0000", "0A", 123);
        let unrelated = row(
            &endpoint(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, 2),
                SERVER.port(),
            )),
            "00000000:0000",
            "0A",
            234,
        );
        let found = scan(&format!("{unrelated}{exact}"), Family::V4, None).ok_or("table")?;
        assert_eq!(found.inodes, BTreeSet::from([123]));
        assert!(!found.foreign);
        let wildcard = row(
            &endpoint(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SERVER.port())),
            "00000000:0000",
            "0A",
            345,
        );
        assert!(
            scan(&format!("{exact}{wildcard}"), Family::V4, None)
                .ok_or("table")?
                .foreign
        );
        let dual_stack = row(
            &format!("{:032X}:{:04X}", 0, SERVER.port()),
            "00000000000000000000000000000000:0000",
            "0A",
            456,
        );
        assert!(scan(&dual_stack, Family::V6, None).ok_or("table")?.foreign);
        Ok(())
    }

    #[test]
    fn accepted_tuple_requires_exact_peer_and_a_nonzero_inode() -> Result<(), &'static str> {
        let queued = row(&endpoint(SERVER), &endpoint(CLIENT), "01", 0);
        let found = scan(&queued, Family::V4, Some(CLIENT)).ok_or("table")?;
        assert!(found.pending && found.inodes.is_empty());
        let accepted = row(&endpoint(SERVER), &endpoint(CLIENT), "01", 123);
        let reverse = row(&endpoint(CLIENT), &endpoint(SERVER), "01", 234);
        let other = row(
            &endpoint(SERVER),
            &endpoint(SocketAddrV4::new(Ipv4Addr::LOCALHOST, CLIENT.port() + 1)),
            "01",
            345,
        );
        let found = scan(
            &format!("{accepted}{reverse}{other}"),
            Family::V4,
            Some(CLIENT),
        )
        .ok_or("table")?;
        assert_eq!(found.inodes, BTreeSet::from([123]));
        assert!(!found.pending && !found.foreign);
        assert!(scan("0: 0100007F:494D invalid 01\n", Family::V4, Some(CLIENT)).is_none());
        Ok(())
    }

    #[test]
    fn stat_and_socket_links_do_not_accept_spoofed_owners() {
        let stat = |state: &str, group: u32| {
            format!(
                "42 (name )\n with parentheses) {state} 12 {group} {} 345 0\n",
                "0 ".repeat(16)
            )
        };
        assert_eq!(child_start(stat("S", 42).as_bytes(), 42, 12), Some(345));
        assert_eq!(child_start(stat("S", 43).as_bytes(), 42, 12), None);
        assert_eq!(child_start(stat("S", 42).as_bytes(), 42, 13), None);
        assert_eq!(child_start(stat("S", 42).as_bytes(), 41, 12), None);
        assert_eq!(child_start(stat("Z", 42).as_bytes(), 42, 12), None);
        assert_eq!(socket_inode("socket:[345]"), Some(345));
        assert_eq!(socket_inode("/tmp/socket:[345]"), None);
        assert_eq!(socket_inode("socket:[+345]"), None);
        assert_eq!(socket_inode("socket:[0]"), None);
    }
}
