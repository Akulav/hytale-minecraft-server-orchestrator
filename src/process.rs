//! Server processes (Hytale and Minecraft): build the `java` command (std `Command`), spawn it, pump its
//! console into the instance log, read players / login prompts from it, send console commands, stop and kill.

use anyhow::{Context, Result, anyhow, bail};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::state::{Game, InstanceConfig, LogKind, Shared, Status, now_ms};

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

/// Reads `r` and calls `f` for every line, splitting on `\n` and `\r` (progress bars redraw with `\r`).
/// Invalid UTF-8 is replaced, never fatal.
pub async fn read_segments<R: tokio::io::AsyncRead + Unpin>(mut r: R, mut f: impl FnMut(String)) {
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 16 * 1024];
    let mut cur: Vec<u8> = Vec::new();
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                if !cur.is_empty() {
                    f(String::from_utf8_lossy(&cur).into_owned());
                    cur.clear();
                }
            } else if cur.len() < 64 * 1024 {
                cur.push(b);
            }
        }
    }
    if !cur.is_empty() {
        f(String::from_utf8_lossy(&cur).into_owned());
    }
}

pub fn strip_ansi(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07").unwrap());
    re.replace_all(s, "").replace('\r', "")
}

pub(crate) fn split_args(s: &str) -> Vec<String> {
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

/// Is the game's port free (UDP for Hytale, TCP for Minecraft)?
pub fn port_free(game: Game, port: u16) -> bool {
    match game {
        Game::Hytale => udp_port_free(port),
        Game::Minecraft => std::net::TcpListener::bind(("0.0.0.0", port)).is_ok(),
    }
}

/// What to run: java, its arguments, extra environment, lines for the console log.
struct Launch {
    java: std::path::PathBuf,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
    notes: Vec<String>,
}

async fn hytale_launch(state: &Shared, id: &str, inst: &InstanceConfig) -> Result<Launch> {
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
    let java = if inst.java_path.trim().is_empty() { crate::java::require(state).await? } else { std::path::PathBuf::from(inst.java_path.trim()) };
    let mut args = server_args(inst, &assets, dir.join("HytaleServer.aot").is_file());
    let mut env = vec![
        // the orchestrator handles updates (versions + 1-click update); the server's own updater would
        // fight with it
        ("HYTALE_DISABLE_UPDATES", "1".to_string()),
    ];
    let mut notes = Vec::new();
    // the saved Hytale account signs the server in: no prompt (token passthrough, see account.rs)
    if !inst.offline_auth {
        match crate::account::session_for_server(state).await {
            Ok(Some((s, i, owner))) => {
                args.push("--owner-uuid".into());
                args.push(owner);
                // environment, not arguments: tokens never show up in the logged command line
                env.push(("HYTALE_SERVER_SESSION_TOKEN", s));
                env.push(("HYTALE_SERVER_IDENTITY_TOKEN", i));
                let name = crate::account::load(state).map(|a| a.profile_name()).unwrap_or_default();
                notes.push(format!("Signed in with your Hytale account ({name}) - no sign-in prompt needed"));
            }
            Ok(None) => {}
            Err(e) => notes.push(format!("Hytale account: {e:#} - the server will ask to sign in instead")),
        }
    }
    Ok(Launch { java, args, env, notes })
}

async fn minecraft_launch(state: &Shared, id: &str, inst: &InstanceConfig) -> Result<Launch> {
    if inst.launch.is_empty() {
        bail!("the server software is not installed yet - wait for the install task, or reinstall it in Settings");
    }
    let dir = state.paths.server_dir(id);
    if !crate::minecraft::eula_accepted(&dir) {
        bail!("accept the Minecraft EULA first (instance Settings)");
    }
    crate::minecraft::sync_port(state, inst)?;
    let java = match crate::java::pick_minecraft(state, inst).await {
        crate::java::McJava::Ready(p) => p,
        crate::java::McJava::Missing(major) => {
            crate::actions::install_java_major(state, major);
            bail!("this server needs Java {major} - installing it now (see Tasks); press Start again when it is done");
        }
    };
    Ok(Launch { java, args: crate::minecraft::launch_args(inst), env: Vec::new(), notes: Vec::new() })
}

pub async fn start(state: &Shared, id: &str) -> Result<()> {
    if let Some(t) = state.running_task_for(id) {
        bail!("wait for \"{}\" to finish", t.title);
    }
    start_inner(state, id).await
}

/// `start` for a task of this very instance (update, version switch): its own task doesn't block it.
pub async fn start_from_task(state: &Shared, id: &str) -> Result<()> {
    start_inner(state, id).await
}

async fn start_inner(state: &Shared, id: &str) -> Result<()> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    // claim the instance before any await, so a double click can't start two servers
    let claimed = state.with_runtime(id, |r| {
        if r.status.is_active() {
            return None;
        }
        let prev = r.status;
        r.status = Status::Starting;
        r.stop_requested = false;
        Some(prev)
    });
    let Some(prev) = claimed else { bail!("{} is already running", inst.name) };
    state.notify();
    let result = start_claimed(state, id, &inst).await;
    if result.is_err() {
        // nothing was spawned: back to how it was
        state.with_runtime(id, |r| {
            if r.pid.is_none() {
                r.status = prev;
            }
        });
        state.notify();
    }
    result
}

