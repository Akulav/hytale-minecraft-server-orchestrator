//! HyOrchestrator - run and manage Hytale servers from one place. Made by Akulav.
//!
//! One program, two faces: with a display it opens a desktop window; without one (a Linux server over
//! SSH, `--headless`) it serves the same UI on a local web server and prints where to open it.

#![cfg_attr(all(windows, feature = "desktop", not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod archive;
mod backup;
#[cfg(feature = "desktop")]
mod desktop;
mod download;
mod instance;
mod java;
mod process;
mod state;
mod telemetry;
mod ui;
mod updater;
mod versions;
mod web;

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::OnceLock;

use state::{AppState, Mode, Paths, Shared, VERSION};

const HELP: &str = "HyOrchestrator - run and manage Hytale servers. Made by Akulav.

Usage: hyorchestrator [options]

  --headless          serve the web UI only (default when there is no display)
  --desktop           open the desktop window even if no display was detected
  --port <n>          web UI port (default 7520, or the one in Settings)
  --host <ip>         address the web UI listens on (headless default 0.0.0.0, desktop 127.0.0.1)
  --data-dir <path>   where instances, versions and backups live
  --print-url         print the web UI address (with its access token) and quit
  --version           print the version and quit
  --help              this text
";

#[derive(Debug, Default, PartialEq)]
struct Args {
    headless: bool,
    desktop: bool,
    port: Option<u16>,
    host: Option<IpAddr>,
    data_dir: Option<PathBuf>,
    print_url: bool,
    version: bool,
    help: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--headless" => a.headless = true,
            "--desktop" => a.desktop = true,
            "--port" => a.port = Some(value("--port")?.parse().map_err(|_| "--port needs a number".to_string())?),
            "--host" => a.host = Some(value("--host")?.parse().map_err(|_| "--host needs an IP address".to_string())?),
            "--data-dir" => a.data_dir = Some(PathBuf::from(value("--data-dir")?)),
            "--print-url" => a.print_url = true,
            "--version" | "-V" => a.version = true,
            "--help" | "-h" => a.help = true,
            "--after-update" => {}
            other => return Err(format!("unknown option {other} (see --help)")),
        }
    }
    Ok(a)
}

/// Is there a screen to open a window on?
fn has_display() -> bool {
    if cfg!(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd")) {
        ["DISPLAY", "WAYLAND_DISPLAY"].iter().any(|v| std::env::var_os(v).is_some_and(|x| !x.is_empty()))
    } else if cfg!(target_os = "macos") {
        // an SSH session cannot open windows on the Mac's screen
        std::env::var_os("SSH_CONNECTION").is_none() && std::env::var_os("SSH_TTY").is_none()
    } else {
        true
    }
}

fn pick_mode(args: &Args) -> Mode {
    if !cfg!(feature = "desktop") || args.headless || args.print_url {
        Mode::Headless
    } else if args.desktop || has_display() {
        Mode::Desktop
    } else {
        Mode::Headless
    }
}

// --- log file -------------------------------------------------------------------------------------

static LOG: OnceLock<parking_lot::Mutex<Option<std::fs::File>>> = OnceLock::new();

fn init_log(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("hyorchestrator.log");
    // keep one previous log
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 5 * 1024 * 1024) {
        let _ = std::fs::rename(&path, dir.join("hyorchestrator.old.log"));
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
    let _ = LOG.set(parking_lot::Mutex::new(file));
}

