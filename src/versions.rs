//! The local cache of Hytale server versions: `data/versions/<version>/` holds `Server/` (HytaleServer.jar,
//! HytaleServer.aot, Licenses) and `Assets.zip`, plus our `version.json`. Instances copy `Server/` and point
//! `--assets` at the shared Assets.zip (about 3 GB), so any cached version can be picked per instance.
//!
//! Versions come from:
//! - the official **Hytale Downloader CLI** (`downloader.hytale.com/hytale-downloader.zip`, Windows & Linux):
//!   latest `release` or `pre-release`; the first run asks for a Hytale login (device code in the task log);
//! - the **Hytale launcher's own files** on this computer (`…/Hytale/install/<patchline>/package/game/latest`);
//! - a **zip or folder** you point at (e.g. copied from another machine).

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::state::{Shared, now_ms, read_json, write_json};

pub const DOWNLOADER_URL: &str = "https://downloader.hytale.com/hytale-downloader.zip";
pub const PATCHLINES: [&str; 2] = ["release", "pre-release"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VersionInfo {
    pub version: String,
    pub patchline: String,
    /// "downloader", "launcher", "import"
    pub source: String,
    pub added_at: i64,
    #[serde(skip)]
    pub size: u64,
}

pub fn version_dir(state: &Shared, version: &str) -> PathBuf {
    state.paths.versions.join(version)
}

pub fn server_files(state: &Shared, version: &str) -> PathBuf {
    version_dir(state, version).join("Server")
}

pub fn assets_zip(state: &Shared, version: &str) -> PathBuf {
    version_dir(state, version).join("Assets.zip")
}

pub fn is_complete(state: &Shared, version: &str) -> bool {
    server_files(state, version).join("HytaleServer.jar").is_file() && assets_zip(state, version).is_file()
}

/// Cached versions, newest first.
pub fn list(state: &Shared) -> Vec<VersionInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&state.paths.versions) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() || !is_complete(state, &name) {
                continue;
            }
            let mut info: VersionInfo = read_json(&e.path().join("version.json")).unwrap_or_default();
            info.version = name.clone();
            info.size = std::fs::metadata(assets_zip(state, &name)).map(|m| m.len()).unwrap_or(0)
                + std::fs::metadata(server_files(state, &name).join("HytaleServer.jar")).map(|m| m.len()).unwrap_or(0);
            out.push(info);
        }
    }
    out.sort_by(|a, b| b.version.cmp(&a.version).then(b.added_at.cmp(&a.added_at)));
    out
}

/// Keeps a version string usable as a folder name.
pub fn sanitize(v: &str) -> String {
    let s: String = v.trim().chars().map(|c| if c.is_ascii_alphanumeric() || "._-+".contains(c) { c } else { '-' }).collect();
    s.trim_matches(['-', '.']).chars().take(64).collect()
}

/// Hytale versions look like `2026.01.22-6f8bdbdc4`.
pub fn find_version_in(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b(\d{4}\.\d{2}\.\d{2}-[0-9a-fA-F]{6,})\b").ok()?;
    text.lines().rev().find_map(|l| re.captures(l).map(|c| c[1].to_string()))
}

/// The server version from the jar's manifest (Implementation-Version / Hytale-Version), if present.
pub fn jar_version(jar: &Path) -> Option<String> {
    let mf = crate::archive::read_entry(jar, "META-INF/MANIFEST.MF")?;
    let mut fallback = None;
    for line in mf.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if let Some(found) = find_version_in(v) {
            return Some(found);
        }
        if k.eq_ignore_ascii_case("Implementation-Version") && !v.is_empty() {
            fallback = Some(v.to_string());
        }
    }
    fallback
}

/// Locates `HytaleServer.jar` and `Assets.zip` in an extracted download / launcher folder.
/// Returns (folder that contains the jar, Assets.zip).
pub fn locate_payload(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let files = crate::archive::walk_files(root);
    let jar = files.iter().filter(|f| f.file_name().is_some_and(|n| n == "HytaleServer.jar")).min_by_key(|f| f.components().count())?;
    let assets = files.iter().filter(|f| f.file_name().is_some_and(|n| n == "Assets.zip")).min_by_key(|f| f.components().count())?;
    Some((jar.parent()?.to_path_buf(), assets.clone()))
}

