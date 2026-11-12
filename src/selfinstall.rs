//! Linux and macOS ship one plain program file, which installs itself: there is no installer package.
//! The same file is the first install and every update (Windows has `Hyve-setup.exe` for both).
//!
//! The user picks the **install folder**; the program *and all its data* (`data/`: servers, versions,
//! backups, Java, logs) live there:
//!
//! - **Linux:** `<folder>/hyve` + `<folder>/data` (default `~/.local/share/Hyve`), a `~/.local/bin/hyve` link,
//!   a menu entry (`~/.local/share/applications/hyve.desktop`) and an icon in the hicolor theme.
//! - **macOS:** `<folder>/Hyve.app` (built here: program, Info.plist, icon, ad-hoc signature) + `<folder>/data`
//!   (default `~/Applications/Hyve`). Before 0.6.1 the app was `~/Applications/Hyve.app` with its data in
//!   `~/Library/Application Support/Hyve/data`; installing over it moves both into the folder.
//!
//! The folder is remembered (`<config dir>/Hyve/install-dir`) so a newer download can offer to update it.
//! Started from anywhere else (e.g. Downloads) in desktop mode, Hyve offers to install itself - or to update
//! an installed copy that is older. `hyve --install [--install-dir <folder>]` does it from a terminal,
//! `hyve --uninstall [--delete-data]` removes it again. Updates later swap the installed program in place
//! (updater.rs).

use anyhow::{Result, bail};
#[cfg(unix)]
use anyhow::{Context, anyhow};
use std::path::{Path, PathBuf};

use crate::state::{APP_NAME, VERSION};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const ICON_256: &[u8] = include_bytes!("../assets/icon.png");
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const ICON_1024: &[u8] = include_bytes!("../assets/icon-1024.png");
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const INFO_PLIST: &str = include_str!("../installer/Info.plist");

/// What the dashboard offers: install, or update an older installed copy.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// version of the installed copy when this is an update
    pub installed: Option<String>,
    /// the install folder (editable in the dashboard for a new install)
    pub target: PathBuf,
}

/// One line holding the install folder.
fn pointer_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP_NAME).join("install-dir"))
}

fn remembered_folder() -> Option<PathBuf> {
    let text = std::fs::read_to_string(pointer_file()?).ok()?;
    let t = text.trim();
    (!t.is_empty()).then(|| PathBuf::from(t))
}

#[cfg_attr(not(unix), allow(dead_code))]
fn remember(folder: &Path) {
    if let Some(f) = pointer_file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, folder.to_string_lossy().as_bytes());
    }
}

#[cfg_attr(not(unix), allow(dead_code))]
fn forget() {
    if let Some(f) = pointer_file() {
        let _ = std::fs::remove_file(&f);
        if let Some(d) = f.parent() {
            let _ = std::fs::remove_dir(d);
        }
    }
}

/// The suggested install folder: Linux `~/.local/share/Hyve` (also where versions before 0.6.1 installed),
/// macOS `~/Applications/Hyve`.
pub fn default_folder() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(dirs::home_dir()?.join("Applications").join(APP_NAME))
    } else if cfg!(target_os = "linux") {
        Some(dirs::data_local_dir()?.join(APP_NAME))
    } else {
        None
    }
}

/// macOS before 0.6.1: `~/Applications/Hyve.app` or `/Applications/Hyve.app`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn legacy_bundle() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let user = dirs::home_dir()?.join("Applications").join(format!("{APP_NAME}.app"));
    let system = PathBuf::from("/Applications").join(format!("{APP_NAME}.app"));
    [user, system].into_iter().find(|a| a.join("Contents").join("MacOS").join("hyve").is_file())
}

/// The program inside an install folder.
pub fn exe_in(folder: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        folder.join(format!("{APP_NAME}.app")).join("Contents").join("MacOS").join("hyve")
    } else {
        folder.join("hyve")
    }
}

/// The install folder: the remembered one, else the default one when an older install exists there.
pub fn install_folder() -> Option<PathBuf> {
    remembered_folder().or_else(|| {
        let d = default_folder()?;
        (exe_in(&d).is_file() || legacy_bundle().is_some()).then_some(d)
    })
}