async fn start_claimed(state: &Shared, id: &str, inst: &InstanceConfig) -> Result<()> {
    if let Some(other) = state.instances.read().iter().find(|o| o.id != id && o.port == inst.port && state.status(&o.id).is_active()) {
        bail!("port {} is used by the running instance \"{}\"", inst.port, other.name);
    }
    if !port_free(inst.game, inst.port) {
        bail!("{} port {} is in use by another program", inst.game.protocol(), inst.port);
    }
    let dir = state.paths.server_dir(id);
    let Launch { java, args, env, notes } = match inst.game {
        Game::Hytale => hytale_launch(state, id, inst).await?,
        Game::Minecraft => minecraft_launch(state, id, inst).await?,
    };
    if state.with_runtime(id, |r| r.stop_requested) || state.is_quitting() {
        bail!("start cancelled");
    }

    let mut cmd = std::process::Command::new(&java);
    cmd.envs(env.iter().map(|(k, v)| (*k, v.as_str())));
    cmd.args(&args).current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
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
    let what = match inst.game {
        Game::Hytale => format!("Hytale {}", inst.version),
        Game::Minecraft => crate::instance::software_label(inst),
    };
    state.system_log(id, format!("Starting {} ({what}) on {} port {}", inst.name, inst.game.protocol(), inst.port));
    for n in notes {
        state.system_log(id, n);
    }
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
        r.auth_code = None;
        r.auth_requested = false;
        r.auth_opened = false;
        r.needs_auth = false;
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
            tokio::spawn(async move { read_segments(r, |line| on_line(&st, &id, kind, &strip_ansi(&line))).await })
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

/// Windows: servers join a job object that ends them when Hyve ends - even if it crashes - so no
/// orphaned server keeps holding its port (Linux does the same with the parent-death signal). A normal
/// quit stops them cleanly with `/stop` first.
#[cfg(windows)]
pub mod job {
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
/// Minecraft lines look like `[12:00:00] [Server thread/INFO]: Steve joined the game` (Forge adds a
/// `[minecraft/MinecraftServer]` bracket), hence the optional `:` after the brackets.
const JOIN_PATTERNS: &str = r"(?i)(?:\bAdding player '([^']+)'|\bplayer '?([A-Za-z0-9_]{2,32})'? (?:has )?(?:joined|connected)|^\W*(?:\[[^\]]*\]\s*)*:?\s*([A-Za-z0-9_]{2,32}) (?:has )?joined (?:the (?:game|server|world)|world))";
const LEAVE_PATTERNS: &str = r"(?i)(?:\bRemoving player '([^']+)'|\bplayer '?([A-Za-z0-9_]{2,32})'? (?:has )?(?:left|disconnected)|^\W*(?:\[[^\]]*\]\s*)*:?\s*([A-Za-z0-9_]{2,32}) (?:has )?(?:left|disconnected)(?: the (?:game|server|world))?)";

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
    AuthCode(String),
    AuthNeeded,
    AuthOk,
    Ready,
}

/// What a console line tells us.
pub fn classify(p: &LinePatterns, line: &str) -> Vec<LineEvent> {
    static READY: OnceLock<regex::Regex> = OnceLock::new();
    let ready = READY.get_or_init(|| regex::Regex::new(r"(?i)(server (?:has )?(?:started|booted)|hytale server booted|server is (?:now )?(?:ready|running)|listening on|done \([0-9.]+s\))").unwrap());
    let mut ev = Vec::new();
    let lower = line.to_lowercase();
    for bit in crate::login::parse(line) {
        match bit {
            crate::login::Bit::Url(u) => ev.push(LineEvent::AuthUrl(u)),
            crate::login::Bit::Code(c) => ev.push(LineEvent::AuthCode(c)),
            crate::login::Bit::Progress(_) => {}
        }
    }
    if lower.contains("authentication successful") {
        ev.push(LineEvent::AuthOk);
    } else if lower.contains("/auth login")
        || lower.contains("not authenticated")
        || lower.contains("starting unauthenticated")
        || lower.contains("no server tokens")
        || lower.contains("authentication required")
    {
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

/// What `on_line` decided to do once the runtime lock is released.
#[derive(Default)]
struct Follow {
    start_login: bool,
    open_url: Option<String>,
    persist: bool,
}

fn on_line(state: &Shared, id: &str, kind: LogKind, line: &str) {
    let mut events = classify(&patterns(state), line);
    let (offline, game) = state.instance(id).map(|i| (i.offline_auth, i.game)).unwrap_or_default();
    if game == Game::Minecraft {
        // Hytale sign-in prompts don't exist there
        events.retain(|e| matches!(e, LineEvent::Joined(_) | LineEvent::Left(_) | LineEvent::Ready));
    }
    let follow = state.with_runtime(id, |r| {
        let mut f = Follow::default();
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
                    // the link that carries the code wins over the plain one
                    if r.auth_url.as_deref().is_none_or(|old| !old.contains("user_code=") || u.contains("user_code=")) {
                        if u.contains("user_code=") && !r.auth_opened {
                            r.auth_opened = true;
                            f.open_url = Some(u.clone());
                        }
                        r.auth_url = Some(u);
                    }
                    r.needs_auth = true;
                }
                LineEvent::AuthCode(c) => r.auth_code = Some(c),
                LineEvent::AuthNeeded => {
                    r.needs_auth = true;
                    // start the device sign-in by ourselves, once per run
                    if !offline && !r.auth_requested {
                        r.auth_requested = true;
                        f.start_login = true;
                    }
                }
                LineEvent::AuthOk => {
                    r.needs_auth = false;
                    r.auth_url = None;
                    r.auth_code = None;
                    f.persist = true;
                }
                LineEvent::Ready => {
                    if r.status == Status::Starting {
                        r.status = Status::Running;
                    }
                }
            }
        }
        f
    });
    if let Some(u) = follow.open_url {
        crate::login::auto_open(state, &u);
    }
    if follow.start_login {
        let _ = send_command(state, id, "/auth login device");
    }
    if follow.persist {
        // keep the login across restarts (stored encrypted in auth.enc / auth.key in the server folder)
        let _ = send_command(state, id, "/auth persistence Encrypted");
    }
    state.notify();
}

