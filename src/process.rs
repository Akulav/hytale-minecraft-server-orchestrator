//! Hytale server processes: build the `java` command (std `Command`), spawn it, pump its console into
//! the instance log, read players / login prompts from it, send console commands, stop and kill.

use anyhow::{Context, Result, anyhow, bail};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::state::{InstanceConfig, LogKind, Shared, Status, now_ms};

/// How long a server gets to save and exit after `/stop` before it is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(45);
/// Auto-restart gives up after this many crashes within 10 minutes.
const MAX_CRASHES: usize = 3;

/// No console window for child processes on Windows.
pub fn no_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

pub fn strip_ansi(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07").unwrap());
    re.replace_all(s, "").replace('\r', "")
}

fn split_args(s: &str) -> Vec<String> {
    // simple shell-like split: spaces, with "double quotes" for values containing spaces
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The full argument list after `java` (JVM options, jar, server options).
pub fn server_args(inst: &InstanceConfig, assets: &std::path::Path, has_aot: bool) -> Vec<String> {
    let mut a = Vec::new();
    if inst.min_memory_mb > 0 {
        a.push(format!("-Xms{}M", inst.min_memory_mb.min(inst.max_memory_mb.max(inst.min_memory_mb))));
    }
    if inst.max_memory_mb > 0 {
        a.push(format!("-Xmx{}M", inst.max_memory_mb));
    }
    if inst.use_aot && has_aot {
        a.push("-XX:AOTCache=HytaleServer.aot".into());
    }
    a.extend(split_args(&inst.jvm_args));
    a.push("-jar".into());
    a.push("HytaleServer.jar".into());
    a.push("--assets".into());
    a.push(assets.display().to_string());
    a.push("--bind".into());
    a.push(format!("0.0.0.0:{}", inst.port));
    if inst.offline_auth {
        a.push("--auth-mode".into());
        a.push("offline".into());
    }
    if inst.disable_sentry {
        a.push("--disable-sentry".into());
    }
    a.extend(split_args(&inst.server_args));
    a
}

/// Is the UDP port free on all interfaces?
pub fn udp_port_free(port: u16) -> bool {
    std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

pub async fn start(state: &Shared, id: &str) -> Result<()> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    if state.status(id).is_active() {
        bail!("{} is already running", inst.name);
    }
    if let Some(t) = state.running_task_for(id) {
        bail!("wait for \"{}\" to finish", t.title);
    }
    if let Some(other) = state.instances.read().iter().find(|o| o.id != id && o.port == inst.port && state.status(&o.id).is_active()) {
        bail!("port {} is used by the running instance \"{}\"", inst.port, other.name);
    }
    if !udp_port_free(inst.port) {
        bail!("UDP port {} is in use by another program", inst.port);
    }
    let dir = state.paths.server_dir(id);
    if !dir.join("HytaleServer.jar").is_file() {
        if crate::versions::is_complete(state, &inst.version) {
            crate::instance::install_version_files(state, id, &inst.version)?;
        } else {
            bail!("server files for {} are missing - pick a version in the instance settings", inst.version);
        }
    }
    let assets = crate::versions::assets_zip(state, &inst.version);
    if !assets.is_file() {
        bail!("Assets.zip of version {} is missing - download the version again", inst.version);
    }
    let java = crate::java::require(state).await?;
    let args = server_args(&inst, &assets, dir.join("HytaleServer.aot").is_file());

    let mut cmd = std::process::Command::new(&java);
    cmd.args(&args)
        .current_dir(&dir)
        // the orchestrator handles updates (versions + 1-click update); the server's own updater would
        // fight with it
        .env("HYTALE_DISABLE_UPDATES", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        // the server ends when the orchestrator dies (no orphaned servers holding the port)
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    state.system_log(id, format!("Starting {} ({}) on UDP port {}", inst.name, inst.version, inst.port));
    state.system_log(id, format!("> {} {}", java.display(), args.join(" ")));
    let mut child = tokio::process::Command::from(cmd).kill_on_drop(true).spawn().with_context(|| format!("cannot start {}", java.display()))?;

    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    let (kill_tx, mut kill_rx) = mpsc::unbounded_channel::<()>();
    let pid = child.id();
    #[cfg(windows)]
    if let Some(pid) = pid {
        job::adopt(pid);
    }
    state.with_runtime(id, |r| {
        r.status = Status::Starting;
        r.pid = pid;
        r.started_at = Some(now_ms());
        r.stdin = Some(stdin_tx);
        r.kill = Some(kill_tx);
        r.stop_requested = false;
        r.players.clear();
        r.history.clear();
        r.auth_url = None;
        r.last_exit = None;
    });
    state.notify();

    // console input
    if let Some(mut stdin) = child.stdin.take() {
        tokio::spawn(async move {
            while let Some(line) = stdin_rx.recv().await {
                if stdin.write_all(format!("{line}\n").as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });
    }
    // console output
    let pumps: Vec<_> = [(child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), LogKind::Out), (child.stderr.take().map(|s| Box::new(s) as _), LogKind::Err)]
        .into_iter()
        .filter_map(|(r, kind)| r.map(|r| (r, kind)))
        .map(|(r, kind)| {
            let (st, id) = (state.clone(), id.to_string());
            tokio::spawn(async move {
                let mut lines = BufReader::new(r).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => on_line(&st, &id, kind, &strip_ansi(&line)),
                        Ok(None) => break,
                        Err(_) => continue, // non-UTF-8 output: skip the line
                    }
                }
            })
        })
        .collect();

    // a server that prints nothing we recognise is considered up after 2 minutes
    {
        let (st, id) = (state.clone(), id.to_string());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(120)).await;
            st.with_runtime(&id, |r| {
                if r.status == Status::Starting && r.pid == pid {
                    r.status = Status::Running;
                }
            });
            st.notify();
        });
    }

    // wait for the end
    let (st, id) = (state.clone(), id.to_string());
    tokio::spawn(async move {
        let status = tokio::select! {
            s = child.wait() => s,
            _ = kill_rx.recv() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        for p in pumps {
            let _ = tokio::time::timeout(Duration::from_secs(2), p).await;
        }
        let code = status.as_ref().ok().and_then(|s| s.code());
        let requested = st.with_runtime(&id, |r| r.stop_requested);
        let crashed = !requested && !matches!(code, Some(0));
        st.with_runtime(&id, |r| {
            r.status = if crashed { Status::Crashed } else { Status::Stopped };
            r.pid = None;
            r.stdin = None;
            r.kill = None;
            r.started_at = None;
            r.players.clear();
            r.cpu = 0.0;
            r.ram_mb = 0;
            r.last_exit = code;
            if crashed {
                let now = now_ms();
                r.crash_times.retain(|t| now - t < 600_000);
                r.crash_times.push(now);
            }
        });
        let msg = match code {
            Some(c) => format!("Server exited with code {c}"),
            None => "Server was terminated".to_string(),
        };
        st.system_log(&id, if crashed { format!("{msg} (crash)") } else { msg });
        if crashed && !st.is_quitting() {
            let (auto, recent) = (st.instance(&id).is_some_and(|i| i.auto_restart), st.with_runtime(&id, |r| r.crash_times.len()));
            if auto && recent <= MAX_CRASHES {
                st.system_log(&id, format!("Auto-restart in 5 s (crash {recent} of {MAX_CRASHES} allowed in 10 min)"));
                tokio::time::sleep(Duration::from_secs(5)).await;
                if !st.status(&id).is_active()
                    && !st.is_quitting()
                    && let Err(e) = start_boxed(st.clone(), id.clone()).await
                {
                    st.system_log(&id, format!("Auto-restart failed: {e:#}"));
                }
            } else if auto {
                st.system_log(&id, "Auto-restart stopped: too many crashes in 10 minutes");
            }
        }
    });
    Ok(())
}

