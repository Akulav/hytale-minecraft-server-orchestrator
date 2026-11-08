//! Desktop mode without a native window - Linux, where linking a web view (GTK / WebKit) would stop the one
//! binary from running on servers without those libraries. The UI opens as a browser "app" window
//! (Chrome, Chromium, Edge, Brave or Vivaldi with `--app`: no tabs or address bar) on its own profile, so
//! closing that window ends Hyve like closing a native one. Without such a browser the UI opens in the
//! default browser and Hyve keeps running until Quit (sidebar) or Ctrl+C.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::state::Shared;

/// Browsers that support `--app` windows, best first.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let names: &[&str] = if cfg!(windows) {
        &["msedge.exe", "chrome.exe", "brave.exe"]
    } else {
        &["google-chrome-stable", "google-chrome", "chromium", "chromium-browser", "microsoft-edge-stable", "microsoft-edge", "brave-browser", "brave", "vivaldi-stable", "vivaldi"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for name in names {
            for dir in std::env::split_paths(&path) {
                let p = dir.join(name);
                if p.is_file() {
                    out.push(p);
                    break;
                }
            }
        }
    }
    if cfg!(windows) {
        for base in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Some(b) = std::env::var_os(base) {
                let b = PathBuf::from(b);
                out.push(b.join("Microsoft/Edge/Application/msedge.exe"));
                out.push(b.join("Google/Chrome/Application/chrome.exe"));
            }
        }
    } else if cfg!(target_os = "macos") {
        for app in ["Google Chrome", "Microsoft Edge", "Brave Browser", "Chromium"] {
            out.push(PathBuf::from(format!("/Applications/{app}.app/Contents/MacOS/{app}")));
        }
    }
    out.retain(|p| p.is_file());
    out
}

/// Opens the UI. Returns at once; the window's end asks the program to quit.
pub fn open(state: &Shared, url: &str) {
    let profile = state.paths.root.join("browser-window");
    for browser in candidates() {
        let mut cmd = Command::new(&browser);
        cmd.arg(format!("--app={url}"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .args(["--no-first-run", "--no-default-browser-check", "--class=Hyve", "--window-size=1320,860"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::process::no_window(&mut cmd);
        match cmd.spawn() {
            Ok(mut child) => {
                crate::logln(&format!("window: {} --app", browser.display()));
                let st = state.clone();
                std::thread::spawn(move || {
                    let started = Instant::now();
                    loop {
                        if st.is_quitting() {
                            // Hyve is ending (Quit, an update): close the window too
                            let _ = child.kill();
                            let _ = child.wait();
                            return;
                        }
                        match child.try_wait() {
                            Ok(Some(_)) | Err(_) => break,
                            Ok(None) => std::thread::sleep(Duration::from_millis(400)),
                        }
                    }
                    if started.elapsed() < Duration::from_secs(5) {
                        // the browser handed the window to an instance that was already running
                        crate::logln("the window belongs to a browser that was already open: quit Hyve from its sidebar");
                    } else {
                        crate::logln("window closed");
                        st.request_quit();
                    }
                });
                return;
            }
            Err(e) => crate::logln(&format!("cannot start {}: {e}", browser.display())),
        }
    }
    crate::logln("no Chrome / Chromium / Edge / Brave found: opening the default browser (quit Hyve from its sidebar)");
    if let Err(e) = open::that_detached(url) {
        crate::logln(&format!("cannot open a browser ({e}): open {url} yourself"));
    }
}

/// Is the UI shown in a browser window rather than a native one?
pub const fn in_use() -> bool {
    !cfg!(native_window)
}
