use std::io::prelude::*;
use std::os::unix::net::UnixListener;
use std::{
    env, fs,
    path::Path,
    process::{exit, id, Command, Stdio},
};
use sysinfo::{ProcessExt, System, SystemExt};

mod config;

fn main() -> std::io::Result<()> {
    env::set_var("RUST_LOG", "swhks=trace");
    env_logger::init();

    let pidfile: String = String::from("/tmp/swhks.pid");
    let sockfile: String = String::from("/tmp/swhkd.sock");

    if Path::new(&pidfile).exists() {
        log::trace!("Reading {} file and checking for running instances.", pidfile);
        let swhkd_pid = match fs::read_to_string(&pidfile) {
            Ok(swhkd_pid) => swhkd_pid,
            Err(e) => {
                log::error!("Unable to read {} to check all running instances", e);
                exit(1);
            }
        };
        log::debug!("Previous PID: {}", swhkd_pid);

        let mut sys = System::new_all();
        sys.refresh_all();
        for (pid, process) in sys.processes() {
            if pid.to_string() == swhkd_pid {
                if process.exe() == env::current_exe().unwrap() {
                    log::error!("Server is already running!");
                    exit(1);
                }
            }
        }
    }

    if Path::new(&sockfile).exists() {
        log::trace!("Sockfile exists, attempting to remove it.");
        match fs::remove_file(&sockfile) {
            Ok(_) => {
                log::debug!("Removed old socket file");
            }
            Err(e) => {
                log::error!("Error removing the socket file!: {}", e);
                log::error!("You can manually remove the socket file: {}", sockfile);
                exit(1);
            }
        };
    }

    match fs::write(&pidfile, id().to_string()) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Unable to write to {}: {}", pidfile, e);
            exit(1);
        }
    }

    fn run_system_command(command: &String) -> () {
        match Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to execute {}", command);
                log::error!("Error, {}", e);
            }
        }
    }

    let listener = UnixListener::bind(sockfile)?;
    loop {
        match listener.accept() {
            Ok((mut socket, addr)) => {
                let mut response = String::new();
                socket.read_to_string(&mut response)?;
                run_system_command(&response);
                log::debug!("Socket: {:?} Address: {:?} Response: {}", socket, addr, response);
            }
            Err(e) => log::error!("accept function failed: {:?}", e),
        }
    }
}
