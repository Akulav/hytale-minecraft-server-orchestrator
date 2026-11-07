//! Multi-step operations run as background tasks: 1-click update, version switch, backup / restore,
//! version downloads and imports, Java install.

use anyhow::{Result, anyhow, bail};

use crate::state::{Shared, spawn_task};
use crate::{backup, instance, process, versions};

/// 1-click update: backup → fetch the latest version of the instance's patchline → stop → swap the
/// server files → start again (if it was running).
pub fn update_instance(state: &Shared, id: &str) -> Result<String> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    if state.running_task_for(id).is_some() {
        bail!("another task is running for this instance");
    }
    let id = id.to_string();
    Ok(spawn_task(state, format!("Update {}", inst.name), Some(&id.clone()), move |st, task| async move {
        st.task_update(&task, Some("Checking for a newer server…"), None);
        let latest = versions::download_latest(&st, &task, &inst.patchline).await?;
        if latest == inst.version {
            return Ok(format!("{} is already on the latest {} ({latest})", inst.name, inst.patchline));
        }
        switch_version(&st, &task, &id, &latest, true).await?;
        Ok(format!("{} updated: {} → {latest}", inst.name, inst.version))
    }))
}

/// Puts `version` into the instance (backup first, stop / restart around it).
async fn switch_version(st: &Shared, task: &str, id: &str, version: &str, backup_first: bool) -> Result<()> {
    let was_running = st.status(id).is_active();
    if backup_first {
        st.task_update(task, Some("Backing up world + mods…"), None);
        let file = backup::create(st, id, "before-update").await?;
        st.task_log(task, format!("Backup: {file}"));
    }
    if was_running {
        st.task_update(task, Some("Stopping the server…"), None);
        let _ = process::send_command(st, id, "/say Server is updating, back in a minute!");
        process::stop(st, id).await?;
    }
    st.task_update(task, Some("Installing the new server files…"), None);
    let (s2, i2, v2) = (st.clone(), id.to_string(), version.to_string());
    tokio::task::spawn_blocking(move || instance::install_version_files(&s2, &i2, &v2)).await??;
    st.update_instance(id, |i| i.version = version.to_string())?;
    st.system_log(id, format!("Server version is now {version}"));
    if was_running {
        st.task_update(task, Some("Starting the server…"), None);
        process::start(st, id).await?;
    }
    Ok(())
}

/// Switches an instance to another cached version (up or down).
pub fn change_version(state: &Shared, id: &str, version: &str) -> Result<String> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    if !versions::is_complete(state, version) {
        bail!("version {version} is not in the cache");
    }
    if inst.version == version {
        bail!("{} already uses {version}", inst.name);
    }
    let (id, version) = (id.to_string(), version.to_string());
    Ok(spawn_task(state, format!("{}: switch to {version}", inst.name), Some(&id.clone()), move |st, task| async move {
        switch_version(&st, &task, &id, &version, true).await?;
        Ok(format!("{} now runs {version}", inst.name))
    }))
}

pub fn backup_now(state: &Shared, id: &str, label: String) -> Result<String> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    let id = id.to_string();
    Ok(spawn_task(state, format!("Back up {}", inst.name), Some(&id.clone()), move |st, task| async move {
        st.task_update(&task, Some("Packing world + mods…"), None);
        let file = backup::create(&st, &id, &label).await?;
        Ok(format!("Backup saved: {file}"))
    }))
}

pub fn restore(state: &Shared, id: &str, file: String) -> Result<String> {
    let inst = state.instance(id).ok_or_else(|| anyhow!("instance not found"))?;
    if state.status(id).is_active() {
        bail!("stop the server before restoring");
    }
    let id = id.to_string();
    Ok(spawn_task(state, format!("Restore {}", inst.name), Some(&id.clone()), move |st, task| async move {
        st.task_update(&task, Some("Restoring…"), None);
        backup::restore(&st, &id, &file).await
    }))
}

pub fn download_version(state: &Shared, patchline: &str) -> String {
    let p = patchline.to_string();
    spawn_task(state, format!("Download latest {p} server"), None, move |st, task| async move {
        let v = versions::download_latest(&st, &task, &p).await?;
        Ok(format!("Version {v} is ready"))
    })
}

pub fn import_launcher(state: &Shared, patchline: String, folder: std::path::PathBuf) -> String {
    spawn_task(state, format!("Import {patchline} server from the Hytale launcher"), None, move |st, task| async move {
        let v = versions::install_folder(&st, &task, &folder, None, &patchline, "launcher", false).await?;
        Ok(format!("Version {v} imported"))
    })
}

/// Imports a server from a path on the machine running HyOrchestrator: a .zip or a folder.
pub fn import_path(state: &Shared, path: &str) -> Result<String> {
    let p = std::path::PathBuf::from(path.trim().trim_matches('"'));
    if !p.exists() {
        bail!("{} does not exist", p.display());
    }
    Ok(spawn_task(state, format!("Import {}", p.display()), None, move |st, task| async move {
        let v = if p.is_dir() {
            versions::install_folder(&st, &task, &p, None, "release", "import", false).await?
        } else {
            versions::install_zip(&st, &task, &p, None, "release", "import").await?
        };
        Ok(format!("Version {v} imported"))
    }))
}

/// Signs the Hytale account in once (device code); every server then starts signed in.
pub fn sign_in_account(state: &Shared) -> Result<String> {
    if crate::account::signing_in() {
        bail!("a sign-in is already waiting - approve the code shown on the card");
    }
    Ok(spawn_task(state, "Sign in to Hytale (all servers)", None, crate::account::sign_in))
}

pub fn install_java(state: &Shared) -> String {
    spawn_task(state, "Install Java 25 (Eclipse Temurin)", None, crate::java::install_temurin)
}
