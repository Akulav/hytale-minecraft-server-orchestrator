//! Server instances: creation, settings, worlds (with seeds), mods and the version files.
//!
//! Layout of one instance (`data/instances/<id>/server`, the server's working directory, like the
//! official `Server/` folder):
//! ```text
//! HytaleServer.jar, HytaleServer.aot, Licenses/   copied from versions/<version>/Server
//! config.json, permissions.json, whitelist.json, bans.json   written by the server
//! universe/worlds/<name>/config.json               one folder per world ("Seed" sets world generation)
//! mods/                                            .jar / .zip mods (".disabled" suffix = off)
//! ```
//! `--assets` points at the shared `versions/<version>/Assets.zip`.

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::Serialize;
use std::path::PathBuf;

use crate::state::{DEFAULT_GAME_PORT, InstanceConfig, Shared, now_ms};
use crate::versions;

/// Files and folders that make up "world + mods" (backups, restores).
pub const DATA_ITEMS: [&str; 6] = ["universe", "mods", "config.json", "permissions.json", "whitelist.json", "bans.json"];
/// Files of the server version that are replaced on a version change; everything else is the instance's.
const PRESERVE: [&str; 9] = ["config.json", "permissions.json", "whitelist.json", "bans.json", "universe", "mods", "logs", "backups", "auth.enc"];

#[derive(Clone, Debug, Default)]
pub struct NewInstance {
    pub name: String,
    pub version: String,
    pub patchline: String,
    pub port: u16,
    pub max_memory_mb: u32,
    pub offline_auth: bool,
    /// world seed for the "default" world; None = random (chosen by the server)
    pub seed: Option<i64>,
}

pub fn validate_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        bail!("give the instance a name");
    }
    if n.chars().count() > 48 {
        bail!("the name is too long (48 characters at most)");
    }
    Ok(n.to_string())
}

/// World folder names: letters, digits, `-` and `_`.
pub fn validate_world_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 32 || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("world names use letters, digits, - and _ (32 at most)");
    }
    Ok(n.to_string())
}

/// Accepts a number, or any text (hashed to a number, like typing a word as a seed).
pub fn parse_seed(text: &str) -> Option<i64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some(n);
    }
    // Java's String.hashCode, so a word gives the same seed every time
    let mut h: i32 = 0;
    for c in t.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as i32);
    }
    Some(h as i64)
}

pub fn random_seed() -> i64 {
    let b = uuid::Uuid::new_v4().into_bytes();
    i64::from_le_bytes(b[..8].try_into().unwrap()).abs() % 10_000_000_000_000
}

/// First free game port from 5520 that no other instance uses.
pub fn suggest_port(state: &Shared) -> u16 {
    let used: Vec<u16> = state.instances.read().iter().map(|i| i.port).collect();
    (DEFAULT_GAME_PORT..DEFAULT_GAME_PORT + 200).find(|p| !used.contains(p)).unwrap_or(DEFAULT_GAME_PORT)
}

pub fn port_conflict(state: &Shared, id: &str, port: u16) -> Option<String> {
    state.instances.read().iter().find(|i| i.id != id && i.port == port).map(|i| i.name.clone())
}

pub fn create(state: &Shared, req: NewInstance) -> Result<InstanceConfig> {
    let name = validate_name(&req.name)?;
    if req.version.is_empty() || !versions::is_complete(state, &req.version) {
        bail!("pick a server version (download one in Versions first)");
    }
    if req.port == 0 {
        bail!("pick a port between 1 and 65535");
    }
    if let Some(other) = port_conflict(state, "", req.port) {
        bail!("port {} is already used by \"{other}\"", req.port);
    }
    let inst = InstanceConfig {
        id: crate::state::new_id(),
        name,
        version: req.version.clone(),
        patchline: if req.patchline.is_empty() { "release".into() } else { req.patchline },
        port: req.port,
        max_memory_mb: req.max_memory_mb.max(1024),
        min_memory_mb: 1024,
        offline_auth: req.offline_auth,
        created_at: now_ms(),
        ..Default::default()
    };
    let dir = state.paths.server_dir(&inst.id);
    std::fs::create_dir_all(dir.join("mods"))?;
    std::fs::create_dir_all(dir.join("universe").join("worlds"))?;
    install_version_files(state, &inst.id, &req.version)?;
    if let Some(seed) = req.seed {
        create_world(state, &inst.id, "default", Some(seed))?;
    }
    state.instances.write().push(inst.clone());
    state.save_instances()?;
    state.notify();
    Ok(inst)
}

