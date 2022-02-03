use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use std::{
    env, fs,
    io::{self, prelude::*, BufReader},
    path::Path,
    process::{exit, id, Command, Stdio},
};
use sysinfo::{ProcessExt, System, SystemExt};

// Necessary so we can test the config file
mod config;

fn main() {
    env::set_var("RUST_LOG", "swhks=trace");
    env_logger::init();

    let pidfile: String = String::from("/tmp/swhkc.pid");
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

    fn handle_error(connection: io::Result<LocalSocketStream>) -> Option<LocalSocketStream> {
        connection.map_err(|error| log::error!("Incoming connection failed: {}", error)).ok()
    }

    fn run_system_command(command: &str) {
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

    let listener = match LocalSocketListener::bind(sockfile.clone()) {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("Failed to connect to {}", sockfile);
            log::error!("Error: {}", e);
            exit(1);
        }
    };
    for conn in listener.incoming().filter_map(handle_error) {
        let mut conn = BufReader::new(conn);
        let mut buffer = String::new();
        match conn.read_line(&mut buffer) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to read incoming command from client.");
                log::error!("Error: {}", e);
                exit(1);
            }
        };
        log::debug!("Recieved command : {}", buffer);
        run_system_command(&buffer);
    }
}