/// Windows: servers join a job object that ends them when HyOrchestrator ends - even if it crashes - so no
/// orphaned server keeps holding its port (Linux does the same with the parent-death signal). A normal
/// quit stops them cleanly with `/stop` first.
#[cfg(windows)]
mod job {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    struct Job(HANDLE);
    // the handle is only passed to thread-safe Win32 calls and never closed (the OS closes it at exit)
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> Option<HANDLE> {
        static JOB: OnceLock<Option<Job>> = OnceLock::new();
        JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(h);
                return None;
            }
            Some(Job(h))
        })
        .as_ref()
        .map(|j| j.0)
    }

    pub fn adopt(pid: u32) {
        let Some(job) = job() else { return };
        unsafe {
            let p = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if !p.is_null() {
                AssignProcessToJobObject(job, p);
                CloseHandle(p);
            }
        }
    }
}

/// `start` behind a box, for the auto-restart inside `start` itself (a recursive async fn).
fn start_boxed(state: Shared, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
    Box::pin(async move { start(&state, &id).await })
}

/// Built-in console patterns (Hytale's exact log wording can change between versions; Settings can
/// override them). Group 1 is the player name.
const JOIN_PATTERNS: &str = r"(?i)(?:\bAdding player '([^']+)'|\bplayer '?([A-Za-z0-9_]{2,32})'? (?:has )?(?:joined|connected)|^\W*(?:\[[^\]]*\]\s*)*([A-Za-z0-9_]{2,32}) (?:has )?joined (?:the (?:game|server|world)|world))";
const LEAVE_PATTERNS: &str = r"(?i)(?:\bRemoving player '([^']+)'|\bplayer '?([A-Za-z0-9_]{2,32})'? (?:has )?(?:left|disconnected)|^\W*(?:\[[^\]]*\]\s*)*([A-Za-z0-9_]{2,32}) (?:has )?(?:left|disconnected)(?: the (?:game|server|world))?)";

