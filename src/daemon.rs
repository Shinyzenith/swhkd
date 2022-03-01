use clap::{arg, Command};
use evdev::{AttributeSet, AutoRepeat, Device, InputEventKind, Key};
use nix::unistd::{Group, Uid};
use signal_hook_tokio::Signals;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::prelude::*,
    os::unix::net::UnixStream,
    path::Path,
    process::{exit, id},
};
use sysinfo::{ProcessExt, System, SystemExt};
use tokio::select;
use tokio::time::Duration;
use tokio::time::{sleep, Instant};
use tokio_stream::{StreamExt, StreamMap};

use signal_hook::consts::signal::*;

mod config;

#[cfg(test)]
mod tests;

struct KeyboardState {
    state_modifiers: HashSet<config::Modifier>,
    state_keysyms: AttributeSet<evdev::Key>,
}

impl KeyboardState {
    fn new() -> KeyboardState {
        KeyboardState { state_modifiers: HashSet::new(), state_keysyms: AttributeSet::new() }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = set_flags().get_matches();
    env::set_var("RUST_LOG", "swhkd=warn");

    if args.is_present("debug") {
        env::set_var("RUST_LOG", "swhkd=trace");
    }

    env_logger::init();
    log::trace!("Logger initialized.");

    let pidfile: String = String::from("/tmp/swhkd.pid");
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
            if pid.to_string() == swhkd_pid && process.exe() == env::current_exe().unwrap() {
                log::error!("Swhkd is already running!");
                exit(1);
            }
        }
    }

    match fs::write(&pidfile, id().to_string()) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Unable to write to {}: {}", pidfile, e);
            exit(1);
        }
    }

    permission_check();

    let load_config = || {
        let config_file_path: std::path::PathBuf = if args.is_present("config") {
            Path::new(args.value_of("config").unwrap()).to_path_buf()
        } else {
            check_config_xdg()
        };
        log::debug!("Using config file path: {:#?}", config_file_path);

        if !config_file_path.exists() {
            log::error!("{:#?} doesn't exist", config_file_path);
            exit(1);
        }

        let hotkeys = match config::load(config_file_path) {
            Err(e) => {
                log::error!("Config Error: {}", e);
                exit(1);
            }
            Ok(out) => out,
        };
        for hotkey in &hotkeys {
            log::debug!("hotkey: {:#?}", hotkey);
        }
        hotkeys
    };

    let mut hotkeys = load_config();

    log::trace!("Attempting to find all keyboard file descriptors.");
    let keyboard_devices: Vec<Device> = evdev::enumerate().filter(check_keyboard).collect();

    if keyboard_devices.is_empty() {
        log::error!("No valid keyboard device was detected!");
        exit(1);
    }
    log::debug!("{} Keyboard device(s) detected.", keyboard_devices.len());

    let modifiers_map: HashMap<Key, config::Modifier> = HashMap::from([
        (Key::KEY_LEFTMETA, config::Modifier::Super),
        (Key::KEY_RIGHTMETA, config::Modifier::Super),
        (Key::KEY_LEFTMETA, config::Modifier::Super),
        (Key::KEY_RIGHTMETA, config::Modifier::Super),
        (Key::KEY_LEFTALT, config::Modifier::Alt),
        (Key::KEY_RIGHTALT, config::Modifier::Alt),
        (Key::KEY_LEFTCTRL, config::Modifier::Control),
        (Key::KEY_RIGHTCTRL, config::Modifier::Control),
        (Key::KEY_LEFTSHIFT, config::Modifier::Shift),
        (Key::KEY_RIGHTSHIFT, config::Modifier::Shift),
    ]);

    let repeat_cooldown_duration: u64 = if args.is_present("cooldown") {
        args.value_of("cooldown").unwrap().parse::<u64>().unwrap()
    } else {
        250
    };

    fn send_command(hotkey: config::Hotkey) {
        log::info!("Hotkey pressed: {:#?}", hotkey);
        if let Err(e) = sock_send(&hotkey.command) {
            log::error!("Failed to send command over IPC.");
            log::error!("Is swhks running?");
            log::error!("{:#?}", e)
        }
    }

    let mut signals = Signals::new(&[SIGUSR1, SIGUSR2, SIGHUP, SIGINT])?;
    let mut paused = false;
    let mut temp_paused = false;

    let mut last_hotkey: Option<config::Hotkey> = None;
    let mut keyboard_states: Vec<KeyboardState> = Vec::new();
    let mut keyboard_stream_map = StreamMap::new();
    for (i, mut device) in keyboard_devices.into_iter().enumerate() {
        device.update_auto_repeat(&AutoRepeat { delay: 0, period: 0 })?;
        keyboard_stream_map.insert(i, device.into_event_stream()?);
        keyboard_states.push(KeyboardState::new());
    }

    // the initial sleep duration is never read because last_hotkey is initialized to None
    let hotkey_repeat_timer = sleep(Duration::from_millis(0));
    tokio::pin!(hotkey_repeat_timer);

    loop {
        select! {
            _ = &mut hotkey_repeat_timer, if &last_hotkey.is_some() => {
                let hotkey = last_hotkey.clone().unwrap();
                send_command(hotkey.clone());
                hotkey_repeat_timer.as_mut().reset(Instant::now() + Duration::from_millis(repeat_cooldown_duration));
            }
            Some(signal) = signals.next() => {
                match signal {
                    SIGUSR1 => {
                        paused = true;
                    }
                    SIGUSR2 => {
                        paused = false;
                    }
                    SIGHUP => {
                        hotkeys = load_config();
                    }
                    SIGINT => {
                        temp_paused = true;
                    }
                    _ => unreachable!()
                }
            }
            Some((i, Ok(event))) = keyboard_stream_map.next() => {
            let keyboard_state = &mut keyboard_states[i];
            if let InputEventKind::Key(key) = event.kind() {
                match event.value() {
                    1 => {
                        if let Some(modifier) = modifiers_map.get(&key) {
                            keyboard_state.state_modifiers.insert(*modifier);
                        } else {
                            keyboard_state.state_keysyms.insert(key);
                        }
                    }
                    0 => {
                        if let Some(modifier) = modifiers_map.get(&key) {
                            if let Some(hotkey) = &last_hotkey {
                                if hotkey.modifiers.contains(modifier) {
                                    last_hotkey = None;
                                }
                            }
                            keyboard_state.state_modifiers.remove(modifier);
                        } else if keyboard_state.state_keysyms.contains(key) {
                            if let Some(hotkey) = &last_hotkey {
                                if key == hotkey.keysym {
                                    last_hotkey = None;
                                }
                            }
                            keyboard_state.state_keysyms.remove(key);
                        }
                    }
                    _ => {}
                }

                if paused || last_hotkey.is_some() {
                    continue;
                }

                let possible_hotkeys: Vec<&config::Hotkey> = hotkeys.iter()
                    .filter(|hotkey| hotkey.modifiers.len() == keyboard_state.state_modifiers.len())
                    .collect();

                if possible_hotkeys.is_empty() {
                    continue;
                }

                log::debug!("state_modifiers: {:#?}", keyboard_state.state_modifiers);
                log::debug!("state_keysyms: {:#?}", keyboard_state.state_keysyms);
                log::debug!("hotkey: {:#?}", possible_hotkeys);
                if temp_paused {
                    if keyboard_state.state_modifiers.iter().all(|x| {
                        vec![config::Modifier::Shift, config::Modifier::Super].contains(x)
                    }) && keyboard_state.state_keysyms.contains(evdev::Key::KEY_ESC)
                    {
                        temp_paused = false;
                    }
                    continue;
                }

                for hotkey in possible_hotkeys {
                    // this should check if state_modifiers and hotkey.modifiers have the same elements
                    if keyboard_state.state_modifiers.iter().all(|x| hotkey.modifiers.contains(x))
                        && keyboard_state.state_modifiers.len() == hotkey.modifiers.len()
                        && keyboard_state.state_keysyms.contains(hotkey.keysym)
                    {
                        last_hotkey = Some(hotkey.clone());
                        send_command(hotkey.clone());
                        hotkey_repeat_timer.as_mut().reset(Instant::now() + Duration::from_millis(repeat_cooldown_duration));
                        break;
                    }
                }
            }
        }
        }
    }
}