/// Copies `versions/<version>/Server/*` into the instance (jar, AOT cache, licenses), keeping the
/// instance's own config, worlds and mods.
pub fn install_version_files(state: &Shared, id: &str, version: &str) -> Result<()> {
    let src = versions::server_files(state, version);
    if !src.join("HytaleServer.jar").is_file() {
        bail!("version {version} is not in the cache");
    }
    let dst = state.paths.server_dir(id);
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(&src)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if PRESERVE.contains(&name.as_str()) {
            continue;
        }
        let target = dst.join(&name);
        if entry.path().is_dir() {
            if target.exists() {
                std::fs::remove_dir_all(&target)?;
            }
            crate::archive::copy_tree(&entry.path(), &target, |_| {})?;
        } else {
            std::fs::copy(entry.path(), &target).with_context(|| format!("cannot copy {name}"))?;
        }
    }
    // an AOT cache from another version must not stay behind
    if !src.join("HytaleServer.aot").exists() {
        let _ = std::fs::remove_file(dst.join("HytaleServer.aot"));
    }
    Ok(())
}

pub fn delete(state: &Shared, id: &str, delete_backups: bool) -> Result<()> {
    if state.status(id).is_active() {
        bail!("stop the server first");
    }
    let dir = state.paths.instance_dir(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("cannot delete {}", dir.display()))?;
    }
    if delete_backups {
        let _ = std::fs::remove_dir_all(state.paths.backup_dir(id));
    }
    state.instances.write().retain(|i| i.id != id);
    state.runtimes.write().remove(id);
    state.save_instances()?;
    state.notify();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Worlds
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WorldInfo {
    pub name: String,
    pub seed: Option<i64>,
    pub pvp: Option<bool>,
    pub size: u64,
    /// created by us but not generated by the server yet
    pub pending: bool,
}

fn worlds_dir(state: &Shared, id: &str) -> PathBuf {
    state.paths.server_dir(id).join("universe").join("worlds")
}

pub fn list_worlds(state: &Shared, id: &str) -> Vec<WorldInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(worlds_dir(state, id)) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let cfg: serde_json::Value = std::fs::read_to_string(e.path().join("config.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            let files = crate::archive::walk_files(&e.path());
            out.push(WorldInfo {
                name: e.file_name().to_string_lossy().to_string(),
                seed: cfg["Seed"].as_i64(),
                pvp: cfg["IsPvpEnabled"].as_bool(),
                size: files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum(),
                pending: files.len() <= 1,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A world `config.json` like the one the server writes (Hytale Server Manual, "Universe structure"),
/// with our seed and a fresh UUID. The server fills in anything newer on its next start.
pub fn world_config(seed: i64) -> serde_json::Value {
    let uuid = base64::engine::general_purpose::STANDARD.encode(uuid::Uuid::new_v4().as_bytes());
    serde_json::json!({
        "Version": 4,
        "UUID": { "$binary": uuid, "$type": "04" },
        "Seed": seed,
        "WorldGen": { "Type": "Hytale", "Name": "Default" },
        "WorldMap": { "Type": "WorldGen" },
        "ChunkStorage": { "Type": "Hytale" },
        "ChunkConfig": {},
        "IsTicking": true,
        "IsBlockTicking": true,
        "IsPvpEnabled": false,
        "IsFallDamageEnabled": true,
        "IsGameTimePaused": false,
        "RequiredPlugins": {},
        "IsSpawningNPC": true,
        "IsSpawnMarkersEnabled": true,
        "IsAllNPCFrozen": false,
        "GameplayConfig": "Default",
        "IsCompassUpdating": true,
        "IsSavingPlayers": true,
        "IsSavingChunks": true,
        "IsUnloadingChunks": true,
        "IsObjectiveMarkersEnabled": true,
        "DeleteOnUniverseStart": false,
        "DeleteOnRemove": false,
        "ResourceStorage": { "Type": "Hytale" },
        "Plugin": {}
    })
}

/// Creates `universe/worlds/<name>/config.json` with the seed (random when None). The server generates
/// the world on its next start.
pub fn create_world(state: &Shared, id: &str, name: &str, seed: Option<i64>) -> Result<i64> {
    let name = validate_world_name(name)?;
    let dir = worlds_dir(state, id).join(&name);
    if dir.exists() {
        bail!("a world named {name} already exists");
    }
    std::fs::create_dir_all(&dir)?;
    let seed = seed.unwrap_or_else(random_seed);
    std::fs::write(dir.join("config.json"), serde_json::to_vec_pretty(&world_config(seed))?)?;
    state.notify();
    Ok(seed)
}

pub fn delete_world(state: &Shared, id: &str, name: &str) -> Result<()> {
    if state.status(id).is_active() {
        bail!("stop the server before deleting a world");
    }
    let name = validate_world_name(name)?;
    let dir = worlds_dir(state, id).join(name);
    if dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    state.notify();
    Ok(())
}

/// Turns PvP on or off in a world's config.json (only while the server is stopped).
pub fn set_world_pvp(state: &Shared, id: &str, name: &str, pvp: bool) -> Result<()> {
    if state.status(id).is_active() {
        bail!("stop the server first: it rewrites config files while running");
    }
    let name = validate_world_name(name)?;
    let file = worlds_dir(state, id).join(name).join("config.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
    v["IsPvpEnabled"] = serde_json::Value::Bool(pvp);
    std::fs::write(&file, serde_json::to_vec_pretty(&v)?)?;
    state.notify();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModInfo {
    pub file: String,
    pub name: String,
    pub size: u64,
    pub enabled: bool,
}

fn mods_dir(state: &Shared, id: &str) -> PathBuf {
    state.paths.server_dir(id).join("mods")
}

pub fn list_mods(state: &Shared, id: &str) -> Vec<ModInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods_dir(state, id)) {
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            let enabled = !file.ends_with(".disabled");
            let name = file.trim_end_matches(".disabled").to_string();
            let lower = name.to_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".zip")) || !e.path().is_file() {
                continue;
            }
            out.push(ModInfo { size: e.metadata().map(|m| m.len()).unwrap_or(0), file, name, enabled });
        }
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

/// The last component of a path written with `/` or `\` - on every OS (a browser on Windows may send
/// `C:\x\mod.jar` to a Linux server, where `\` is not a separator for `Path`).
pub fn base_name(path: &str) -> &str {
    path.trim().rsplit(['/', '\\']).next().unwrap_or_default()
}

/// A safe file name for an uploaded mod (no folders, only .jar / .zip).
pub fn sanitize_mod_name(name: &str) -> Result<String> {
    let base = base_name(name);
    let clean: String = base.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).collect();
    let lower = clean.to_lowercase();
    if clean.is_empty() || clean.starts_with('.') {
        bail!("invalid file name");
    }
    if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
        bail!("{clean}: mods are .jar or .zip files");
    }
    Ok(clean)
}

pub fn add_mod(state: &Shared, id: &str, name: &str, bytes: &[u8]) -> Result<String> {
    state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    let name = sanitize_mod_name(name)?;
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        bail!("{name} is not a valid .jar / .zip archive");
    }
    let dir = mods_dir(state, id);
    std::fs::create_dir_all(&dir)?;
    let _ = std::fs::remove_file(dir.join(format!("{name}.disabled")));
    std::fs::write(dir.join(&name), bytes)?;
    state.notify();
    Ok(name)
}

