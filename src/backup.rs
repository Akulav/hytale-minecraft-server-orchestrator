//! Backups of an instance's "world + mods" (Hytale: universe/, mods/ and the server's JSON configs;
//! Minecraft: world folders, mods/, config/, server.properties and player lists) as zip files in
//! `data/backups/<instance>/`, and restoring them.

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::path::PathBuf;

use crate::instance::data_items;
use crate::state::Shared;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BackupInfo {
    pub file: String,
    pub label: String,
    pub size: u64,
    pub created: i64,
}

/// `20260107-153012_before-update.zip` -> "before update"
fn label_of(file: &str) -> String {
    file.trim_end_matches(".zip").split_once('_').map(|(_, l)| l.replace('-', " ")).unwrap_or_default()
}

fn safe_label(label: &str) -> String {
    let s: String = label.trim().chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    s.chars().take(40).collect()
}

pub fn list(state: &Shared, id: &str) -> Vec<BackupInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(state.paths.backup_dir(id)) {
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            if !file.ends_with(".zip") {
                continue;
            }
            let meta = e.metadata().ok();
            let created = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(BackupInfo { label: label_of(&file), size: meta.map(|m| m.len()).unwrap_or(0), created, file });
        }
    }
    out.sort_by(|a, b| b.file.cmp(&a.file));
    out
}

fn backup_path(state: &Shared, id: &str, file: &str) -> Result<PathBuf> {
    state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    let name = crate::instance::base_name(file);
    if !name.ends_with(".zip") || name.starts_with('.') {
        bail!("invalid backup");
    }
    let p = state.paths.backup_dir(id).join(name);
    if !p.is_file() {
        bail!("backup {file} not found");
    }
    Ok(p)
}

/// Zips the instance's world + mods. Works while the server runs (it may be writing: for a
/// guaranteed-consistent copy, back up while stopped). Returns the file name.
pub async fn create(state: &Shared, id: &str, label: &str) -> Result<String> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let label = safe_label(label);
    let file = if label.is_empty() { format!("{stamp}.zip") } else { format!("{stamp}_{label}.zip") };
    let zip = state.paths.backup_dir(id).join(&file);
    if state.status(id).is_active() {
        // ask the server to flush chunks first (harmless if the command doesn't exist)
        let _ = crate::process::send_command(state, id, crate::process::save_command(inst.game));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    let base = state.paths.server_dir(id);
    let z2 = zip.clone();
    let items = data_items(state, id);
    let count = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = items.iter().map(String::as_str).collect();
        crate::archive::zip_paths(&base, &refs, &z2)
    })
    .await??;
    state.notify();
    crate::logln(&format!("[{}] backup {file}: {count} files", inst.name));
    Ok(file)
}

/// Replaces the instance's world + mods with a backup. The server must be stopped; a safety backup of
/// the current state is made first.
pub async fn restore(state: &Shared, id: &str, file: &str) -> Result<String> {
    if state.status(id).is_active() {
        bail!("stop the server before restoring");
    }
    let zip = backup_path(state, id, file)?;
    let safety = create(state, id, "before-restore").await.context("the safety backup failed, nothing was restored")?;
    let base = state.paths.server_dir(id);
    let items = data_items(state, id);
    tokio::task::spawn_blocking(move || -> Result<()> {
        for item in items {
            let p = base.join(&item);
            if p.is_dir() {
                std::fs::remove_dir_all(&p)?;
            } else if p.is_file() {
                std::fs::remove_file(&p)?;
            }
        }
        crate::archive::extract_zip(&zip, &base, |_| {})?;
        std::fs::create_dir_all(base.join("mods"))?;
        Ok(())
    })
    .await??;
    state.notify();
    Ok(format!("Restored {file} (the previous state is saved as {safety})"))
}

pub fn delete(state: &Shared, id: &str, file: &str) -> Result<()> {
    std::fs::remove_file(backup_path(state, id, file)?)?;
    state.notify();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_roundtrip_through_file_names() {
        assert_eq!(safe_label("Before update!"), "before-update");
        assert_eq!(safe_label("  "), "");
        assert_eq!(label_of("20260107-153012_before-update.zip"), "before update");
        assert_eq!(label_of("20260107-153012.zip"), "");
    }
}
