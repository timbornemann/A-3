use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let mode = arguments.next().ok_or("missing fixture mode")?;
    match mode.as_str() {
        "echo" => {
            let value = arguments.next().ok_or("missing echo value")?;
            println!("{value}");
        }
        "cwd" => {
            let value = fs::read_to_string("marker.txt")?;
            print!("{value}");
        }
        "environment" => {
            let allowed = env::var("A3_ALLOWED")?;
            let path_present = env::var_os("PATH").is_some();
            println!("{allowed};path={path_present}");
        }
        "temp_environment" => {
            println!("temp={};tmp={};tmpdir={};path={}",
                env::var_os("TEMP").is_some(), env::var_os("TMP").is_some(),
                env::var_os("TMPDIR").is_some(), env::var_os("PATH").is_some());
        }
        "hang" => loop {
            thread::sleep(Duration::from_secs(1));
        },
        "overflow" => {
            let bytes: usize = arguments.next().ok_or("missing byte count")?.parse()?;
            let mut remaining = bytes;
            let block = [b'x'; 16 * 1_024];
            let mut stdout = io::stdout().lock();
            while remaining > 0 {
                let write = remaining.min(block.len());
                stdout.write_all(&block[..write])?;
                remaining -= write;
            }
            stdout.flush()?;
        }
        "secret-output" => {
            println!("token=fixture-secret-value");
        }
        "spawn-child" => {
            let pid_file = arguments.next().ok_or("missing pid file")?;
            let executable = env::current_exe()?;
            let child = Command::new(executable).arg("child").spawn()?;
            write_pid(Path::new(&pid_file), child.id())?;
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        "spawn-child-and-exit" => {
            let pid_file = arguments.next().ok_or("missing child pid file")?;
            let parent_pid_file = arguments.next().ok_or("missing parent pid file")?;
            write_pid(Path::new(&parent_pid_file), std::process::id())?;
            let mut child = Command::new(env::current_exe()?)
                .arg("hold-inherited-pipes")
                .arg(&pid_file)
                .spawn()?;
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                if fs::read_to_string(&pid_file).ok()
                    .and_then(|text| text.strip_suffix('\n')?.parse::<u32>().ok()) == Some(child.id())
                {
                    break;
                }
                if child.try_wait()?.is_some() || Instant::now() >= deadline {
                    return Err("pipe-owning descendant did not become ready".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            // Only exit after the descendant has actually flushed both pipe markers.
        }
        "hold-inherited-pipes" => {
            let pid_file = arguments.next().ok_or("missing child pid file")?;
            let mut stdout = io::stdout().lock();
            let mut stderr = io::stderr().lock();
            stdout.write_all(b"pipe-held-stdout\n")?;
            stderr.write_all(b"pipe-held-stderr\n")?;
            stdout.flush()?;
            stderr.flush()?;
            write_pid(Path::new(&pid_file), std::process::id())?;
            // Separate finite backstop, much later than the product deadline.
            thread::sleep(Duration::from_secs(12));
            drop(stderr);
            drop(stdout);
        }
        "child" => loop {
            thread::sleep(Duration::from_secs(1));
        },
        _ => return Err("unknown fixture mode".into()),
    }
    Ok(())
}

fn write_pid(path: &Path, pid: u32) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "{pid}")?;
    file.sync_all()
}