pub fn toggle_mod(state: &Shared, id: &str, file: &str) -> Result<()> {
    let dir = mods_dir(state, id);
    let file = base_name(file);
    if file.is_empty() || file.starts_with('.') {
        bail!("invalid mod");
    }
    let from = dir.join(file);
    let to = match file.strip_suffix(".disabled") {
        Some(base) => dir.join(base),
        None => dir.join(format!("{file}.disabled")),
    };
    std::fs::rename(&from, &to).with_context(|| format!("cannot rename {file}"))?;
    state.notify();
    Ok(())
}

pub fn delete_mod(state: &Shared, id: &str, file: &str) -> Result<()> {
    let file = base_name(file);
    if file.is_empty() || file.starts_with('.') {
        bail!("invalid mod");
    }
    std::fs::remove_file(mods_dir(state, id).join(file))?;
    state.notify();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_from_numbers_and_words() {
        assert_eq!(parse_seed("12345"), Some(12345));
        assert_eq!(parse_seed("  -7 "), Some(-7));
        assert_eq!(parse_seed(""), None);
        // Java: "hello".hashCode() == 99162322
        assert_eq!(parse_seed("hello"), Some(99162322));
        assert!(random_seed() >= 0);
    }

    #[test]
    fn mod_names_are_sanitized() {
        assert_eq!(sanitize_mod_name("cool-mod-1.2.jar").unwrap(), "cool-mod-1.2.jar");
        assert_eq!(sanitize_mod_name("C:\\x\\..\\pack.ZIP").unwrap(), "pack.ZIP");
        assert_eq!(sanitize_mod_name("../../evil.jar").unwrap(), "evil.jar");
        assert_eq!(sanitize_mod_name("..\\..\\evil.jar").unwrap(), "evil.jar");
        assert_eq!(base_name("a/b\\c.zip"), "c.zip");
        assert_eq!(base_name(".."), "..");
        assert!(sanitize_mod_name("readme.txt").is_err());
        assert!(sanitize_mod_name(".jar").is_err());
    }

    #[test]
    fn world_names_and_config() {
        assert!(validate_world_name("survival_2").is_ok());
        assert!(validate_world_name("../x").is_err());
        assert!(validate_world_name("").is_err());
        let cfg = world_config(42);
        assert_eq!(cfg["Seed"], 42);
        assert_eq!(cfg["UUID"]["$type"], "04");
        let raw = base64::engine::general_purpose::STANDARD.decode(cfg["UUID"]["$binary"].as_str().unwrap()).unwrap();
        assert_eq!(raw.len(), 16);
    }
}