/// Is `dir` the (remembered) Linux/macOS install folder?
pub fn is_install_folder(dir: &Path) -> bool {
    !cfg!(windows) && install_folder().is_some_and(|f| same_file(&f, dir))
}

/// The installed program file.
pub fn installed_exe() -> Option<PathBuf> {
    if let Some(f) = remembered_folder() {
        return Some(exe_in(&f));
    }
    let d = default_folder()?;
    let exe = exe_in(&d);
    if exe.is_file() {
        return Some(exe);
    }
    legacy_bundle().map(|a| a.join("Contents").join("MacOS").join("hyve")).or(Some(exe))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Is `exe` an installed copy (it then updates itself in place and never offers to install)?
pub fn is_installed_copy(exe: &Path) -> bool {
    if cfg!(target_os = "macos") {
        return crate::updater::app_bundle(exe).is_some();
    }
    installed_exe().is_some_and(|i| same_file(exe, &i))
}

/// `hyve 0.5.0` -> `0.5.0` (what `--version` prints).
pub fn parse_version_output(text: &str) -> Option<String> {
    let v = text.split_whitespace().nth(1)?.trim();
    v.chars().next().filter(char::is_ascii_digit).map(|_| v.to_string())
}

fn version_of(exe: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--version").stdin(std::process::Stdio::null());
    crate::process::no_window(&mut cmd);
    let out = cmd.output().ok()?;
    parse_version_output(&String::from_utf8_lossy(&out.stdout))
}

/// At start: is there something to offer? (Linux / macOS, release copies run from outside the install.)
pub fn check() -> Option<Offer> {
    if cfg!(windows) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    if crate::updater::self_built(&exe) || is_installed_copy(&exe) {
        return None;
    }
    if let Some(installed_exe) = installed_exe().filter(|e| e.is_file()) {
        // an installed copy exists: only offer when this one is newer
        let target = install_folder().or_else(default_folder)?;
        let v = version_of(&installed_exe).unwrap_or_else(|| "0".into());
        return crate::updater::newer(VERSION, &v).then_some(Offer { installed: Some(v), target });
    }
    Some(Offer { installed: None, target: install_folder().or_else(default_folder)? })
}

/// Check a folder typed into the install banner. `~` means the home folder.
pub fn parse_folder(text: &str) -> Result<PathBuf> {
    let text = text.trim();
    if text.is_empty() {
        bail!("choose a folder");
    }
    let path = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home folder"))?.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    };
    if !path.is_absolute() {
        bail!("use a full path, for example ~/Servers/Hyve");
    }
    if path.parent().is_none() {
        bail!("pick a folder, not the root of a disk");
    }
    if path.is_file() {
        bail!("that is a file, not a folder");
    }
    Ok(path)
}

// ---------------------------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------------------------

/// The menu entry.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={APP_NAME}\nGenericName=Game server manager\nComment=Run and manage Hytale and Minecraft servers\nExec=\"{}\" --desktop\nIcon=hyve\nTerminal=false\nCategories=Game;Network;\nKeywords=hytale;minecraft;server;\nStartupWMClass=Hyve\n",
        exe.display()
    )
}

#[cfg(target_os = "linux")]
fn linux_paths() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let share = dirs::data_local_dir().ok_or_else(|| anyhow!("no home folder"))?;
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home folder"))?;
    let (entry, icons) = (share.join("applications").join("hyve.desktop"), share.join("icons").join("hicolor"));
    Ok((home.join(".local").join("bin").join("hyve"), entry, icons, share))
}

