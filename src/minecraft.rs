//! Minecraft (Java Edition) servers: version and loader catalogs, installing vanilla / Fabric / Quilt / Forge /
//! NeoForge into an instance, the launch command, worlds and the files that make up a backup.
//!
//! Layout of a Minecraft instance (`data/instances/<id>/server`, the server's working directory):
//! ```text
//! server.jar                  Mojang's server (vanilla, and the base Fabric / Quilt launch)
//! fabric-server-launch.jar    Fabric  | quilt-server-launch.jar  Quilt
//! libraries/, run.sh, run.bat Forge / NeoForge (we start java with their @…_args.txt file ourselves)
//! server.properties, eula.txt, ops.json, whitelist.json, banned-*.json
//! world/ (or any level-name)  worlds: folders with a level.dat
//! mods/, config/              mods and their settings
//! ```
//! Sources: Mojang's version manifest, meta.fabricmc.net, meta.quiltmc.org + the Quilt installer,
//! Forge's promotions + installer, NeoForge's maven + installer.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::mcprops::Properties;
use crate::state::{InstanceConfig, Loader, Shared, now_ms};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";
const QUILT_INSTALLER_META: &str = "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/maven-metadata.xml";
const FORGE_PROMOS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
pub const EULA_URL: &str = "https://aka.ms/MinecraftEULA";

// ---------------------------------------------------------------------------------------------
// Catalog (what can be installed) - fetched in the background when the UI needs it
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct McVersion {
    pub id: String,
    /// `release`, `snapshot`, `old_beta`, `old_alpha`
    pub kind: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub fetched_at: i64,
    pub loading: bool,
    pub errors: Vec<String>,
    /// newest first
    pub versions: Vec<McVersion>,
    pub fabric_games: Vec<String>,
    pub fabric_loaders: Vec<String>,
    pub quilt_games: Vec<String>,
    pub quilt_loaders: Vec<String>,
    /// game version -> [recommended, latest] (deduplicated)
    pub forge: BTreeMap<String, Vec<String>>,
    /// newest first
    pub neoforge: Vec<String>,
}

impl Catalog {
    pub fn ready(&self) -> bool {
        !self.versions.is_empty()
    }

    /// Does `loader` support game version `mc`? None while unknown (list not loaded).
    pub fn supports(&self, loader: Loader, mc: &str) -> Option<bool> {
        match loader {
            Loader::Vanilla => Some(true),
            Loader::Fabric => (!self.fabric_games.is_empty()).then(|| self.fabric_games.iter().any(|v| v == mc)),
            Loader::Quilt => (!self.quilt_games.is_empty()).then(|| self.quilt_games.iter().any(|v| v == mc)),
            Loader::Forge => (!self.forge.is_empty()).then(|| self.forge.contains_key(mc)),
            Loader::NeoForge => (!self.neoforge.is_empty()).then(|| self.neoforge.iter().any(|v| neoforge_game(v).as_deref() == Some(mc))),
        }
    }

    /// Loader versions to suggest for `mc`, best first.
    pub fn loader_versions(&self, loader: Loader, mc: &str) -> Vec<String> {
        match loader {
            Loader::Vanilla => Vec::new(),
            Loader::Fabric => self.fabric_loaders.iter().take(40).cloned().collect(),
            Loader::Quilt => self.quilt_loaders.iter().take(40).cloned().collect(),
            Loader::Forge => self.forge.get(mc).cloned().unwrap_or_default(),
            Loader::NeoForge => self.neoforge.iter().filter(|v| neoforge_game(v).as_deref() == Some(mc)).take(40).cloned().collect(),
        }
    }

    /// Game versions offered for `loader` (releases, plus snapshots when asked), newest first.
    pub fn game_versions(&self, loader: Loader, snapshots: bool) -> Vec<String> {
        self.versions
            .iter()
            .filter(|v| v.kind == "release" || (snapshots && v.kind == "snapshot"))
            .filter(|v| self.supports(loader, &v.id) != Some(false))
            .map(|v| v.id.clone())
            .collect()
    }
}

/// Starts a background refresh of the catalog when it is missing or old. Cheap to call on every render.
pub fn ensure_catalog(state: &Shared) {
    {
        let mut c = state.mc_catalog.write();
        let age = now_ms() - c.fetched_at;
        let stale = c.fetched_at == 0 || age > 30 * 60_000 || (!c.errors.is_empty() && age > 60_000);
        if c.loading || !stale {
            return;
        }
        c.loading = true;
    }
    let st = state.clone();
    state.rt.spawn(async move {
        let cat = fetch_catalog(&st).await;
        *st.mc_catalog.write() = cat;
        st.notify();
    });
}

async fn get_json(state: &Shared, url: &str) -> Result<Value> {
    let resp = state.http.get(url).send().await.with_context(|| format!("cannot reach {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} answered {}", resp.status());
    }
    resp.json::<Value>().await.with_context(|| format!("{url}: unexpected answer"))
}