pub fn permission_check() {
    if !Uid::current().is_root() {
        let groups = nix::unistd::getgroups();
        for (_, groups) in groups.iter().enumerate() {
            for group in groups {
                let group = Group::from_gid(*group);
                if group.unwrap().unwrap().name == "input" {
                    log::error!("Note: INVOKING USER IS IN INPUT GROUP!!!!");
                    log::error!("THIS IS A HUGE SECURITY RISK!!!!");
                }
            }
        }
        log::error!("Consider using `pkexec swhkd ...`");
        exit(1);
    } else {
        log::warn!("Running swhkd as root!");
    }
}

pub fn check_keyboard(device: &Device) -> bool {
    if device.supported_keys().map_or(false, |keys| keys.contains(Key::KEY_ENTER)) {
        log::debug!("{} is a keyboard.", device.name().unwrap(),);
        true
    } else {
        log::trace!("{} is not a keyboard.", device.name().unwrap(),);
        false
    }
}

pub fn set_flags() -> Command<'static> {
    let app = Command::new("swhkd")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Simple Wayland HotKey Daemon")
        .arg(
            arg!(-c --config <CONFIG_FILE_PATH>)
                .required(false)
                .takes_value(true)
                .help("Set a custom config file path."),
        )
        .arg(
            arg!(-C --cooldown <COOLDOWN_IN_MS>)
                .required(false)
                .takes_value(true)
                .help("Set a custom repeat cooldown duration. Default is 250ms."),
        )
        .arg(arg!(-d - -debug).required(false).help("Enable debug mode."));
    app
}

pub fn check_config_xdg() -> std::path::PathBuf {
    let config_file_path: std::path::PathBuf;
    match env::var("XDG_CONFIG_HOME") {
        Ok(val) => {
            config_file_path = Path::new(&val).join("swhkd/swhkdrc");
            log::debug!("XDG_CONFIG_HOME exists: {:#?}", val);
            return config_file_path;
        }
        Err(_) => {
            log::error!("XDG_CONFIG_HOME has not been set.");
            config_file_path = Path::new("/etc/swhkd/swhkdrc").to_path_buf();
            log::warn!(
                "Note: Due to the design of the application, the invoking user is always root."
            );
            log::warn!("You can set a custom config file with the -c option.");
            log::warn!("Adding your user to the input group could solve this.");
            log::warn!("However that's a massive security flaw and basically defeats the purpose of using wayland.");
            log::warn!("The following issue may be addressed in the future, but it is certainly not a priority right now.");
        }
    }
    config_file_path
}

fn sock_send(command: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect("/tmp/swhkd.sock")?;
    stream.write_all(command.as_bytes())?;
    Ok(())
}
