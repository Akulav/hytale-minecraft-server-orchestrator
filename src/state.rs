//! Shared application state: data folders, settings, the instance registry, the live runtime of each
//! server, background tasks and change notifications for the UI.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{mpsc, watch};

pub const APP_NAME: &str = "HyOrchestrator";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("HyOrchestrator/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Akulav/hytale-server-orchestrator)");
/// Default Hytale server port (UDP, QUIC).
pub const DEFAULT_GAME_PORT: u16 = 5520;
/// Default port of the orchestrator's own web UI.
pub const DEFAULT_WEB_PORT: u16 = 7520;
/// Lines of console output kept in memory per instance.
pub const LOG_CAP: usize = 2000;
/// Telemetry samples kept per instance (one every 2 s: 3 minutes).
pub const HISTORY_CAP: usize = 90;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

// ---------------------------------------------------------------------------------------------
// Folders
// ---------------------------------------------------------------------------------------------

/// Everything the orchestrator writes lives under one data folder.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub instances: PathBuf,
    pub versions: PathBuf,
    pub backups: PathBuf,
    pub tools: PathBuf,
    pub java: PathBuf,
    pub logs: PathBuf,
    pub tmp: PathBuf,
    pub updates: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            instances: root.join("instances"),
            versions: root.join("versions"),
            backups: root.join("backups"),
            tools: root.join("tools"),
            java: root.join("java"),
            logs: root.join("logs"),
            tmp: root.join("tmp"),
            updates: root.join("updates"),
            root,
        }
    }

    /// The per-user location: `%LOCALAPPDATA%\HyOrchestrator\data`, `~/.local/share/HyOrchestrator/data`,
    /// `~/Library/Application Support/HyOrchestrator/data`. Used when the program's own folder can't hold
    /// the data (macOS app bundle, read-only folder, a build from source) and by versions before 0.3.
    pub fn user_root() -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_NAME).join("data")
    }

    /// Where the data goes by default: a `data` folder next to the program, i.e. in the folder chosen
    /// at install time. Returns the folder and a note for the log (a move or a fallback).
    pub fn default_root() -> (PathBuf, Option<String>) {
        let legacy = Self::user_root();
        match std::env::current_exe() {
            Ok(exe) => Self::choose_root(&exe, legacy),
            Err(_) => (legacy, None),
        }
    }

    /// `default_root` for a given program path and per-user folder (testable).
    pub fn choose_root(exe: &Path, legacy: PathBuf) -> (PathBuf, Option<String>) {
        let exe = exe.to_path_buf();
        let in_bundle = cfg!(target_os = "macos") && crate::updater::app_bundle(&exe).is_some();
        if crate::updater::self_built(&exe) || in_bundle {
            return (legacy, None);
        }
        let Some(dir) = exe.parent() else { return (legacy, None) };
        let here = dir.join("data");
        if here.is_dir() || here == legacy {
            return (here, None);
        }
        if !writable(dir) {
            return (legacy.clone(), Some(format!("{} is read-only: data stays in {}", dir.display(), legacy.display())));
        }
        if legacy.is_dir() {
            // data from an older version. Only an installed copy (the setup's uninstaller sits next to it)
            // takes it over, so a stray copy run from Downloads never moves the real data away.
            let installed = std::fs::read_dir(dir).map(|rd| rd.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.starts_with("unins") && n.ends_with(".exe")
            }));
            if !installed.unwrap_or(false) {
                return (legacy, None);
            }
            // bring it next to the program (instant on the same drive)
            return match std::fs::rename(&legacy, &here) {
                Ok(()) => (here.clone(), Some(format!("moved the data from {} to {}", legacy.display(), here.display()))),
                Err(e) => (
                    legacy.clone(),
                    Some(format!(
                        "data stays in {} (it could not be moved next to the program: {e}). Move the folder to {} by hand, or start with --data-dir.",
                        legacy.display(),
                        here.display()
                    )),
                ),
            };
        }
        (here, None)
    }

    pub fn create_all(&self) -> Result<()> {
        for d in [&self.root, &self.instances, &self.versions, &self.backups, &self.tools, &self.java, &self.logs, &self.tmp] {
            std::fs::create_dir_all(d).with_context(|| format!("cannot create {}", d.display()))?;
        }
        Ok(())
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances.join(id)
    }

    /// The Hytale server's working directory (universe/, mods/, config.json, ...).
    pub fn server_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("server")
    }

    pub fn backup_dir(&self, id: &str) -> PathBuf {
        self.backups.join(id)
    }

    fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn registry_file(&self) -> PathBuf {
        self.root.join("instances.json")
    }
}