#[cfg(target_os = "linux")]
fn install_files(exe: &Path, folder: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let target = exe_in(folder);
    std::fs::create_dir_all(folder).with_context(|| format!("cannot create {}", folder.display()))?;
    // copy next to it first, then rename: a running installed copy keeps its old file
    let tmp = folder.join("hyve.new");
    std::fs::copy(exe, &tmp).with_context(|| format!("cannot copy the program to {}", folder.display()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, &target)?;
    let (link, entry, icons, share) = linux_paths()?;
    if let Some(d) = link.parent() {
        std::fs::create_dir_all(d)?;
    }
    if std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()) {
        let _ = std::fs::remove_file(&link);
    }
    if !link.exists() {
        let _ = std::os::unix::fs::symlink(&target, &link);
    }
    if let Some(d) = entry.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&entry, desktop_entry(&target))?;
    let icon = icons.join("256x256").join("apps");
    std::fs::create_dir_all(&icon)?;
    std::fs::write(icon.join("hyve.png"), ICON_256)?;
    let _ = std::process::Command::new("update-desktop-database").arg(share.join("applications")).output();
    let _ = std::process::Command::new("gtk-update-icon-cache").args(["-f", "-t"]).arg(&icons).output();
    remember(folder);
    Ok(target)
}

#[cfg(target_os = "linux")]
fn uninstall_files(delete_data: bool) -> Result<Vec<String>> {
    let mut done = Vec::new();
    let Some(root) = install_folder() else { return Ok(done) };
    let (link, entry, icons, _) = linux_paths()?;
    if std::fs::read_link(&link).is_ok_and(|t| t.starts_with(&root)) {
        std::fs::remove_file(&link)?;
        done.push(link.display().to_string());
    }
    for f in [entry, icons.join("256x256").join("apps").join("hyve.png"), root.join("hyve"), root.join("hyve.old"), root.join("hyve.new")] {
        if f.exists() {
            std::fs::remove_file(&f)?;
            done.push(f.display().to_string());
        }
    }
    forget();
    if delete_data && root.join("data").exists() {
        std::fs::remove_dir_all(root.join("data"))?;
        done.push(root.join("data").display().to_string());
    }
    let _ = std::fs::remove_dir(&root);
    Ok(done)
}

// ---------------------------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------------------------

/// The app's Info.plist for this version.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn info_plist() -> String {
    let re = regex::Regex::new(r"(<key>CFBundle(?:Short)?VersionString</key><string>|<key>CFBundleVersion</key><string>)[^<]*").unwrap();
    re.replace_all(INFO_PLIST, |c: &regex::Captures| format!("{}{VERSION}", &c[1])).into_owned()
}