fn first_group(re: &regex::Regex, line: &str) -> Option<String> {
    let c = re.captures(line)?;
    (1..c.len()).find_map(|i| c.get(i)).map(|m| m.as_str().to_string())
}

pub struct LinePatterns {
    join: regex::Regex,
    leave: regex::Regex,
}

impl LinePatterns {
    pub fn new(join: &str, leave: &str) -> Self {
        let pick = |custom: &str, default: &str| regex::Regex::new(if custom.trim().is_empty() { default } else { custom }).unwrap_or_else(|_| regex::Regex::new(default).unwrap());
        Self { join: pick(join, JOIN_PATTERNS), leave: pick(leave, LEAVE_PATTERNS) }
    }
}

#[derive(Debug, PartialEq)]
pub enum LineEvent {
    Joined(String),
    Left(String),
    AuthUrl(String),
    AuthNeeded,
    AuthOk,
    Ready,
}

/// What a console line tells us.
pub fn classify(p: &LinePatterns, line: &str) -> Vec<LineEvent> {
    static AUTH_URL: OnceLock<regex::Regex> = OnceLock::new();
    static READY: OnceLock<regex::Regex> = OnceLock::new();
    let auth_url = AUTH_URL.get_or_init(|| regex::Regex::new(r"https://accounts\.hytale\.com/device\S*").unwrap());
    let ready = READY.get_or_init(|| regex::Regex::new(r"(?i)(server (?:has )?(?:started|booted)|hytale server booted|server is (?:now )?(?:ready|running)|listening on|done \([0-9.]+s\))").unwrap());
    let mut ev = Vec::new();
    let lower = line.to_lowercase();
    if let Some(m) = auth_url.find(line) {
        ev.push(LineEvent::AuthUrl(m.as_str().trim_end_matches(['.', ',', ')']).to_string()));
    }
    if lower.contains("authentication successful") {
        ev.push(LineEvent::AuthOk);
    } else if lower.contains("/auth login") || lower.contains("not authenticated") || lower.contains("no server tokens") || lower.contains("authentication required") {
        ev.push(LineEvent::AuthNeeded);
    }
    if let Some(n) = first_group(&p.leave, line) {
        ev.push(LineEvent::Left(n));
    } else if let Some(n) = first_group(&p.join, line) {
        ev.push(LineEvent::Joined(n));
    }
    if ready.is_match(line) {
        ev.push(LineEvent::Ready);
    }
    ev
}