/// Can we create files in `dir`?
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".hyo-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

// ---------------------------------------------------------------------------------------------
// Settings & instance registry (persisted as JSON)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Path to a Java 25 `java` executable; empty = automatic (managed Java, JAVA_HOME, PATH).
    pub java_path: String,
    /// Port of the web UI.
    pub web_port: u16,
    /// Desktop mode: also accept connections from other computers (headless always does).
    pub lan_access: bool,
    /// Secret that every web UI request must carry (cookie or `?token=`).
    pub web_token: String,
    /// Look for a new HyOrchestrator release on GitHub at start and every 6 hours.
    pub check_updates: bool,
    /// Override the console patterns that detect players joining / leaving (regex, group 1 = name).
    pub join_pattern: String,
    pub leave_pattern: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            java_path: String::new(),
            web_port: DEFAULT_WEB_PORT,
            lan_access: false,
            web_token: new_token(),
            check_updates: true,
            join_pattern: String::new(),
            leave_pattern: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    /// Hytale server version from the local version cache (`versions/<version>`).
    pub version: String,
    /// Channel used by "Update": `release` or `pre-release`.
    pub patchline: String,
    /// UDP port (QUIC) the server binds to on 0.0.0.0.
    pub port: u16,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    /// `--auth-mode offline` (LAN tests without Hytale authentication).
    pub offline_auth: bool,
    /// Use the shipped ahead-of-time cache (`-XX:AOTCache=HytaleServer.aot`) for faster boots.
    pub use_aot: bool,
    pub disable_sentry: bool,
    /// Restart automatically when the server crashes (not after a normal stop).
    pub auto_restart: bool,
    pub jvm_args: String,
    pub server_args: String,
    /// Players made operator through the orchestrator.
    pub operators: Vec<String>,
    pub created_at: i64,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            patchline: "release".into(),
            port: DEFAULT_GAME_PORT,
            min_memory_mb: 2048,
            max_memory_mb: 4096,
            offline_auth: false,
            use_aot: true,
            disable_sentry: false,
            auto_restart: false,
            jvm_args: String::new(),
            server_args: String::new(),
            operators: Vec::new(),
            created_at: 0,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