/// A minimal .icns: the 1024 px PNG as `ic10` and the 256 px one as `ic08` (macOS reads PNG entries).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn icns() -> Vec<u8> {
    let entries: [(&[u8; 4], &[u8]); 2] = [(b"ic08", ICON_256), (b"ic10", ICON_1024)];
    let total = 8 + entries.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (kind, data) in entries {
        out.extend_from_slice(kind);
        out.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

#[cfg(target_os = "macos")]
fn install_files(exe: &Path, folder: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let app = folder.join(format!("{APP_NAME}.app"));
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS")).with_context(|| format!("cannot create {}", app.display()))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    let target = contents.join("MacOS").join("hyve");
    let tmp = contents.join("MacOS").join("hyve.new");
    std::fs::copy(exe, &tmp).with_context(|| format!("cannot copy the program into {}", app.display()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, &target)?;
    std::fs::write(contents.join("Info.plist"), info_plist())?;
    std::fs::write(contents.join("Resources").join("icon.icns"), icns())?;
    // a downloaded file is quarantined: the app made from it must not be
    let _ = std::process::Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&app).output();
    let _ = std::process::Command::new("codesign").args(["--force", "--deep", "--sign", "-"]).arg(&app).output();
    // the pre-0.6.1 layout: its data comes along, the old app goes
    let data = folder.join("data");
    let old_data = crate::state::Paths::user_root();
    if !data.exists() && old_data.is_dir() {
        let _ = crate::state::move_tree(&old_data, &data);
    }
    if let Some(old_app) = legacy_bundle().filter(|a| !same_file(a, &app)) {
        let _ = std::fs::remove_dir_all(old_app);
    }
    remember(folder);
    Ok(target)
}

#[cfg(target_os = "macos")]
fn uninstall_files(delete_data: bool) -> Result<Vec<String>> {
    let mut done = Vec::new();
    let folder = install_folder();
    let apps = folder.iter().map(|f| f.join(format!("{APP_NAME}.app"))).chain(legacy_bundle());
    for app in apps {
        if app.join("Contents").join("MacOS").join("hyve").is_file() {
            std::fs::remove_dir_all(&app)?;
            done.push(app.display().to_string());
        }
    }
    forget();
    if delete_data {
        for data in folder.iter().map(|f| f.join("data")).chain([crate::state::Paths::user_root()]) {
            if data.exists() {
                std::fs::remove_dir_all(&data)?;
                done.push(data.display().to_string());
            }
        }
    }
    if let Some(f) = folder {
        let _ = std::fs::remove_dir(f);
    }
    Ok(done)
}

#[cfg(not(unix))]
fn install_files(_exe: &Path, _folder: &Path) -> Result<PathBuf> {
    bail!("on Windows, Hyve-setup.exe installs and updates Hyve")
}

#[cfg(not(unix))]
fn uninstall_files(_delete_data: bool) -> Result<Vec<String>> {
    bail!("on Windows, uninstall Hyve in Settings → Apps")
}

// ---------------------------------------------------------------------------------------------
// Doing it
// ---------------------------------------------------------------------------------------------

/// Data this copy made next to itself (`<folder>/data`, e.g. in Downloads) moves along with the install,
/// when the installed copy has none yet.
fn move_data_along(exe: &Path, folder: &Path) -> Option<String> {
    let here = exe.parent()?.join("data");
    let there = folder.join("data");
    if !here.is_dir() || there.exists() || same_file(&here, &there) {
        return None;
    }
    Some(match crate::state::move_tree(&here, &there) {
        Ok(()) => format!("moved the data from {} to {}", here.display(), there.display()),
        Err(e) => format!("the data stays in {} ({e}); start the installed Hyve with --data-dir to use it", here.display()),
    })
}

/// `hyve --install [--install-dir <folder>]` (terminal): installs this program for the user.
pub fn install_cli(folder: Option<PathBuf>) -> Result<String> {
    let exe = std::env::current_exe()?;
    if is_installed_copy(&exe) {
        bail!("this is already the installed copy ({})", exe.display());
    }
    let folder = match folder {
        Some(f) => parse_folder(&f.to_string_lossy())?,
        None => install_folder().or_else(default_folder).ok_or_else(|| anyhow::anyhow!("no home folder"))?,
    };
    let target = install_files(&exe, &folder)?;
    let mut msg = format!("Hyve {VERSION} is installed: {} (its data goes to {})", target.display(), folder.join("data").display());
    if let Some(note) = move_data_along(&exe, &folder) {
        msg.push_str(&format!("\n{note}"));
    }
    if cfg!(target_os = "linux") {
        msg.push_str("\nStart it from the app menu, or run `hyve` (~/.local/bin is on the PATH of most systems).");
    }
    Ok(msg)
}

/// `hyve --uninstall [--delete-data]`.
pub fn uninstall_cli(delete_data: bool) -> Result<String> {
    let data = install_folder().map(|f| f.join("data"));
    let done = uninstall_files(delete_data)?;
    Ok(if done.is_empty() {
        "Hyve is not installed for this user - nothing to remove.".to_string()
    } else {
        let keep = match (&data, delete_data) {
            (Some(d), false) => format!("\nYour servers and data stay in {} (add --delete-data to remove them).", d.display()),
            _ => String::new(),
        };
        format!("Removed:\n  {}{keep}", done.join("\n  "))
    })
}

/// The dashboard's Install / Update button: installs into the offered folder, then starts the installed copy
/// and ends this one.
pub async fn install_task(state: crate::state::Shared, task: String) -> Result<String> {
    let exe = std::env::current_exe()?;
    let folder = state.install_offer.read().as_ref().map(|o| o.target.clone());
    let folder = match folder {
        Some(f) => parse_folder(&f.to_string_lossy())?,
        None => install_folder().or_else(default_folder).ok_or_else(|| anyhow::anyhow!("no home folder"))?,
    };
    state.task_update(&task, Some("Stopping servers…"), None);
    crate::process::stop_all(&state).await;
    state.task_update(&task, Some("Installing…"), None);
    let (e2, f2) = (exe.clone(), folder.clone());
    let target = tokio::task::spawn_blocking(move || install_files(&e2, &f2)).await??;
    state.task_log(&task, format!("Installed {}", target.display()));
    // data next to this copy moves along only when it is the folder in use
    if exe.parent().is_some_and(|d| same_file(&d.join("data"), &state.paths.root))
        && let Some(note) = move_data_along(&exe, &folder)
    {
        state.task_log(&task, note);
    }
    relaunch_installed(&target)?;
    *state.install_offer.write() = None;
    let st = state.clone();
    state.rt.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        st.request_quit();
    });
    Ok(format!("Installed in {} - starting it", folder.display()))
}