fn patterns(state: &Shared) -> std::sync::Arc<LinePatterns> {
    static CACHE: OnceLock<parking_lot::Mutex<(String, String, std::sync::Arc<LinePatterns>)>> = OnceLock::new();
    let (j, l) = {
        let s = state.settings.read();
        (s.join_pattern.clone(), s.leave_pattern.clone())
    };
    let cache = CACHE.get_or_init(|| parking_lot::Mutex::new((j.clone(), l.clone(), std::sync::Arc::new(LinePatterns::new(&j, &l)))));
    let mut c = cache.lock();
    if c.0 != j || c.1 != l {
        *c = (j.clone(), l.clone(), std::sync::Arc::new(LinePatterns::new(&j, &l)));
    }
    c.2.clone()
}

fn on_line(state: &Shared, id: &str, kind: LogKind, line: &str) {
    let events = classify(&patterns(state), line);
    state.with_runtime(id, |r| {
        r.push_log(kind, line);
        for e in events {
            match e {
                LineEvent::Joined(n) => {
                    r.players.insert(n);
                }
                LineEvent::Left(n) => {
                    r.players.remove(&n);
                }
                LineEvent::AuthUrl(u) => {
                    r.auth_url = Some(u);
                    r.needs_auth = true;
                }
                LineEvent::AuthNeeded => r.needs_auth = true,
                LineEvent::AuthOk => {
                    r.needs_auth = false;
                    r.auth_url = None;
                }
                LineEvent::Ready => {
                    if r.status == Status::Starting {
                        r.status = Status::Running;
                    }
                }
            }
        }
    });
    state.notify();
}

/// Sends a console command (with a leading `/`, as in the Hytale console).
pub fn send_command(state: &Shared, id: &str, command: &str) -> Result<()> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Ok(());
    }
    let cmd = if cmd.starts_with('/') { cmd.to_string() } else { format!("/{cmd}") };
    let tx = state.with_runtime(id, |r| r.stdin.clone()).ok_or_else(|| anyhow!("the server is not running"))?;
    tx.send(cmd.clone()).map_err(|_| anyhow!("the server console is closed"))?;
    state.with_runtime(id, |r| r.push_log(LogKind::Input, format!("> {cmd}")));
    state.notify();
    Ok(())
}