/// Moves (or copies) a payload into `versions/<version>` and records where it came from.
#[allow(clippy::too_many_arguments)]
fn adopt_payload(state: &Shared, task: &str, server_dir: &Path, assets: &Path, version: &str, patchline: &str, source: &str, move_files: bool) -> Result<()> {
    let target = version_dir(state, version);
    let staging = state.paths.versions.join(format!(".staging-{}", crate::state::new_id()));
    std::fs::create_dir_all(staging.join("Server"))?;
    let result = (|| -> Result<()> {
        let st = state.clone();
        let t = task.to_string();
        let report = move |label: &str, p: f32| st.task_update(&t, Some(&format!("{label} {:.0}%", p * 100.0)), Some(p));
        if move_files && std::fs::rename(server_dir, staging.join("Server")).is_ok() {
            // moved in one go
        } else {
            crate::archive::copy_tree(server_dir, &staging.join("Server"), |p| report("Copying server files…", p))?;
        }
        if !(move_files && std::fs::rename(assets, staging.join("Assets.zip")).is_ok()) {
            let mut r = std::fs::File::open(assets)?;
            let total = r.metadata()?.len().max(1);
            let mut w = std::fs::File::create(staging.join("Assets.zip"))?;
            let mut buf = vec![0u8; 4 << 20];
            let mut done = 0u64;
            loop {
                use std::io::{Read, Write};
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                done += n as u64;
                report("Copying Assets.zip…", done as f32 / total as f32);
            }
        }
        write_json(
            &staging.join("version.json"),
            &VersionInfo { version: version.into(), patchline: patchline.into(), source: source.into(), added_at: now_ms(), size: 0 },
        )?;
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&staging, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

// ---------------------------------------------------------------------------------------------
// Hytale Downloader CLI
// ---------------------------------------------------------------------------------------------

fn downloader_dir(state: &Shared) -> PathBuf {
    state.paths.tools.join("hytale-downloader")
}

/// Picks the executable for this system out of the extracted downloader archive.
pub fn pick_downloader(files: &[PathBuf]) -> Option<PathBuf> {
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "darwin" } else { "linux" };
    let candidates: Vec<&PathBuf> = files
        .iter()
        .filter(|f| {
            let n = f.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            n.starts_with("hytale-downloader") && !n.ends_with(".md") && !n.ends_with(".txt") && !n.ends_with(".zip") && (cfg!(windows) == n.ends_with(".exe"))
        })
        .collect();
    let arch_ok = |n: &str| if cfg!(target_arch = "aarch64") { n.contains("arm64") || n.contains("aarch64") } else { !n.contains("arm64") && !n.contains("aarch64") };
    candidates
        .iter()
        .find(|f| {
            let n = f.file_name().unwrap().to_string_lossy().to_lowercase();
            n.contains(os) && arch_ok(&n)
        })
        .or_else(|| candidates.iter().find(|f| f.file_name().unwrap().to_string_lossy().to_lowercase().contains(os)))
        .or_else(|| candidates.first())
        .map(|p| (*p).clone())
}

async fn ensure_downloader(state: &Shared, task: &str) -> Result<PathBuf> {
    if cfg!(target_os = "macos") {
        bail!("Hytale publishes the downloader for Windows and Linux only. On a Mac, import the server files from your Hytale launcher (Versions → Import from launcher) or from a zip.");
    }
    let dir = downloader_dir(state);
    if let Some(exe) = pick_downloader(&crate::archive::walk_files(&dir)) {
        // Hypixel updates the tool now and then: refresh a copy older than a week (best effort - the
        // saved login next to it is kept, the archive only overwrites the programs)
        let old = std::fs::metadata(&exe).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 7 * 86400);
        if !old {
            return Ok(exe);
        }
        state.task_log(task, "Refreshing the Hytale Downloader CLI (a week old)");
        let zip = state.paths.tmp.join("hytale-downloader.zip");
        if crate::download::to_file(state, task, DOWNLOADER_URL, &zip).await.is_ok() {
            let (z2, d2) = (zip.clone(), dir.clone());
            let _ = tokio::task::spawn_blocking(move || crate::archive::extract_zip(&z2, &d2, |_| {})).await;
            let _ = std::fs::remove_file(&zip);
        }
        let exe = pick_downloader(&crate::archive::walk_files(&dir)).unwrap_or(exe);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
        }
        // a fresh extract has a new modification time only if the archive did; touch it either way
        if let Ok(f) = std::fs::File::options().append(true).open(&exe) {
            let _ = f.set_modified(std::time::SystemTime::now());
        }
        return Ok(exe);
    }
    state.task_log(task, format!("Fetching the Hytale Downloader CLI from {DOWNLOADER_URL}"));
    let zip = state.paths.tmp.join("hytale-downloader.zip");
    crate::download::to_file(state, task, DOWNLOADER_URL, &zip).await?;
    let (z2, d2) = (zip.clone(), dir.clone());
    tokio::task::spawn_blocking(move || crate::archive::extract_zip(&z2, &d2, |_| {})).await??;
    let _ = std::fs::remove_file(&zip);
    let exe = pick_downloader(&crate::archive::walk_files(&dir)).ok_or_else(|| anyhow!("the downloader archive has no program for this system"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(exe)
}

/// Runs the downloader with `args`, streaming its output into the task log. Returns all output.
async fn run_downloader(state: &Shared, task: &str, exe: &Path, args: &[String]) -> Result<String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args).current_dir(downloader_dir(state)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::process::no_window(&mut cmd);
    let mut child = tokio::process::Command::from(cmd).kill_on_drop(true).spawn().with_context(|| format!("cannot start {}", exe.display()))?;
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        crate::process::job::adopt(pid);
    }
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let shared = std::sync::Arc::new(parking_lot::Mutex::new(Pump::default()));
    let pump = |r: Box<dyn tokio::io::AsyncRead + Unpin + Send>| {
        let (st, t, sh) = (state.clone(), task.to_string(), shared.clone());
        tokio::spawn(async move { crate::process::read_segments(r, |line| on_downloader_line(&st, &t, &sh, &line)).await })
    };
    let a = pump(Box::new(out));
    let b = pump(Box::new(err));
    let status = child.wait().await?;
    let _ = tokio::join!(a, b);
    state.task_login(task, None);
    let text = shared.lock().text.clone();
    if !status.success() {
        if shared.lock().prompt.is_some() {
            bail!("the Hytale sign-in was not completed in time - start the download again for a new code");
        }
        bail!("the Hytale downloader failed ({status}); see the log above");
    }
    Ok(text)
}