/// Starts the installed copy once this one has ended (its web port is free again).
#[cfg(unix)]
fn relaunch_installed(target: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("/bin/sh");
    match crate::updater::app_bundle(target).filter(|_| cfg!(target_os = "macos")) {
        Some(app) => {
            cmd.arg("-c").arg("sleep 3; open -n \"$0\"").arg(app);
        }
        None => {
            // same options (a --data-dir stays in use)
            let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--after-update").collect();
            cmd.arg("-c").arg("sleep 3; exec \"$0\" \"$@\"").arg(target).args(args);
        }
    }
    cmd.process_group(0).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| anyhow!("installed, but couldn't start it: {e} - start Hyve from the menu"))?;
    Ok(())
}

#[cfg(not(unix))]
fn relaunch_installed(_target: &Path) -> Result<()> {
    bail!("on Windows, Hyve-setup.exe installs Hyve")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions_and_writes_entries() {
        assert_eq!(parse_version_output("hyve 0.5.0\n").as_deref(), Some("0.5.0"));
        assert_eq!(parse_version_output("hyve"), None);
        assert_eq!(parse_version_output("error: x"), None);
        let e = desktop_entry(Path::new("/home/u/.local/share/Hyve/hyve"));
        assert!(e.starts_with("[Desktop Entry]\n") && e.contains("Exec=\"/home/u/.local/share/Hyve/hyve\" --desktop\n") && e.contains("Icon=hyve\n"));
    }

    #[test]
    fn builds_the_mac_app_files() {
        let p = info_plist();
        assert!(p.contains(&format!("<key>CFBundleShortVersionString</key><string>{VERSION}</string>")), "{p}");
        assert!(p.contains(&format!("<key>CFBundleVersion</key><string>{VERSION}</string>")), "{p}");
        let i = icns();
        assert_eq!(&i[..4], b"icns");
        assert_eq!(u32::from_be_bytes(i[4..8].try_into().unwrap()) as usize, i.len());
        assert_eq!(&i[8..12], b"ic08");
        // PNG data right after the first entry header
        assert_eq!(&i[16..20], b"\x89PNG");
    }

    #[test]
    fn installed_copies_are_recognised() {
        let exe = std::env::temp_dir().join("Downloads-test").join("hyve-linux-x64");
        assert!(!is_installed_copy(&exe));
        if cfg!(target_os = "macos") {
            assert!(is_installed_copy(Path::new("/Users/u/Applications/Hyve/Hyve.app/Contents/MacOS/hyve")));
        }
    }

    #[test]
    fn checks_typed_folders() {
        assert!(parse_folder("").is_err());
        assert!(parse_folder("relative/dir").is_err());
        if cfg!(unix) {
            assert_eq!(parse_folder(" /srv/hyve ").unwrap(), PathBuf::from("/srv/hyve"));
            assert!(parse_folder("~/Servers/Hyve").unwrap().ends_with("Servers/Hyve"));
            assert_eq!(exe_in(Path::new("/srv/hyve")).file_name().unwrap(), "hyve");
        } else {
            assert!(parse_folder(r"C:\").is_err());
            assert_eq!(parse_folder(r"D:\Servers\Hyve").unwrap(), PathBuf::from(r"D:\Servers\Hyve"));
        }
    }
}