async fn get_text(state: &Shared, url: &str) -> Result<String> {
    let resp = state.http.get(url).send().await.with_context(|| format!("cannot reach {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} answered {}", resp.status());
    }
    Ok(resp.text().await?)
}

fn strings(v: &Value, field: &str, only_stable: bool) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter(|e| !only_stable || e["stable"].as_bool().unwrap_or(true))
                .filter_map(|e| e[field].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_catalog(state: &Shared) -> Catalog {
    let urls = [
        format!("{FABRIC_META}/versions/game"),
        format!("{FABRIC_META}/versions/loader"),
        format!("{QUILT_META}/versions/game"),
        format!("{QUILT_META}/versions/loader"),
    ];
    let (manifest, fg, fl, qg, ql, forge, neo) = tokio::join!(
        get_json(state, MANIFEST_URL),
        get_json(state, &urls[0]),
        get_json(state, &urls[1]),
        get_json(state, &urls[2]),
        get_json(state, &urls[3]),
        get_json(state, FORGE_PROMOS),
        get_json(state, NEOFORGE_VERSIONS),
    );
    let mut c = Catalog { fetched_at: now_ms(), ..Default::default() };
    let err = c_err;
    match manifest {
        Ok(m) => {
            c.versions = m["versions"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| Some(McVersion { id: v["id"].as_str()?.to_string(), kind: v["type"].as_str().unwrap_or_default().to_string() })).collect())
                .unwrap_or_default()
        }
        Err(e) => c.errors.push(err("Minecraft versions", e)),
    }
    match fg {
        Ok(v) => c.fabric_games = strings(&v, "version", false),
        Err(e) => c.errors.push(err("Fabric", e)),
    }
    match fl {
        // newest first; Fabric flags only its newest build as stable
        Ok(v) => c.fabric_loaders = strings(&v, "version", false),
        Err(e) => c.errors.push(err("Fabric loaders", e)),
    }
    match qg {
        Ok(v) => c.quilt_games = strings(&v, "version", false),
        Err(e) => c.errors.push(err("Quilt", e)),
    }
    match ql {
        Ok(v) => c.quilt_loaders = sort_stable_first(strings(&v, "version", false)),
        Err(e) => c.errors.push(err("Quilt loaders", e)),
    }
    match forge {
        Ok(v) => c.forge = forge_promos(&v),
        Err(e) => c.errors.push(err("Forge", e)),
    }
    match neo {
        Ok(v) => c.neoforge = sort_neoforge(v["versions"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()),
        Err(e) => c.errors.push(err("NeoForge", e)),
    }
    c
}

fn c_err(what: &str, e: anyhow::Error) -> String {
    format!("{what}: {e:#}")
}

/// Stable versions first, each group newest first (Quilt's lists come in no particular order).
fn sort_stable_first(mut list: Vec<String>) -> Vec<String> {
    list.sort_by(|a, b| is_prerelease(a).cmp(&is_prerelease(b)).then(num_key(b).cmp(&num_key(a))).then(b.cmp(a)));
    list.dedup();
    list
}

fn is_prerelease(v: &str) -> bool {
    let l = v.to_lowercase();
    l.contains("beta") || l.contains("alpha") || l.contains("-pre") || l.contains("-rc") || l.contains("snapshot")
}

/// `{"promos": {"1.20.1-recommended": "47.3.0", "1.20.1-latest": "47.3.12"}}` -> 1.20.1 -> [47.3.0, 47.3.12]
pub fn forge_promos(v: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(p) = v["promos"].as_object() {
        // recommended first
        for suffix in ["-recommended", "-latest"] {
            for (k, ver) in p {
                if let (Some(mc), Some(ver)) = (k.strip_suffix(suffix), ver.as_str()) {
                    let list = out.entry(mc.to_string()).or_default();
                    if !list.iter().any(|x| x == ver) {
                        list.push(ver.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Numeric parts of a version for sorting (`21.1.77-beta` -> [21, 1, 77]).
fn num_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '+']).map_while(|p| p.parse::<u64>().ok()).collect()
}

/// NeoForge versions newest first, stable before beta within the same numbers.
fn sort_neoforge(mut list: Vec<String>) -> Vec<String> {
    list.retain(|v| neoforge_game(v).is_some());
    list.sort_by(|a, b| num_key(b).cmp(&num_key(a)).then(is_prerelease(a).cmp(&is_prerelease(b))));
    list.dedup();
    list
}

/// The Minecraft version a NeoForge version is for: `21.1.77` -> `1.21.1`, `21.0.3` -> `1.21`,
/// `26.1.0.5` -> `26.1`, `26.1.2.3` -> `26.1.2` (year-based versions since 2026).
pub fn neoforge_game(v: &str) -> Option<String> {
    let n = num_key(v);
    match n.as_slice() {
        [major, ..] if *major >= 25 && n.len() >= 4 => Some(if n[2] == 0 { format!("{}.{}", n[0], n[1]) } else { format!("{}.{}.{}", n[0], n[1], n[2]) }),
        [major, minor, _, ..] if (20..25).contains(major) => Some(if *minor == 0 { format!("1.{major}") } else { format!("1.{major}.{minor}") }),
        _ => None,
    }
}

/// `1.20.4` -> (1, 20, 4), `26.1` -> (26, 1, 0); snapshots -> None.
pub fn parse_game(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.');
    let a = parts.next()?.parse().ok()?;
    let b = parts.next()?.parse().ok()?;
    let c = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    parts.next().is_none().then_some((a, b, c))
}

/// World presets are `minecraft:normal` etc. since 1.19; older servers use `default`, `flat`, …
pub fn level_type_value(game: &str, preset: &str) -> String {
    let modern = parse_game(game).is_none_or(|(a, b, _)| a > 1 || b >= 19);
    if modern || !preset.starts_with("minecraft:") {
        return preset.to_string();
    }
    match preset {
        "minecraft:flat" => "flat",
        "minecraft:large_biomes" => "largeBiomes",
        "minecraft:amplified" => "amplified",
        _ => "default",
    }
    .to_string()
}

// ---------------------------------------------------------------------------------------------
// Instance files
// ---------------------------------------------------------------------------------------------

pub fn properties_path(state: &Shared, id: &str) -> PathBuf {
    state.paths.server_dir(id).join("server.properties")
}

pub fn load_properties(state: &Shared, id: &str) -> Properties {
    Properties::load(&properties_path(state, id))
}

/// Settings chosen when the instance is created (written before the first start).
#[derive(Clone, Debug, Default)]
pub struct WorldSetup {
    pub seed: String,
    /// a `LEVEL_TYPES` id
    pub level_type: String,
    pub gamemode: String,
    pub difficulty: String,
    pub max_players: u32,
    pub motd: String,
    pub online_mode: bool,
}

pub fn write_initial_properties(dir: &Path, game: &str, port: u16, w: &WorldSetup) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("server.properties");
    let mut p = Properties::load(&path);
    if p.pairs().is_empty() {
        p = Properties::parse("#Minecraft server properties (written by Hyve; the server adds the rest on its first start)\n");
    }
    p.set("server-port", &port.to_string());
    p.set("online-mode", if w.online_mode { "true" } else { "false" });
    p.set("level-seed", w.seed.trim());
    if !w.level_type.is_empty() {
        p.set("level-type", &level_type_value(game, &w.level_type));
    }
    for (k, v) in [("gamemode", &w.gamemode), ("difficulty", &w.difficulty), ("motd", &w.motd)] {
        if !v.trim().is_empty() {
            p.set(k, v.trim());
        }
    }
    if w.max_players > 0 {
        p.set("max-players", &w.max_players.to_string());
    }
    p.save(&path)
}

/// Keeps `server-port` in server.properties equal to the instance's port.
pub fn sync_port(state: &Shared, inst: &InstanceConfig) -> Result<()> {
    let path = properties_path(state, &inst.id);
    let mut p = Properties::load(&path);
    if p.get("server-port") != Some(inst.port.to_string().as_str()) {
        p.set("server-port", &inst.port.to_string());
        p.save(&path)?;
    }
    Ok(())
}

pub fn eula_accepted(dir: &Path) -> bool {
    Properties::load(&dir.join("eula.txt")).get("eula").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

pub fn accept_eula(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().format("%a %b %d %H:%M:%S %Y");
    std::fs::write(
        dir.join("eula.txt"),
        format!("#By changing the setting below to TRUE you are indicating your agreement to our EULA ({EULA_URL}).\n#{stamp} (accepted in Hyve)\neula=true\n"),
    )?;
    Ok(())
}

/// Files and folders of "world + mods" for backups: every world folder plus mods, configs and player lists.
pub fn data_items(dir: &Path) -> Vec<String> {
    let mut items: Vec<String> = ["mods", "config", "defaultconfigs", "server.properties", "ops.json", "whitelist.json", "banned-players.json", "banned-ips.json", "usercache.json"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    items.extend(world_dirs(dir));
    items
}

/// Folders of the server directory that hold a world (`level.dat`).
fn world_dirs(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.path().join("level.dat").is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct McWorld {
    pub name: String,
    pub size: u64,
    /// the world `level-name` points at
    pub active: bool,
    /// set as active but not generated yet
    pub pending: bool,
}

pub fn list_worlds(state: &Shared, id: &str) -> Vec<McWorld> {
    let dir = state.paths.server_dir(id);
    let active = load_properties(state, id).get("level-name").filter(|s| !s.is_empty()).unwrap_or("world").to_string();
    let mut out: Vec<McWorld> = world_dirs(&dir)
        .into_iter()
        .map(|name| {
            let size = crate::archive::walk_files(&dir.join(&name)).iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum();
            McWorld { active: name == active, name, size, pending: false }
        })
        .collect();
    if !out.iter().any(|w| w.active) {
        out.insert(0, McWorld { name: active, size: 0, active: true, pending: true });
    }
    out
}

/// Makes a new world the active one: generated with `seed` and `level_type` on the next start.
pub fn create_world(state: &Shared, id: &str, name: &str, seed: &str, level_type: &str) -> Result<()> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    let name = crate::instance::validate_world_name(name)?;
    if state.paths.server_dir(id).join(&name).exists() {
        bail!("a world named {name} already exists - use Make active");
    }
    let path = properties_path(state, id);
    let mut p = Properties::load(&path);
    p.set("level-name", &name);
    p.set("level-seed", seed.trim());
    if !level_type.is_empty() {
        p.set("level-type", &level_type_value(&inst.version, level_type));
    }
    p.save(&path)?;
    state.notify();
    Ok(())
}

pub fn set_active_world(state: &Shared, id: &str, name: &str) -> Result<()> {
    let name = crate::instance::validate_world_name(name)?;
    let path = properties_path(state, id);
    let mut p = Properties::load(&path);
    p.set("level-name", &name);
    p.save(&path)?;
    state.notify();
    Ok(())
}

pub fn delete_world(state: &Shared, id: &str, name: &str) -> Result<()> {
    if state.status(id).is_active() {
        bail!("stop the server before deleting a world");
    }
    let name = crate::instance::validate_world_name(name)?;
    let dir = state.paths.server_dir(id).join(&name);
    if dir.join("level.dat").is_file() {
        std::fs::remove_dir_all(&dir)?;
    }
    state.notify();
    Ok(())
}

/// The arguments after `java`: memory, the user's JVM options, the launch target, `nogui`, extra arguments.
pub fn launch_args(inst: &InstanceConfig) -> Vec<String> {
    let mut a = Vec::new();
    if inst.min_memory_mb > 0 {
        a.push(format!("-Xms{}M", inst.min_memory_mb.min(inst.max_memory_mb.max(inst.min_memory_mb))));
    }
    if inst.max_memory_mb > 0 {
        a.push(format!("-Xmx{}M", inst.max_memory_mb));
    }
    a.extend(crate::process::split_args(&inst.jvm_args));
    a.extend(inst.launch.iter().cloned());
    a.push("nogui".into());
    a.extend(crate::process::split_args(&inst.server_args));
    a
}

// ---------------------------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------------------------

/// Mojang's facts about a version: (server jar URL, its SHA-1, the Java major it needs).
async fn version_info(state: &Shared, game: &str) -> Result<(String, String, u32)> {
    let manifest = get_json(state, MANIFEST_URL).await?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|a| a.iter().find(|v| v["id"] == game))
        .and_then(|v| v["url"].as_str())
        .ok_or_else(|| anyhow!("Minecraft {game} is not in Mojang's version list"))?
        .to_string();
    let v = get_json(state, &url).await?;
    let server = &v["downloads"]["server"];
    let jar = server["url"].as_str().ok_or_else(|| anyhow!("Mojang publishes no server for Minecraft {game}"))?.to_string();
    let sha1 = server["sha1"].as_str().unwrap_or_default().to_string();
    let java = v["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    Ok((jar, sha1, java))
}

fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Mojang's server jar for `game`, from the cache or downloaded (checked against its SHA-1).
async fn server_jar(state: &Shared, task: &str, game: &str, url: &str, sha1: &str) -> Result<PathBuf> {
    let jar = state.paths.minecraft.join("vanilla").join(crate::versions::sanitize(game)).join("server.jar");
    let good = |p: PathBuf| {
        let s = sha1.to_string();
        async move { tokio::task::spawn_blocking(move || p.is_file() && (s.is_empty() || sha1_file(&p).ok().as_deref() == Some(s.as_str()))).await.unwrap_or(false) }
    };
    if good(jar.clone()).await {
        state.task_log(task, format!("Minecraft {game} server jar: in the cache"));
        return Ok(jar);
    }
    state.task_log(task, format!("Downloading the Minecraft {game} server from Mojang"));
    crate::download::to_file(state, task, url, &jar).await?;
    if !good(jar.clone()).await {
        let _ = std::fs::remove_file(&jar);
        bail!("the downloaded server jar does not match Mojang's checksum - try again");
    }
    Ok(jar)
}

/// Runs a Java installer (Quilt, Forge, NeoForge) with its output in the task log.
async fn run_java(state: &Shared, task: &str, java: &Path, args: &[String], cwd: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new(java);
    cmd.args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::process::no_window(&mut cmd);
    state.task_log(task, format!("> java {}", args.join(" ")));
    let mut child = tokio::process::Command::from(cmd).kill_on_drop(true).spawn().with_context(|| format!("cannot start {}", java.display()))?;
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        crate::process::job::adopt(pid);
    }
    let pump = |r: Box<dyn tokio::io::AsyncRead + Unpin + Send>| {
        let (st, t) = (state.clone(), task.to_string());
        tokio::spawn(async move { crate::process::read_segments(r, |line| st.task_log(&t, crate::process::strip_ansi(&line))).await })
    };
    let a = pump(Box::new(child.stdout.take().ok_or_else(|| anyhow!("no output"))?));
    let b = pump(Box::new(child.stderr.take().ok_or_else(|| anyhow!("no output"))?));
    let status = child.wait().await?;
    let _ = tokio::join!(a, b);
    if !status.success() {
        bail!("the installer failed ({status}) - see the task log");
    }
    Ok(())
}

/// Removes the files of the previously installed server software (worlds, mods and configs stay).
fn remove_server_software(dir: &Path) {
    for d in ["libraries", ".fabric", ".quilt", "versions"] {
        let _ = std::fs::remove_dir_all(dir.join(d));
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            let software = n == "server.jar"
                || n == "run.bat"
                || n == "run.sh"
                || (n.ends_with(".jar") && ["fabric-server-launch", "quilt-server-launch", "forge-", "neoforge-", "minecraft_server", "minecraftforge"].iter().any(|p| n.starts_with(p)));
            if software && e.path().is_file() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// The java command in Forge / NeoForge's run.sh / run.bat, without the user args file and `"$@"`.
pub fn parse_run_script(text: &str) -> Option<Vec<String>> {
    for line in text.lines() {
        let t = line.trim();
        let lower = t.to_lowercase();
        if t.is_empty() || t.starts_with('#') || t.starts_with("::") || lower.starts_with("rem") || lower.starts_with('@') {
            continue;
        }
        let tokens: Vec<&str> = t.split_whitespace().collect();
        let first = tokens.first()?.trim_matches('"').to_lowercase();
        if !(first == "java" || first.ends_with("/java") || first.ends_with("\\java") || first.ends_with("java.exe")) {
            continue;
        }
        let args: Vec<String> = tokens[1..]
            .iter()
            .map(|s| s.trim_matches('"').to_string())
            .filter(|s| !s.is_empty() && !s.starts_with("@user_jvm_args") && s != "$@" && s != "%*" && s != "nogui")
            .collect();
        if !args.is_empty() {
            return Some(args);
        }
    }
    None
}

/// How to start an installed Forge / NeoForge server (relative to its folder).
pub fn detect_launch(dir: &Path) -> Option<Vec<String>> {
    let (mine, other) = if cfg!(windows) { ("win_args.txt", "unix_args.txt") } else { ("unix_args.txt", "win_args.txt") };
    let exists = |args: &[String]| {
        args.iter().enumerate().all(|(i, a)| {
            if let Some(f) = a.strip_prefix('@') {
                dir.join(f).is_file()
            } else if i > 0 && args[i - 1] == "-jar" {
                dir.join(a).is_file()
            } else {
                true
            }
        })
    };
    let scripts = if cfg!(windows) { ["run.bat", "run.sh"] } else { ["run.sh", "run.bat"] };
    for s in scripts {
        if let Some(args) = std::fs::read_to_string(dir.join(s)).ok().as_deref().and_then(parse_run_script) {
            let args: Vec<String> = args.into_iter().map(|a| if a.starts_with('@') && a.ends_with(other) { format!("{}{mine}", &a[..a.len() - other.len()]) } else { a }).collect();
            if exists(&args) {
                return Some(args);
            }
        }
    }
    // no script: the args file itself, or a runnable jar
    let libs = dir.join("libraries");
    if let Some(f) = crate::archive::walk_files(&libs).into_iter().find(|f| f.file_name().is_some_and(|n| n == mine)) {
        let rel = f.strip_prefix(dir).ok()?.to_string_lossy().replace('\\', "/");
        return Some(vec![format!("@{rel}")]);
    }
    let jar = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).find(|n| {
        let l = n.to_lowercase();
        (l.starts_with("forge-") || l.starts_with("neoforge-")) && l.ends_with(".jar") && !l.contains("installer")
    })?;
    Some(vec!["-jar".into(), jar])
}

/// Picks the loader version: the one asked for, else the best for `game`.
async fn resolve_loader(state: &Shared, loader: Loader, game: &str, wanted: &str) -> Result<String> {
    let wanted = wanted.trim();
    match loader {
        Loader::Vanilla => Ok(String::new()),
        Loader::Fabric => {
            let list = get_json(state, &format!("{FABRIC_META}/versions/loader/{game}")).await?;
            let all: Vec<(String, bool)> = list.as_array().into_iter().flatten().filter_map(|e| Some((e["loader"]["version"].as_str()?.to_string(), e["loader"]["stable"].as_bool().unwrap_or(false)))).collect();
            if all.is_empty() {
                bail!("Fabric does not support Minecraft {game}");
            }
            if !wanted.is_empty() {
                return Ok(wanted.to_string());
            }
            Ok(all.iter().find(|(_, s)| *s).unwrap_or(&all[0]).0.clone())
        }
        Loader::Quilt => {
            let list = get_json(state, &format!("{QUILT_META}/versions/loader/{game}")).await?;
            let all: Vec<String> = list.as_array().into_iter().flatten().filter_map(|e| e["loader"]["version"].as_str().map(str::to_string)).collect();
            if all.is_empty() {
                bail!("Quilt does not support Minecraft {game}");
            }
            if !wanted.is_empty() {
                return Ok(wanted.to_string());
            }
            Ok(sort_stable_first(all)[0].clone())
        }
        Loader::Forge => {
            if !wanted.is_empty() {
                return Ok(wanted.trim_start_matches(&format!("{game}-")).to_string());
            }
            let promos = forge_promos(&get_json(state, FORGE_PROMOS).await?);
            promos.get(game).and_then(|l| l.first().cloned()).ok_or_else(|| anyhow!("Forge has no build for Minecraft {game}"))
        }
        Loader::NeoForge => {
            if !wanted.is_empty() {
                return Ok(wanted.to_string());
            }
            let v = get_json(state, NEOFORGE_VERSIONS).await?;
            let list = sort_neoforge(v["versions"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect());
            list.into_iter().find(|v| neoforge_game(v).as_deref() == Some(game)).ok_or_else(|| anyhow!("NeoForge has no build for Minecraft {game}"))
        }
    }
}

/// The newest build of `loader` for `game` (empty for vanilla).
pub async fn latest_loader(state: &Shared, loader: Loader, game: &str) -> Result<String> {
    resolve_loader(state, loader, game, "").await
}

/// Mojang's latest release (`26.3`).
pub async fn latest_release(state: &Shared) -> Result<String> {
    let m = get_json(state, MANIFEST_URL).await?;
    m["latest"]["release"].as_str().map(str::to_string).ok_or_else(|| anyhow!("Mojang's version list has no latest release"))
}

/// `<release>0.15.1</release>` from a maven-metadata.xml.
fn maven_release(xml: &str) -> Option<String> {
    let tag = |t: &str| xml.split_once(&format!("<{t}>")).and_then(|(_, r)| r.split_once(&format!("</{t}>"))).map(|(v, _)| v.trim().to_string());
    tag("release").or_else(|| tag("latest"))
}

/// Installs (or reinstalls) `game` with `loader` into the instance and records how to start it.
/// The server must be stopped. Worlds, mods and configs are kept. Returns a summary.
pub async fn install(state: &Shared, task: &str, id: &str, game: &str, loader: Loader, wanted: &str) -> Result<String> {
    let mut inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    if state.status(id).is_active() {
        bail!("stop the server first");
    }
    state.task_update(task, Some(&format!("Looking up Minecraft {game}…")), None);
    let (jar_url, sha1, java_major) = version_info(state, game).await?;
    let loader_version = resolve_loader(state, loader, game, wanted).await?;
    let what = if loader == Loader::Vanilla { format!("Minecraft {game}") } else { format!("{} {loader_version} for Minecraft {game}", loader.label()) };
    state.task_log(task, format!("Installing {what} (needs Java {java_major})"));

    // the Java this server will run on (installers run on it too)
    inst.java_major = java_major;
    inst.loader = loader;
    let java = crate::java::ensure_for_minecraft(state, task, &inst).await?;
    state.task_log(task, format!("Java: {}", java.display()));

    let dir = state.paths.server_dir(id);
    std::fs::create_dir_all(dir.join("mods"))?;
    remove_server_software(&dir);
    let tmp = state.paths.tmp.join(format!("mc-{}", crate::state::new_id()));
    std::fs::create_dir_all(&tmp)?;
    let result: Result<Vec<String>> = async {
        if matches!(loader, Loader::Vanilla | Loader::Fabric | Loader::Quilt) {
            let jar = server_jar(state, task, game, &jar_url, &sha1).await?;
            std::fs::copy(&jar, dir.join("server.jar")).context("cannot copy server.jar")?;
        }
        state.task_update(task, Some(&format!("Installing {what}…")), None);
        match loader {
            Loader::Vanilla => Ok(vec!["-jar".into(), "server.jar".into()]),
            Loader::Fabric => {
                let installers = get_json(state, &format!("{FABRIC_META}/versions/installer")).await?;
                let installer = strings(&installers, "version", true).into_iter().next().ok_or_else(|| anyhow!("no Fabric installer version"))?;
                let url = format!("{FABRIC_META}/versions/loader/{game}/{loader_version}/{installer}/server/jar");
                crate::download::to_file(state, task, &url, &dir.join("fabric-server-launch.jar")).await?;
                Ok(vec!["-jar".into(), "fabric-server-launch.jar".into()])
            }
            Loader::Quilt => {
                let iv = maven_release(&get_text(state, QUILT_INSTALLER_META).await?).ok_or_else(|| anyhow!("no Quilt installer version"))?;
                let installer = tmp.join("quilt-installer.jar");
                crate::download::to_file(state, task, &format!("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/{iv}/quilt-installer-{iv}.jar"), &installer).await?;
                let args = vec!["-jar".to_string(), installer.display().to_string(), "install".into(), "server".into(), game.to_string(), loader_version.clone(), format!("--install-dir={}", dir.display())];
                run_java(state, task, &java, &args, &tmp).await?;
                if !dir.join("quilt-server-launch.jar").is_file() {
                    bail!("the Quilt installer did not create quilt-server-launch.jar");
                }
                Ok(vec!["-jar".into(), "quilt-server-launch.jar".into()])
            }
            Loader::Forge | Loader::NeoForge => {
                let url = if loader == Loader::Forge {
                    let full = format!("{game}-{loader_version}");
                    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar")
                } else {
                    format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar")
                };
                let installer = tmp.join("installer.jar");
                crate::download::to_file(state, task, &url, &installer).await.with_context(|| format!("{} {loader_version} could not be downloaded - is the version right?", loader.label()))?;
                state.task_update(task, Some(&format!("Running the {} installer (downloads libraries, takes a minute)…", loader.label())), None);
                let args = vec!["-jar".to_string(), installer.display().to_string(), "--installServer".into(), dir.display().to_string()];
                run_java(state, task, &java, &args, &dir).await?;
                detect_launch(&dir).ok_or_else(|| anyhow!("the {} installer finished but its start files were not found", loader.label()))
            }
        }
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    let launch = result?;
    state.task_log(task, format!("Start command: java … {} nogui", launch.join(" ")));
    let (g, lv) = (game.to_string(), loader_version.clone());
    state.update_instance(id, |i| {
        i.version = g;
        i.loader = loader;
        i.loader_version = lv;
        i.java_major = java_major;
        i.launch = launch;
    })?;
    state.system_log(id, format!("Installed {what}"));
    Ok(format!("{what} is installed"))
}

/// Cached Mojang server jars: (game version, size).
pub fn cached_jars(state: &Shared) -> Vec<(String, u64)> {
    let dir = state.paths.minecraft.join("vanilla");
    let mut out: Vec<(String, u64)> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter_map(|e| Some((e.file_name().to_string_lossy().to_string(), e.path().join("server.jar").metadata().ok()?.len()))).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

pub fn delete_cached_jar(state: &Shared, game: &str) -> Result<()> {
    let dir = state.paths.minecraft.join("vanilla").join(crate::versions::sanitize(game));
    if dir.starts_with(state.paths.minecraft.join("vanilla")) && dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    state.notify();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_neoforge_to_minecraft() {
        assert_eq!(neoforge_game("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_game("21.0.3-beta").as_deref(), Some("1.21"));
        assert_eq!(neoforge_game("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_game("26.3.0.101").as_deref(), Some("26.3"));
        assert_eq!(neoforge_game("26.1.2.7-beta").as_deref(), Some("26.1.2"));
        assert_eq!(neoforge_game("0.25w14craftmine.3-beta"), None);
        let sorted = sort_neoforge(vec!["21.1.9".into(), "21.1.77".into(), "21.1.77-beta".into(), "20.2.3-beta".into(), "0.25w14craftmine.3-beta".into()]);
        assert_eq!(sorted, vec!["21.1.77", "21.1.77-beta", "21.1.9", "20.2.3-beta"]);
        // Quilt's per-version lists are unordered
        let q = sort_stable_first(vec!["0.20.0-beta.9".into(), "0.24.0".into(), "0.30.2".into(), "0.31.0-beta.6".into(), "0.29.10".into()]);
        assert_eq!(q, vec!["0.30.2", "0.29.10", "0.24.0", "0.31.0-beta.6", "0.20.0-beta.9"]);
    }

    #[test]
    fn reads_forge_promotions() {
        let v: Value = serde_json::json!({"promos": {"1.20.1-latest": "47.3.12", "1.20.1-recommended": "47.3.0", "26.1-latest": "62.0.9", "1.21.11-latest": "61.2.5", "1.21.11-recommended": "61.2.5"}});
        let p = forge_promos(&v);
        assert_eq!(p["1.20.1"], vec!["47.3.0", "47.3.12"]);
        assert_eq!(p["26.1"], vec!["62.0.9"]);
        assert_eq!(p["1.21.11"], vec!["61.2.5"]);
    }

    #[test]
    fn game_versions_and_level_types() {
        assert_eq!(parse_game("1.20.4"), Some((1, 20, 4)));
        assert_eq!(parse_game("26.3"), Some((26, 3, 0)));
        assert_eq!(parse_game("26.4-snapshot-7"), None);
        assert_eq!(level_type_value("1.18.2", "minecraft:flat"), "flat");
        assert_eq!(level_type_value("1.16.5", "minecraft:large_biomes"), "largeBiomes");
        assert_eq!(level_type_value("1.21.4", "minecraft:flat"), "minecraft:flat");
        assert_eq!(level_type_value("26.3", "minecraft:amplified"), "minecraft:amplified");
        assert_eq!(maven_release("<metadata><versioning><latest>0.15.1</latest><release>0.15.0</release></versioning></metadata>").as_deref(), Some("0.15.0"));
    }

    #[test]
    fn reads_forge_run_scripts() {
        let sh = "#!/usr/bin/env sh\n# Forge requires a configured set of both JVM and program arguments.\njava @user_jvm_args.txt @libraries/net/minecraftforge/forge/1.20.1-47.3.0/unix_args.txt \"$@\"\n";
        assert_eq!(parse_run_script(sh), Some(vec!["@libraries/net/minecraftforge/forge/1.20.1-47.3.0/unix_args.txt".to_string()]));
        let bat = "@echo off\nREM Forge requires a configured set of both JVM and program arguments.\njava @user_jvm_args.txt @libraries/net/neoforged/neoforge/21.1.77/win_args.txt %*\npause\n";
        assert_eq!(parse_run_script(bat), Some(vec!["@libraries/net/neoforged/neoforge/21.1.77/win_args.txt".to_string()]));
        let shim = "java @user_jvm_args.txt -jar forge-1.20.4-49.0.31-shim.jar \"$@\"";
        assert_eq!(parse_run_script(shim), Some(vec!["-jar".to_string(), "forge-1.20.4-49.0.31-shim.jar".to_string()]));
        assert_eq!(parse_run_script("echo nothing"), None);
    }

    #[test]
    fn detects_launch_files_for_this_system() {
        let dir = std::env::temp_dir().join(format!("hyo-mc-{}", crate::state::new_id()));
        let lib = dir.join("libraries/net/neoforged/neoforge/21.1.77");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("win_args.txt"), b"x").unwrap();
        std::fs::write(lib.join("unix_args.txt"), b"x").unwrap();
        // the script names the other system's file: we use ours
        std::fs::write(dir.join("run.sh"), "java @user_jvm_args.txt @libraries/net/neoforged/neoforge/21.1.77/unix_args.txt \"$@\"\n").unwrap();
        std::fs::write(dir.join("run.bat"), "java @user_jvm_args.txt @libraries/net/neoforged/neoforge/21.1.77/win_args.txt %*\n").unwrap();
        let mine = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        assert_eq!(detect_launch(&dir), Some(vec![format!("@libraries/net/neoforged/neoforge/21.1.77/{mine}")]));
        // removing the software keeps worlds and mods
        std::fs::create_dir_all(dir.join("world")).unwrap();
        std::fs::write(dir.join("world/level.dat"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("server.jar"), b"x").unwrap();
        std::fs::write(dir.join("forge-1.20.4-49.0.31-shim.jar"), b"x").unwrap();
        remove_server_software(&dir);
        assert!(!dir.join("libraries").exists() && !dir.join("server.jar").exists() && !dir.join("run.sh").exists());
        assert!(!dir.join("forge-1.20.4-49.0.31-shim.jar").exists());
        assert!(dir.join("world/level.dat").is_file() && dir.join("mods").is_dir());
        let items = data_items(&dir);
        assert!(items.contains(&"world".to_string()) && items.contains(&"server.properties".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_first_properties_and_eula() {
        let dir = std::env::temp_dir().join(format!("hyo-mcp-{}", crate::state::new_id()));
        let w = WorldSetup { seed: "hello".into(), level_type: "minecraft:flat".into(), gamemode: "creative".into(), difficulty: "hard".into(), max_players: 8, motd: "Hi: there".into(), online_mode: false };
        write_initial_properties(&dir, "1.16.5", 25570, &w).unwrap();
        let p = Properties::load(&dir.join("server.properties"));
        assert_eq!(p.get("server-port"), Some("25570"));
        assert_eq!(p.get("online-mode"), Some("false"));
        assert_eq!(p.get("level-type"), Some("flat"));
        assert_eq!(p.get("motd"), Some("Hi: there"));
        assert!(!eula_accepted(&dir));
        accept_eula(&dir).unwrap();
        assert!(eula_accepted(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }
}