/// Writes a line to the log file (and stderr).
pub fn logln(msg: &str) {
    let line = format!("{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    eprintln!("{line}");
    if let Some(m) = LOG.get()
        && let Some(f) = m.lock().as_mut()
    {
        let _ = writeln!(f, "{line}");
    }
}

/// A release build for Windows has no console; reuse the one it was started from (if any) so
/// `--headless`, `--version` and `--print-url` can print.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// This computer's address on the local network (no packet is sent).
fn lan_ip() -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn urls_for(host: IpAddr, port: u16, token: &str) -> Vec<String> {
    let mut out = vec![format!("http://127.0.0.1:{port}/?token={token}")];
    if host.is_unspecified() {
        if let Some(ip) = lan_ip() {
            out.push(format!("http://{ip}:{port}/?token={token}"));
        }
    } else if !host.is_loopback() {
        out = vec![format!("http://{host}:{port}/?token={token}")];
    }
    out
}

fn banner(state: &Shared) {
    let urls = state.urls.read().clone();
    let line = "─".repeat(68);
    println!("\n {line}");
    println!("  HyOrchestrator {VERSION} · Made by Akulav");
    println!("  Headless mode: the web UI is served here instead of a window.");
    println!(" {line}");
    for (i, u) in urls.iter().enumerate() {
        println!("  {}  {u}", if i == 0 { "Local:  " } else { "Network:" });
    }
    println!(" {line}");
    println!("  Data:     {}", state.paths.root.display());
    println!("  Press Ctrl+C to quit (running servers are stopped cleanly).");
    println!(" {line}\n");
    let _ = std::io::stdout().flush();
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(a) => a,
        Err(e) => {
            attach_console();
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mode = pick_mode(&args);
    if args.help || args.version || mode == Mode::Headless {
        attach_console();
    }
    if args.help {
        print!("{HELP}");
        return;
    }
    if args.version {
        println!("hyorchestrator {VERSION}");
        return;
    }

    let paths = Paths::new(args.data_dir.clone().unwrap_or_else(Paths::default_root));
    init_log(&paths.logs);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("hyo").build().expect("tokio runtime");
    let state = match AppState::load(paths, mode, runtime.handle().clone()) {
        Ok(s) => s,
        Err(e) => {
            logln(&format!("cannot start: {e:#}"));
            std::process::exit(1);
        }
    };
    let (port, host) = {
        let s = state.settings.read();
        let host = args.host.unwrap_or(match mode {
            Mode::Headless => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            Mode::Desktop if s.lan_access => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            Mode::Desktop => IpAddr::V4(Ipv4Addr::LOCALHOST),
        });
        (args.port.unwrap_or(s.web_port), host)
    };
    let token = state.settings.read().web_token.clone();
    if args.print_url {
        for u in urls_for(host, port, &token) {
            println!("{u}");
        }
        return;
    }

    logln(&format!("HyOrchestrator {VERSION} starting ({mode:?}), data in {}", state.paths.root.display()));
    updater::cleanup(&state);
    let addr = match runtime.block_on(web::serve(state.clone(), host, port)) {
        Ok(a) => a,
        Err(e) => {
            logln(&format!("cannot start the web server: {e:#}"));
            std::process::exit(1);
        }
    };
    *state.urls.write() = urls_for(host, addr.port(), &token);
    logln(&format!("web UI on {addr}"));

    telemetry::spawn(state.clone());
    updater::spawn_background(state.clone());
    {
        let st = state.clone();
        runtime.spawn(async move {
            java::detect(&st).await;
        });
    }

    match mode {
        #[cfg(feature = "desktop")]
        Mode::Desktop => {
            let url = format!("http://127.0.0.1:{}/?token={token}", addr.port());
            // the runtime keeps running on its worker threads while the window owns the main thread
            let _guard = runtime.enter();
            desktop::run(state, url);
        }
        _ => {
            banner(&state);
            runtime.block_on(async {
                let st = state.clone();
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = st.quit.notified() => {}
                    _ = terminate() => {}
                }
                st.quitting.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("Stopping servers…");
                process::stop_all(&st).await;
            });
            logln("bye");
        }
    }
}

#[cfg(unix)]
async fn terminate() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut s) => {
            s.recv().await;
        }
        Err(_) => std::future::pending::<()>().await,
    }
}

#[cfg(not(unix))]
async fn terminate() {
    std::future::pending::<()>().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_command_line() {
        let a = parse_args(&s(&["--headless", "--port", "9000", "--host", "0.0.0.0", "--data-dir", "/srv/hyo"])).unwrap();
        assert!(a.headless);
        assert_eq!(a.port, Some(9000));
        assert_eq!(a.host, Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert_eq!(a.data_dir, Some(PathBuf::from("/srv/hyo")));
        assert!(parse_args(&s(&["--port"])).is_err());
        assert!(parse_args(&s(&["--bogus"])).is_err());
        assert_eq!(parse_args(&s(&["--after-update"])).unwrap(), Args::default());
    }

    #[test]
    fn headless_flag_wins() {
        let a = Args { headless: true, desktop: true, ..Default::default() };
        assert_eq!(pick_mode(&a), Mode::Headless);
        if !cfg!(feature = "desktop") {
            assert_eq!(pick_mode(&Args::default()), Mode::Headless);
        }
    }

    #[test]
    fn urls_include_token_and_network_address() {
        let u = urls_for(IpAddr::V4(Ipv4Addr::LOCALHOST), 7520, "tok");
        assert_eq!(u, vec!["http://127.0.0.1:7520/?token=tok".to_string()]);
        let u = urls_for(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 80, "t");
        assert_eq!(u, vec!["http://10.0.0.5:80/?token=t".to_string()]);
    }
}