pub enum Status {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Stopped => "Stopped",
            Status::Starting => "Starting",
            Status::Running => "Running",
            Status::Stopping => "Stopping",
            Status::Crashed => "Crashed",
        }
    }
    pub fn css(self) -> &'static str {
        match self {
            Status::Stopped => "stopped",
            Status::Starting | Status::Stopping => "busy",
            Status::Running => "running",
            Status::Crashed => "crashed",
        }
    }
    pub fn is_active(self) -> bool {
        matches!(self, Status::Starting | Status::Running | Status::Stopping)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LogKind {
    Out,
    Err,
    /// a line written by the orchestrator itself (start, stop, commands)
    System,
    /// a command typed by the user
    Input,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub kind: LogKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    pub at: i64,
    /// share of the whole machine (0-100)
    pub cpu: f32,
    pub ram_mb: u64,
    pub players: usize,
}

/// What a running (or recently stopped) server looks like right now.
#[derive(Default)]
pub struct Runtime {
    pub status: Status,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub stdin: Option<mpsc::UnboundedSender<String>>,
    pub kill: Option<mpsc::UnboundedSender<()>>,
    pub stop_requested: bool,
    pub log: VecDeque<LogLine>,
    pub log_seq: u64,
    pub players: BTreeSet<String>,
    pub cpu: f32,
    pub ram_mb: u64,
    pub history: VecDeque<Sample>,
    /// device-login link printed by `/auth login device`
    pub auth_url: Option<String>,
    pub auth_code: Option<String>,
    pub needs_auth: bool,
    /// a device sign-in was started in this run (automatically or by the button)
    pub auth_requested: bool,
    /// the sign-in page was opened in the browser (desktop mode)
    pub auth_opened: bool,
    pub last_exit: Option<i32>,
    pub crash_times: Vec<i64>,
}

impl Runtime {
    pub fn push_log(&mut self, kind: LogKind, text: impl Into<String>) {
        self.log_seq += 1;
        self.log.push_back(LogLine { seq: self.log_seq, kind, text: text.into() });
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TaskStatus {
    Running,
    Done,
    Failed,
}

/// A Hytale device sign-in waiting for the user: open `url` (it already carries the code) and approve.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LoginPrompt {
    pub url: String,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    /// 0..1 when known
    pub progress: Option<f32>,
    pub message: String,
    pub log: Vec<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// instance the task belongs to (if any)
    pub instance: Option<String>,
    /// the task waits for a Hytale sign-in (the downloader's device code)
    pub login: Option<LoginPrompt>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HostStats {
    pub cpu: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub cpus: usize,
    /// (cpu %, ram %, players online across all servers)
    pub history: VecDeque<(f32, f32, f32)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct JavaInfo {
    pub path: String,
    pub version: String,
    pub major: u32,
    pub error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Desktop,
    Headless,
}

pub struct AppState {
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub instances: RwLock<Vec<InstanceConfig>>,
    pub runtimes: RwLock<HashMap<String, Runtime>>,
    pub tasks: RwLock<Vec<Task>>,
    pub host: RwLock<HostStats>,
    pub java: RwLock<JavaInfo>,
    pub updates: RwLock<crate::updater::UpdateInfo>,
    pub http: reqwest::Client,
    /// main multi-threaded runtime (UI sessions run on their own small runtimes)
    pub rt: tokio::runtime::Handle,
    pub mode: Mode,
    /// how to reach the web UI (with the token), e.g. http://127.0.0.1:7520/?token=…
    pub urls: RwLock<Vec<String>>,
    /// this network's public IP (for the "copy public address" button)
    pub public_ip: RwLock<Option<String>>,
    changes: watch::Sender<u64>,
    pub quitting: AtomicBool,
    pub quit: tokio::sync::Notify,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn load(paths: Paths, mode: Mode, rt: tokio::runtime::Handle) -> Result<Shared> {
        paths.create_all()?;
        let mut settings: Settings = read_json(&paths.settings_file()).unwrap_or_default();
        if settings.web_token.len() < 32 {
            settings.web_token = new_token();
        }
        let instances: Vec<InstanceConfig> = read_json(&paths.registry_file()).unwrap_or_default();
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()?;
        let (changes, _) = watch::channel(0u64);
        let state = Arc::new(Self {
            paths,
            settings: RwLock::new(settings),
            instances: RwLock::new(instances),
            runtimes: RwLock::new(HashMap::new()),
            tasks: RwLock::new(Vec::new()),
            host: RwLock::new(HostStats::default()),
            java: RwLock::new(JavaInfo::default()),
            updates: RwLock::new(Default::default()),
            http,
            rt,
            mode,
            urls: RwLock::new(Vec::new()),
            public_ip: RwLock::new(None),
            changes,
            quitting: AtomicBool::new(false),
            quit: tokio::sync::Notify::new(),
        });
        state.save_settings()?;
        Ok(state)
    }

    /// Tells every open UI that something changed.
    pub fn notify(&self) {
        self.changes.send_modify(|v| *v = v.wrapping_add(1));
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub fn save_settings(&self) -> Result<()> {
        let s = self.settings.read().clone();
        write_json(&self.paths.settings_file(), &s)
    }

    pub fn save_instances(&self) -> Result<()> {
        let list = self.instances.read().clone();
        write_json(&self.paths.registry_file(), &list)
    }

    pub fn instance(&self, id: &str) -> Option<InstanceConfig> {
        self.instances.read().iter().find(|i| i.id == id).cloned()
    }

    pub fn update_instance(&self, id: &str, f: impl FnOnce(&mut InstanceConfig)) -> Result<()> {
        {
            let mut list = self.instances.write();
            let inst = list.iter_mut().find(|i| i.id == id).ok_or_else(|| anyhow::anyhow!("instance not found"))?;
            f(inst);
        }
        self.save_instances()?;
        self.notify();
        Ok(())
    }

    pub fn status(&self, id: &str) -> Status {
        self.runtimes.read().get(id).map(|r| r.status).unwrap_or_default()
    }

    pub fn with_runtime<T>(&self, id: &str, f: impl FnOnce(&mut Runtime) -> T) -> T {
        let mut map = self.runtimes.write();
        f(map.entry(id.to_string()).or_default())
    }

    pub fn system_log(&self, id: &str, text: impl Into<String>) {
        let text = text.into();
        crate::logln(&format!("[{id}] {text}"));
        self.with_runtime(id, |r| r.push_log(LogKind::System, text));
        self.notify();
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::Relaxed)
    }

    /// Asks the program to end (window closed, Ctrl+C, update installed). Servers are stopped first.
    pub fn request_quit(&self) {
        self.quitting.store(true, Ordering::Relaxed);
        self.quit.notify_waiters();
        self.quit.notify_one();
    }

    // --- tasks -----------------------------------------------------------------------------------

    pub fn task_start(&self, title: impl Into<String>, instance: Option<&str>) -> String {
        let id = new_id();
        let mut tasks = self.tasks.write();
        tasks.insert(
            0,
            Task {
                id: id.clone(),
                title: title.into(),
                status: TaskStatus::Running,
                progress: None,
                message: "Starting…".into(),
                log: Vec::new(),
                started_at: now_ms(),
                ended_at: None,
                instance: instance.map(str::to_string),
                login: None,
            },
        );
        tasks.truncate(40);
        drop(tasks);
        self.notify();
        id
    }

    pub fn task_update(&self, id: &str, message: Option<&str>, progress: Option<f32>) {
        if let Some(t) = self.tasks.write().iter_mut().find(|t| t.id == id) {
            if let Some(m) = message {
                t.message = m.to_string();
            }
            if progress.is_some() {
                t.progress = progress;
            }
        }
        self.notify();
    }

    pub fn task_log(&self, id: &str, line: impl Into<String>) {
        let line = line.into();
        if let Some(t) = self.tasks.write().iter_mut().find(|t| t.id == id) {
            t.log.push(line);
            if t.log.len() > 400 {
                t.log.remove(0);
            }
        }
        self.notify();
    }

    /// Shows (Some) or clears (None) a sign-in prompt on a task.
    pub fn task_login(&self, id: &str, login: Option<LoginPrompt>) {
        if let Some(t) = self.tasks.write().iter_mut().find(|t| t.id == id) {
            t.login = login;
        }
        self.notify();
    }

    pub fn task_finish(&self, id: &str, result: &Result<String>) {
        if let Some(t) = self.tasks.write().iter_mut().find(|t| t.id == id) {
            t.ended_at = Some(now_ms());
            t.login = None;
            match result {
                Ok(msg) => {
                    t.status = TaskStatus::Done;
                    t.progress = Some(1.0);
                    t.message = msg.clone();
                }
                Err(e) => {
                    t.status = TaskStatus::Failed;
                    t.message = format!("{e:#}");
                }
            }
            crate::logln(&format!("[task] {}: {}", t.title, t.message));
        }
        self.notify();
    }

    pub fn running_task_for(&self, instance: &str) -> Option<Task> {
        self.tasks.read().iter().find(|t| t.status == TaskStatus::Running && t.instance.as_deref() == Some(instance)).cloned()
    }
}

/// Runs `job` as a background task shown in the UI's task list.
pub fn spawn_task<F, Fut>(state: &Shared, title: impl Into<String>, instance: Option<&str>, job: F) -> String
where
    F: FnOnce(Shared, String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<String>> + Send + 'static,
{
    let id = state.task_start(title, instance);
    let (st, tid) = (state.clone(), id.clone());
    state.rt.spawn(async move {
        let result = job(st.clone(), tid.clone()).await;
        st.task_finish(&tid, &result);
    });
    id
}

// ---------------------------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------------------------

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes atomically (temp file + rename) so a crash never leaves half a file.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[i]) }
}

pub fn human_duration(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let (d, h, m, sec) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60, s % 60);
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {sec}s")
    } else {
        format!("{sec}s")
    }
}