/// What the downloader printed so far, and its sign-in prompt (while one is open).
#[derive(Default)]
struct Pump {
    text: String,
    prompt: Option<crate::state::LoginPrompt>,
    opened: bool,
    last_pct: f32,
}

fn on_downloader_line(st: &Shared, task: &str, sh: &parking_lot::Mutex<Pump>, raw: &str) {
    let line = crate::process::strip_ansi(raw);
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let bits = crate::login::parse(line);
    let mut p = sh.lock();
    p.text.push_str(line);
    p.text.push('\n');
    let mut changed = false;
    for b in &bits {
        if let crate::login::Bit::Progress(pct) = b {
            if p.prompt.take().is_some() {
                st.task_login(task, None);
                st.task_log(task, "Signed in to Hytale ✓ (the login is saved for next time)");
            }
            st.task_update(task, Some(&format!("Downloading… {pct:.0}%")), Some(pct / 100.0));
            // log every 10 % instead of every redraw
            if *pct >= p.last_pct + 10.0 || *pct >= 100.0 && p.last_pct < 100.0 {
                p.last_pct = *pct;
                st.task_log(task, line.to_string());
            }
            return;
        }
        let prompt = p.prompt.get_or_insert_with(Default::default);
        changed |= crate::login::merge(prompt, b);
    }
    if changed {
        let prompt = p.prompt.clone().unwrap_or_default();
        if !p.opened && prompt.url.contains("user_code=") {
            p.opened = true;
            crate::login::auto_open(st, &prompt.url);
        }
        st.task_login(task, Some(prompt));
        st.task_update(task, Some("Sign in to Hytale to continue - approve the code in your browser"), None);
    } else if bits.is_empty() && p.prompt.is_some() && !crate::login::is_prompt_text(line) {
        // the tool moved on: the sign-in went through
        p.prompt = None;
        st.task_login(task, None);
        st.task_log(task, "Signed in to Hytale ✓ (the login is saved for next time)");
        st.task_update(task, Some("Signed in - working…"), None);
    }
    st.task_log(task, line.to_string());
}