/// Asks the server to save and stop; kills it if it does not exit in time.
pub async fn stop(state: &Shared, id: &str) -> Result<()> {
    if !state.status(id).is_active() {
        return Ok(());
    }
    let pid = state.with_runtime(id, |r| {
        r.stop_requested = true;
        r.status = Status::Stopping;
        r.pid
    });
    state.system_log(id, "Stopping (sending /stop)…");
    if send_command(state, id, "/stop").is_err() {
        kill(state, id);
    }
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    while state.with_runtime(id, |r| r.pid.is_some() && r.pid == pid) {
        if tokio::time::Instant::now() > deadline {
            state.system_log(id, format!("No exit after {} s - killing the server", STOP_TIMEOUT.as_secs()));
            kill(state, id);
            tokio::time::sleep(Duration::from_secs(3)).await;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Ok(())
}

pub fn kill(state: &Shared, id: &str) {
    let tx = state.with_runtime(id, |r| {
        r.stop_requested = true;
        r.kill.clone()
    });
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}

pub async fn restart(state: &Shared, id: &str) -> Result<()> {
    stop(state, id).await?;
    start(state, id).await
}

/// Stops every running server (in parallel). Used when the program ends.
pub async fn stop_all(state: &Shared) {
    let ids: Vec<String> = state.runtimes.read().iter().filter(|(_, r)| r.status.is_active()).map(|(k, _)| k.clone()).collect();
    let jobs = ids.iter().map(|id| stop(state, id));
    futures::future::join_all(jobs).await;
}

/// Makes a player operator (`/op add`) or removes it (`/op remove`), and remembers the list.
pub fn set_operator(state: &Shared, id: &str, player: &str, op: bool) -> Result<()> {
    let player = player.trim();
    if player.is_empty() || player.len() > 64 || player.contains(char::is_whitespace) {
        bail!("enter one player name (or UUID)");
    }
    if !state.status(id).is_active() {
        bail!("start the server first: operators are set through its console");
    }
    send_command(state, id, &format!("/op {} {player}", if op { "add" } else { "remove" }))?;
    state.update_instance(id, |i| {
        i.operators.retain(|p| !p.eq_ignore_ascii_case(player));
        if op {
            i.operators.push(player.to_string());
            i.operators.sort_by_key(|p| p.to_lowercase());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_server_command_line() {
        let inst = InstanceConfig { port: 5521, max_memory_mb: 6144, min_memory_mb: 2048, offline_auth: true, jvm_args: "-XX:+UseG1GC \"-Dname=a b\"".into(), server_args: "--disable-sentry".into(), ..Default::default() };
        let a = server_args(&inst, std::path::Path::new("/data/versions/v1/Assets.zip"), true);
        let s = a.join(" ");
        assert!(s.starts_with("-Xms2048M -Xmx6144M -XX:AOTCache=HytaleServer.aot -XX:+UseG1GC -Dname=a b -jar HytaleServer.jar"), "{s}");
        assert!(s.contains("--assets /data/versions/v1/Assets.zip --bind 0.0.0.0:5521 --auth-mode offline"), "{s}");
        assert!(a.contains(&"-Dname=a b".to_string()));
        let no_aot = server_args(&inst, std::path::Path::new("A.zip"), false);
        assert!(!no_aot.iter().any(|x| x.contains("AOTCache")));
    }

    #[test]
    fn reads_players_and_login_prompts() {
        let p = LinePatterns::new("", "");
        assert_eq!(classify(&p, "[2026/01/13 10:00:00 INFO] [Universe] Adding player 'Steve' to world default"), vec![LineEvent::Joined("Steve".into())]);
        assert_eq!(classify(&p, "[INFO] Removing player 'Steve' from world"), vec![LineEvent::Left("Steve".into())]);
        assert_eq!(classify(&p, "[Server] Alex joined the game"), vec![LineEvent::Joined("Alex".into())]);
        assert_eq!(classify(&p, "Player Alex disconnected"), vec![LineEvent::Left("Alex".into())]);
        let ev = classify(&p, "Or visit: https://accounts.hytale.com/device?user_code=ABCD-1234");
        assert_eq!(ev, vec![LineEvent::AuthUrl("https://accounts.hytale.com/device?user_code=ABCD-1234".into())]);
        assert_eq!(classify(&p, "> Authentication successful! Mode: OAUTH_DEVICE"), vec![LineEvent::AuthOk]);
        assert!(classify(&p, "Hytale Server Booted! [Multiplayer]").contains(&LineEvent::Ready));
        assert!(classify(&p, "Loading assets…").is_empty());
        // a custom pattern replaces the built-in one
        let custom = LinePatterns::new(r"JOIN:(\w+)", "");
        assert_eq!(classify(&custom, "JOIN:Bob"), vec![LineEvent::Joined("Bob".into())]);
    }

    #[test]
    fn strips_terminal_colours() {
        assert_eq!(strip_ansi("\x1b[32mINFO\x1b[0m ready\r"), "INFO ready");
    }

    #[test]
    fn splits_arguments_with_quotes() {
        assert_eq!(split_args(" -a  \"b c\" d "), vec!["-a", "b c", "d"]);
        assert!(split_args("   ").is_empty());
    }
}
