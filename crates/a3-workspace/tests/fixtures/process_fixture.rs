use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

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
            let _child = Command::new(executable).arg("child").arg(pid_file).spawn()?;
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        "child" => {
            let pid_file = arguments.next().ok_or("missing pid file")?;
            write_pid(Path::new(&pid_file))?;
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        _ => return Err("unknown fixture mode".into()),
    }
    Ok(())
}

fn write_pid(path: &Path) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "{}", std::process::id())?;
    file.sync_all()
}