pub fn fmt_time(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_millis_opt(ms).single().map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_and_durations() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(human_duration(59_000), "59s");
        assert_eq!(human_duration(61_000), "1m 1s");
        assert_eq!(human_duration(3_726_000), "1h 2m");
        assert_eq!(human_duration(90_000_000), "1d 1h 0m");
    }

    #[test]
    fn settings_fill_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"web_port": 9000}"#).unwrap();
        assert_eq!(s.web_port, 9000);
        assert!(s.check_updates);
        let i: InstanceConfig = serde_json::from_str(r#"{"id": "a", "name": "x"}"#).unwrap();
        assert_eq!(i.port, DEFAULT_GAME_PORT);
        assert_eq!(i.patchline, "release");
    }

    #[test]
    fn data_lives_next_to_the_installed_program() {
        let t = std::env::temp_dir().join(format!("hyo-root-{}", new_id()));
        let app = t.join("Games").join("HyOrchestrator");
        let legacy = t.join("LocalAppData").join("HyOrchestrator").join("data");
        std::fs::create_dir_all(&app).unwrap();
        let exe = app.join("hyorchestrator.exe");
        std::fs::write(&exe, b"").unwrap();

        // fresh install, nothing old: data next to the program
        assert_eq!(Paths::choose_root(&exe, legacy.clone()).0, app.join("data"));

        // old data + a stray copy (no uninstaller next to it): the old data is not touched
        std::fs::create_dir_all(legacy.join("instances")).unwrap();
        assert_eq!(Paths::choose_root(&exe, legacy.clone()), (legacy.clone(), None));
        assert!(legacy.is_dir());

        // old data + an installed copy: moved next to the program
        std::fs::write(app.join("unins000.exe"), b"").unwrap();
        let (root, note) = Paths::choose_root(&exe, legacy.clone());
        assert_eq!(root, app.join("data"));
        assert!(note.unwrap().contains("moved"));
        assert!(app.join("data").join("instances").is_dir() && !legacy.exists());

        // from now on that folder is used as it is
        assert_eq!(Paths::choose_root(&exe, legacy.clone()), (app.join("data"), None));

        // a build from source keeps using the per-user folder
        let dev = t.join("repo").join("target").join("debug").join("hyorchestrator.exe");
        assert_eq!(Paths::choose_root(&dev, legacy.clone()).0, legacy);
        let _ = std::fs::remove_dir_all(t);
    }

    #[test]
    fn json_roundtrip_is_atomic() {
        let dir = std::env::temp_dir().join(format!("hyo-json-{}", new_id()));
        let f = dir.join("a.json");
        write_json(&f, &vec![1, 2, 3]).unwrap();
        let back: Vec<i32> = read_json(&f).unwrap();
        assert_eq!(back, vec![1, 2, 3]);
        assert!(!f.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