/// The downloader keeps its Hytale login next to itself after the first sign-in.
pub fn downloader_signed_in(state: &Shared) -> bool {
    downloader_dir(state).join(".hytale-downloader-credentials.json").is_file()
}

/// Forgets the downloader's Hytale login (the next download asks again).
pub fn downloader_sign_out(state: &Shared) -> Result<()> {
    let f = downloader_dir(state).join(".hytale-downloader-credentials.json");
    if f.exists() {
        std::fs::remove_file(f)?;
    }
    state.notify();
    Ok(())
}

/// Downloads the latest server of `patchline` into the cache (skipped when already there).
/// Returns the version.
pub async fn download_latest(state: &Shared, task: &str, patchline: &str) -> Result<String> {
    if !PATCHLINES.contains(&patchline) {
        bail!("unknown patchline {patchline}");
    }
    let exe = ensure_downloader(state, task).await?;
    state.task_update(task, Some("Asking Hytale for the latest version…"), None);
    let base = vec!["-patchline".to_string(), patchline.to_string(), "-skip-update-check".to_string()];
    let mut args = base.clone();
    args.push("-print-version".into());
    let printed = run_downloader(state, task, &exe, &args).await?;
    let latest = find_version_in(&printed).map(|v| sanitize(&v));
    if let Some(v) = &latest {
        state.task_log(task, format!("Latest {patchline}: {v}"));
        if is_complete(state, v) {
            return Ok(v.clone());
        }
    }
    let zip = state.paths.tmp.join(format!("hytale-{patchline}-{}.zip", crate::state::new_id()));
    let mut args = base;
    args.push("-download-path".into());
    args.push(zip.display().to_string());
    state.task_update(task, Some("Downloading the server files (several GB)…"), None);
    run_downloader(state, task, &exe, &args).await?;
    let version = install_zip(state, task, &zip, latest, patchline, "downloader").await;
    let _ = std::fs::remove_file(&zip);
    version
}

/// Extracts a server zip (downloader output or an import) and caches it. Returns the version.
pub async fn install_zip(state: &Shared, task: &str, zip: &Path, known_version: Option<String>, patchline: &str, source: &str) -> Result<String> {
    let extract = state.paths.tmp.join(format!("extract-{}", crate::state::new_id()));
    let (st, t, z, x) = (state.clone(), task.to_string(), zip.to_path_buf(), extract.clone());
    state.task_update(task, Some("Extracting…"), Some(0.0));
    let r = tokio::task::spawn_blocking(move || {
        crate::archive::extract_zip(&z, &x, |p| {
            if ((p * 1000.0) as u32).is_multiple_of(10) {
                st.task_update(&t, Some(&format!("Extracting… {:.0}%", p * 100.0)), Some(p));
            }
        })
    })
    .await?;
    let result = match r {
        Ok(()) => install_folder(state, task, &extract, known_version, patchline, source, true).await,
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&extract);
    result
}

/// Caches the server found in `folder` (a launcher install or an extracted download).
pub async fn install_folder(state: &Shared, task: &str, folder: &Path, known_version: Option<String>, patchline: &str, source: &str, move_files: bool) -> Result<String> {
    let (server_dir, assets) = locate_payload(folder).ok_or_else(|| anyhow!("no HytaleServer.jar + Assets.zip found in {}", folder.display()))?;
    let version = known_version
        .or_else(|| jar_version(&server_dir.join("HytaleServer.jar")).map(|v| sanitize(&v)))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("{source}-{}", chrono::Local::now().format("%Y%m%d-%H%M")));
    state.task_log(task, format!("Server version: {version}"));
    let (st, t, v, p, s) = (state.clone(), task.to_string(), version.clone(), patchline.to_string(), source.to_string());
    tokio::task::spawn_blocking(move || adopt_payload(&st, &t, &server_dir, &assets, &v, &p, &s, move_files)).await??;
    state.notify();
    Ok(version)
}

