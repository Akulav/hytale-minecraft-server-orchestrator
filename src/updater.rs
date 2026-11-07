//! In-app updates from the GitHub releases of Akulav/hytale-server-orchestrator: checked 15 s after start
//! and every 6 hours (setting), or on demand (About → Check for updates).
//!
//! A release has one file per system, always checked against the release's SHA256SUMS.txt first:
//! - Windows: `Hyve-setup.exe` (Inno Setup). Started on this install's folder with `/SILENT`
//!   (a progress window, no questions); Hyve stops its servers and ends; the setup replaces the
//!   program and starts it again.
//! - Linux: `hyve-linux-x64` (desktop) / `hyve-linux-x64-headless`, macOS:
//!   `hyve-macos-arm64`: the program itself, swapped in place (the old one is kept as `.old` until
//!   the next start) and started again with the same arguments.
//!
//! A copy built on this computer (…/target/…) never replaces itself.
//!
//! Rename (0.4, HyOrchestrator → Hyve): copies before 0.4 look for `HyOrchestrator-setup.exe`,
//! `hyorchestrator-linux-x64(-headless)` and `hyorchestrator-macos-arm64`. The Build workflow publishes
//! the new files under those old names too (same bytes, both in SHA256SUMS.txt), so old installs update
//! straight into Hyve. Keep that while people may still run a pre-0.4 version.

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::state::{Shared, VERSION, now_ms};

pub const REPO: &str = "Akulav/hytale-server-orchestrator";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

/// This build's release file.
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "Hyve-setup.exe"
    } else if cfg!(target_os = "macos") {
        "hyve-macos-arm64"
    } else if cfg!(feature = "desktop") {
        "hyve-linux-x64"
    } else {
        "hyve-linux-x64-headless"
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub file: String,
    pub sums: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub available: Option<Release>,
    pub checked_at: i64,
    pub checking: bool,
    pub installing: bool,
    pub error: String,
    pub self_built: bool,
}

/// "v1.10.2" -> [1, 10, 2]
fn numbers(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|x| x.parse().unwrap_or(0)).collect()
}

pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (numbers(latest), numbers(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn parse_release(v: &Value) -> Result<Release> {
    let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("GitHub's answer has no version"))?;
    let asset = |name: &str| -> String {
        v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"].as_str() == Some(name))
            .and_then(|a| a["browser_download_url"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        url: v["html_url"].as_str().unwrap_or_default().to_string(),
        file: asset(asset_name()),
        sums: asset("SHA256SUMS.txt"),
        notes: v["body"].as_str().unwrap_or_default().chars().take(4000).collect(),
    })
}

/// Built from source: cargo's `…/target/…`, or run.bat's `<repo>/dist/Hyve.exe`.
pub fn self_built(exe: &Path) -> bool {
    exe.components().any(|c| c.as_os_str() == "target")
        || exe.ancestors().nth(2).is_some_and(|repo| repo.join("Cargo.toml").is_file() && repo.join("src").join("main.rs").is_file())
}

pub fn spawn_background(state: Shared) {
    let st = state.clone();
    state.rt.spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            if st.is_quitting() {
                break;
            }
            if st.settings.read().check_updates {
                check(&st).await;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Asks GitHub for the latest release. Never fails: problems end up in `error`.
pub async fn check(state: &Shared) -> UpdateInfo {
    {
        let mut u = state.updates.write();
        if u.checking {
            return u.clone();
        }
        u.checking = true;
    }
    state.notify();
    let result = latest(state).await;
    {
        let mut u = state.updates.write();
        u.current = VERSION.to_string();
        u.checking = false;
        u.checked_at = now_ms();
        u.self_built = std::env::current_exe().map(|e| self_built(&e)).unwrap_or(false);
        match result {
            Ok(r) => {
                u.available = r;
                u.error.clear();
            }
            Err(e) => u.error = format!("{e:#}"),
        }
    }
    state.notify();
    state.updates.read().clone()
}

async fn latest(state: &Shared) -> Result<Option<Release>> {
    let resp = state
        .http
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .await?;
    if resp.status().as_u16() == 404 {
        return Ok(None); // no release yet
    }
    let rel = parse_release(&resp.error_for_status()?.json().await?)?;
    Ok((newer(&rel.version, VERSION) && !rel.file.is_empty()).then_some(rel))
}

fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (hash, n) = (it.next()?, it.next()?.trim_start_matches('*'));
        (n == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Downloads, verifies and starts the update, then asks the program to quit (servers stop cleanly).
pub async fn install(state: Shared, task: String) -> Result<String> {
    let rel = {
        let mut u = state.updates.write();
        if u.installing {
            bail!("an update is already being installed");
        }
        let rel = u.available.clone().ok_or_else(|| anyhow!("no update available - check again"))?;
        u.installing = true;
        rel
    };
    let result = install_inner(&state, &task, &rel).await;
    state.updates.write().installing = false;
    match result {
        Ok(()) => {
            state.task_update(&task, Some("Restarting into the new version…"), Some(1.0));
            let st = state.clone();
            state.rt.spawn(async move {
                tokio::time::sleep(Duration::from_millis(800)).await;
                st.request_quit();
            });
            Ok(format!("Hyve {} is being installed", rel.version))
        }
        Err(e) => Err(e),
    }
}

async fn install_inner(state: &Shared, task: &str, rel: &Release) -> Result<()> {
    let exe = std::env::current_exe()?;
    if self_built(&exe) {
        bail!("this copy was built on this computer: update it from the source (git pull, cargo build) instead");
    }
    if rel.file.is_empty() || rel.sums.is_empty() {
        bail!("release {} has no {} with a checksum", rel.version, asset_name());
    }
    std::fs::create_dir_all(&state.paths.updates)?;
    let file = state.paths.updates.join(format!("{}-{}", rel.version, asset_name()));
    crate::download::to_file(state, task, &rel.file, &file).await?;
    state.task_update(task, Some("Verifying the checksum…"), None);
    let sums = state.http.get(&rel.sums).send().await?.error_for_status()?.text().await?;
    let expected = expected_hash(&sums, asset_name()).ok_or_else(|| anyhow!("{} is not listed in SHA256SUMS.txt", asset_name()))?;
    let f2 = file.clone();
    let actual = tokio::task::spawn_blocking(move || sha256_file(&f2)).await??;
    if actual != expected {
        let _ = std::fs::remove_file(&file);
        bail!("the download doesn't match the release's checksum - nothing was installed");
    }
    // servers are stopped before the program is replaced
    state.task_update(task, Some("Stopping servers…"), None);
    crate::process::stop_all(state).await;
    let app_dir = exe.parent().ok_or_else(|| anyhow!("cannot locate the program folder"))?.to_path_buf();
    #[cfg(windows)]
    {
        run_setup(&file, &app_dir, &state.paths.logs.join("setup.log"))
    }
    #[cfg(not(windows))]
    {
        let _ = app_dir;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
        swap(&exe, &file)?;
        relaunch(&exe)
    }
}

/// Arguments for the setup when it updates (see installer/hyve.iss).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn setup_args(app_dir: &Path, log: &Path) -> Vec<String> {
    vec![
        "/SILENT".into(),
        "/SUPPRESSMSGBOXES".into(),
        "/NORESTART".into(),
        "/CLOSEAPPLICATIONS".into(),
        "/LAUNCH=1".into(),
        format!("/DIR={}", app_dir.display()),
        format!("/LOG={}", log.display()),
    ]
}

#[cfg(windows)]
fn run_setup(setup: &Path, app_dir: &Path, log: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    if let Some(d) = log.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let spawn = |flags: u32| std::process::Command::new(setup).args(setup_args(app_dir, log)).creation_flags(flags).spawn();
    spawn(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP))
        .map_err(|e| anyhow!("couldn't start the setup: {e}"))?;
    Ok(())
}

pub fn sibling(exe: &Path, ext: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(format!(".{ext}"));
    PathBuf::from(s)
}

/// Puts the new program in this one's place (this one becomes `.old`).
#[cfg_attr(windows, allow(dead_code))]
pub fn swap(exe: &Path, new: &Path) -> Result<()> {
    let old = sibling(exe, "old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| anyhow!("couldn't move this program aside: {e}"))?;
    if let Err(e) = std::fs::rename(new, exe).or_else(|_| std::fs::copy(new, exe).map(|_| ())) {
        let _ = std::fs::rename(&old, exe);
        return Err(anyhow!("couldn't put the new program in place: {e}"));
    }
    Ok(())
}

#[cfg_attr(windows, allow(dead_code))]
pub fn app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// Starts the new program after this one has ended, with the same arguments.
#[cfg(not(windows))]
fn relaunch(exe: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("/bin/sh");
    match app_bundle(exe).filter(|_| cfg!(target_os = "macos")) {
        Some(bundle) => {
            let _ = std::process::Command::new("codesign").args(["--force", "--deep", "--sign", "-"]).arg(&bundle).status();
            cmd.arg("-c").arg("sleep 3; open -n \"$0\"").arg(&bundle);
        }
        None => {
            let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--after-update").collect();
            cmd.arg("-c").arg("sleep 3; exec \"$0\" --after-update \"$@\"").arg(exe).args(args);
        }
    }
    cmd.process_group(0).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| anyhow!("couldn't start the new version: {e} - start Hyve again to run it"))?;
    Ok(())
}

/// At start: removes what the last update left behind.
pub fn cleanup(state: &Shared) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(sibling(&exe, "old"));
    }
    let _ = std::fs::remove_dir_all(&state.paths.updates);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("v0.2.1", "0.2.0"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.9", "0.2.0"));
        assert!(newer("1", "0.99.99"));
    }

    #[test]
    fn release_json_gives_this_builds_file() {
        let v = serde_json::json!({"tag_name": "v0.3.0", "html_url": "https://x/y", "body": "notes", "assets": [
            {"name": "SHA256SUMS.txt", "browser_download_url": "https://dl/sums"},
            {"name": asset_name(), "browser_download_url": "https://dl/prog"}]});
        let r = parse_release(&v).unwrap();
        assert_eq!((r.version.as_str(), r.file.as_str(), r.sums.as_str(), r.notes.as_str()), ("0.3.0", "https://dl/prog", "https://dl/sums", "notes"));
        assert!(parse_release(&serde_json::json!({})).is_err());
    }

    #[test]
    fn finds_hash_in_sums() {
        let h = "b774e08051dda5228007f80eb2dbb3ca22408f4b2f83450c468603b32f73819e";
        let sums = format!("{h}  hyve-linux-x64\r\nabc  Hyve-setup.exe\n");
        assert_eq!(expected_hash(&sums, "hyve-linux-x64").as_deref(), Some(h));
        assert_eq!(expected_hash(&sums, "Hyve-setup.exe"), None);
    }

    #[test]
    fn setup_runs_silently_on_this_folder() {
        let a = setup_args(Path::new("C:/Users/u/AppData/Local/Hyve"), Path::new("C:/x/setup.log"));
        assert!(a.contains(&"/SILENT".to_string()) && a.contains(&"/LAUNCH=1".to_string()));
        assert!(a.iter().any(|x| x == "/DIR=C:/Users/u/AppData/Local/Hyve"));
    }

    #[test]
    fn swap_keeps_the_old_program() {
        let dir = std::env::temp_dir().join(format!("hyo-swap-{}", crate::state::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("hyve");
        std::fs::write(&exe, b"old").unwrap();
        let new = dir.join("dl");
        std::fs::write(&new, b"new").unwrap();
        swap(&exe, &new).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(sibling(&exe, "old")).unwrap(), b"old");
        assert!(swap(&exe, &dir.join("missing")).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn self_built_copies_are_detected() {
        assert!(self_built(Path::new("D:/src/hyo/target/release/hyve.exe")));
        assert!(!self_built(Path::new("C:/Users/u/AppData/Local/Hyve/hyve.exe")));
        assert_eq!(app_bundle(Path::new("/Applications/Hyve.app/Contents/MacOS/hyve")), Some(PathBuf::from("/Applications/Hyve.app")));
    }
}