/// Starts a device sign-in by hand (the button), even if one was already tried in this run.
pub fn start_login(state: &Shared, id: &str) -> Result<()> {
    state.with_runtime(id, |r| {
        r.auth_requested = true;
        r.auth_opened = false;
        r.auth_url = None;
        r.auth_code = None;
    });
    send_command(state, id, "/auth login device")
}

/// The console command for an action, in the game's syntax (Hytale: leading `/`, Minecraft: none).
pub fn game_command(game: Game, cmd: &str) -> String {
    let bare = cmd.trim().trim_start_matches('/');
    match game {
        Game::Hytale => format!("/{bare}"),
        Game::Minecraft => bare.to_string(),
    }
}

/// Saves the worlds to disk.
pub fn save_command(game: Game) -> &'static str {
    match game {
        Game::Hytale => "/save",
        Game::Minecraft => "save-all flush",
    }
}

/// Sends a console command (Hytale gets a leading `/`, Minecraft's console takes commands without it).
pub fn send_command(state: &Shared, id: &str, command: &str) -> Result<()> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Ok(());
    }
    let game = state.instance(id).map(|i| i.game).unwrap_or_default();
    let cmd = game_command(game, cmd);
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
    let stop = game_command(state.instance(id).map(|i| i.game).unwrap_or_default(), "stop");
    state.system_log(id, format!("Stopping (sending {stop})…"));
    if send_command(state, id, &stop).is_err() {
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

/// Makes a player operator or removes it (Hytale `/op add|remove`, Minecraft `op` / `deop`), and remembers
/// the list.
pub fn set_operator(state: &Shared, id: &str, player: &str, op: bool) -> Result<()> {
    let player = player.trim();
    if player.is_empty() || player.len() > 64 || player.contains(char::is_whitespace) {
        bail!("enter one player name (or UUID)");
    }
    if !state.status(id).is_active() {
        bail!("start the server first: operators are set through its console");
    }
    let game = state.instance(id).map(|i| i.game).unwrap_or_default();
    let cmd = match (game, op) {
        (Game::Hytale, true) => format!("/op add {player}"),
        (Game::Hytale, false) => format!("/op remove {player}"),
        (Game::Minecraft, true) => format!("op {player}"),
        (Game::Minecraft, false) => format!("deop {player}"),
    };
    send_command(state, id, &cmd)?;
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
        assert_eq!(ev, vec![LineEvent::AuthCode("ABCD-1234".into()), LineEvent::AuthUrl("https://accounts.hytale.com/device?user_code=ABCD-1234".into())]);
        assert_eq!(classify(&p, "Enter code: ABCD-1234"), vec![LineEvent::AuthCode("ABCD-1234".into())]);
        assert!(classify(&p, "Server is not authenticated. Run /auth login device").contains(&LineEvent::AuthNeeded));
        assert_eq!(classify(&p, "> Authentication successful! Mode: OAUTH_DEVICE"), vec![LineEvent::AuthOk]);
        assert!(classify(&p, "Hytale Server Booted! [Multiplayer]").contains(&LineEvent::Ready));
        assert!(classify(&p, "Loading assets…").is_empty());
        // Minecraft (vanilla and Forge log layouts)
        assert_eq!(classify(&p, "[12:00:01] [Server thread/INFO]: Steve joined the game"), vec![LineEvent::Joined("Steve".into())]);
        assert_eq!(classify(&p, "[12:00:01] [Server thread/INFO] [minecraft/MinecraftServer]: Steve left the game"), vec![LineEvent::Left("Steve".into())]);
        assert!(classify(&p, "[12:00:01] [Server thread/INFO]: <Steve> Bob joined the game").is_empty());
        assert!(classify(&p, "[12:00:00] [Server thread/INFO]: Done (3.512s)! For help, type \"help\"").contains(&LineEvent::Ready));
        assert_eq!(game_command(Game::Minecraft, "/op Steve"), "op Steve");
        assert_eq!(game_command(Game::Hytale, "op add Steve"), "/op add Steve");
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