/// Hytale launcher installs on this computer: (patchline, folder).
pub fn launcher_installs() -> Vec<(String, PathBuf)> {
    let mut bases = Vec::new();
    if cfg!(windows) {
        if let Some(d) = dirs::config_dir() {
            bases.push(d.join("Hytale")); // %APPDATA%\Hytale
        }
    } else if cfg!(target_os = "macos") {
        if let Some(d) = dirs::data_dir() {
            bases.push(d.join("Hytale")); // ~/Library/Application Support/Hytale
        }
    } else {
        let xdg = std::env::var("XDG_DATA_HOME").map(PathBuf::from).ok().or_else(dirs::data_dir);
        if let Some(d) = xdg {
            bases.push(d.join("Hytale"));
        }
    }
    let mut out = Vec::new();
    for base in bases {
        for p in PATCHLINES {
            let dir = base.join("install").join(p).join("package").join("game").join("latest");
            if dir.join("Server").join("HytaleServer.jar").is_file() && dir.join("Assets.zip").is_file() {
                out.push((p.to_string(), dir));
            }
        }
    }
    out
}

/// Deletes a cached version (refused while an instance uses it).
pub fn delete(state: &Shared, version: &str) -> Result<()> {
    let users: Vec<String> = state.instances.read().iter().filter(|i| i.version == version).map(|i| i.name.clone()).collect();
    if !users.is_empty() {
        bail!("{version} is used by: {}", users.join(", "));
    }
    let dir = version_dir(state, version);
    if dir.starts_with(&state.paths.versions) && dir != state.paths.versions && dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    state.notify();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_versions_in_downloader_output() {
        assert_eq!(find_version_in("Checking...\n2026.01.22-6f8bdbdc4\n").as_deref(), Some("2026.01.22-6f8bdbdc4"));
        assert_eq!(find_version_in("game version: 2026.03.01-abcdef12 (release)").as_deref(), Some("2026.03.01-abcdef12"));
        assert_eq!(find_version_in("nothing here"), None);
    }

    #[test]
    fn reads_version_from_jar_manifest() {
        let dir = std::env::temp_dir().join(format!("hyo-jar-{}", crate::state::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("HytaleServer.jar");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        zw.start_file("META-INF/MANIFEST.MF", zip::write::SimpleFileOptions::default()).unwrap();
        use std::io::Write;
        zw.write_all(b"Manifest-Version: 1.0\r\nImplementation-Version: 2026.01.22-6f8bdbdc4\r\n\r\n").unwrap();
        zw.finish().unwrap();
        assert_eq!(jar_version(&jar).as_deref(), Some("2026.01.22-6f8bdbdc4"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sanitizes_folder_names() {
        assert_eq!(sanitize(" 2026.01.22-6f8bdbdc4 "), "2026.01.22-6f8bdbdc4");
        assert_eq!(sanitize("../evil/../x"), "evil-..-x");
        assert!(!sanitize("a/b\\c:d").contains(['/', '\\', ':']));
    }

    #[test]
    fn picks_the_downloader_for_this_system() {
        let files: Vec<PathBuf> = ["QUICKSTART.md", "hytale-downloader-linux-amd64", "hytale-downloader-windows-amd64.exe"].iter().map(PathBuf::from).collect();
        let picked = pick_downloader(&files).unwrap();
        if cfg!(windows) {
            assert_eq!(picked, PathBuf::from("hytale-downloader-windows-amd64.exe"));
        } else if cfg!(target_os = "linux") {
            assert_eq!(picked, PathBuf::from("hytale-downloader-linux-amd64"));
        }
    }

    #[test]
    fn picks_the_real_downloader_archive_names() {
        // the names inside downloader.hytale.com/hytale-downloader.zip (checked 2026-11)
        let files: Vec<PathBuf> = ["QUICKSTART.md", "hytale-downloader-linux-amd64", "hytale-downloader-windows-amd64.exe"].iter().map(PathBuf::from).collect();
        assert!(pick_downloader(&files).is_some());
    }

    #[test]
    fn locates_payload_in_launcher_layout() {
        let root = std::env::temp_dir().join(format!("hyo-payload-{}", crate::state::new_id()));
        std::fs::create_dir_all(root.join("latest/Server")).unwrap();
        std::fs::create_dir_all(root.join("latest/Client")).unwrap();
        std::fs::write(root.join("latest/Server/HytaleServer.jar"), b"x").unwrap();
        std::fs::write(root.join("latest/Assets.zip"), b"x").unwrap();
        let (server, assets) = locate_payload(&root).unwrap();
        assert_eq!(server, root.join("latest/Server"));
        assert_eq!(assets, root.join("latest/Assets.zip"));
        let _ = std::fs::remove_dir_all(root);
    }
}
