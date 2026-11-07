//! Java 25 for the Hytale server: finds a usable `java` (the setting, the managed copy in
//! `data/java`, JAVA_HOME, PATH) and can install Eclipse Temurin 25 from the Adoptium API.

use anyhow::{Context, Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::state::{JavaInfo, Shared};

pub const REQUIRED_MAJOR: u32 = 25;

fn exe_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Candidate `java` executables, best first.
fn candidates(state: &Shared) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let configured = state.settings.read().java_path.trim().to_string();
    if !configured.is_empty() {
        out.push(PathBuf::from(configured));
        return out; // an explicit choice is not second-guessed
    }
    if let Some(managed) = find_java_in(&state.paths.java) {
        out.push(managed);
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        out.push(Path::new(&home).join("bin").join(exe_name()));
    }
    out.push(PathBuf::from(exe_name()));
    out
}

/// Finds `bin/java` below `dir` (a Temurin archive extracts to `jdk-25…/bin` or `…/Contents/Home/bin`).
pub fn find_java_in(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let candidate = d.join("bin").join(exe_name());
        if candidate.is_file() {
            return Some(candidate);
        }
        if depth < 4
            && let Ok(rd) = std::fs::read_dir(&d)
        {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    stack.push((e.path(), depth + 1));
                }
            }
        }
    }
    None
}

/// `openjdk version "25.0.1" 2025-10-21 LTS` -> 25 ; `java version "1.8.0_391"` -> 8
pub fn parse_major(version_output: &str) -> Option<u32> {
    let re = regex::Regex::new(r#"version "([0-9]+)(?:\.([0-9]+))?"#).ok()?;
    let c = re.captures(version_output)?;
    let first: u32 = c.get(1)?.as_str().parse().ok()?;
    if first == 1 { c.get(2).and_then(|m| m.as_str().parse().ok()) } else { Some(first) }
}

async fn probe(path: &Path) -> Result<(String, u32)> {
    let mut cmd = std::process::Command::new(path);
    cmd.arg("-version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::process::no_window(&mut cmd);
    let out = tokio::process::Command::from(cmd).output().await.with_context(|| format!("cannot run {}", path.display()))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    let first = text.lines().next().unwrap_or_default().trim().to_string();
    let major = parse_major(&text).ok_or_else(|| anyhow!("unexpected `java -version` output: {first}"))?;
    Ok((first, major))
}

/// Re-detects Java and stores the result in the state (shown in Settings and before a start).
pub async fn detect(state: &Shared) -> JavaInfo {
    let mut info = JavaInfo::default();
    let mut errors = Vec::new();
    for c in candidates(state) {
        match probe(&c).await {
            Ok((version, major)) if major >= REQUIRED_MAJOR => {
                info = JavaInfo { path: c.display().to_string(), version, major, error: String::new() };
                break;
            }
            Ok((version, major)) => {
                errors.push(format!("{} is Java {major} ({version})", c.display()));
                if info.path.is_empty() {
                    info = JavaInfo { path: c.display().to_string(), version, major, error: String::new() };
                }
            }
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if info.major < REQUIRED_MAJOR {
        info.error = if errors.is_empty() {
            format!("Java {REQUIRED_MAJOR} was not found")
        } else {
            format!("Java {REQUIRED_MAJOR} or newer is required. {}", errors.join("; "))
        };
    }
    *state.java.write() = info.clone();
    state.notify();
    info
}

/// The `java` to start servers with, or a readable error.
pub async fn require(state: &Shared) -> Result<PathBuf> {
    let info = detect(state).await;
    if info.major >= REQUIRED_MAJOR && !info.path.is_empty() {
        Ok(PathBuf::from(info.path))
    } else {
        bail!("{} - install it in Settings → Java (one click) or set its path", info.error)
    }
}

fn adoptium_os_arch() -> Result<(&'static str, &'static str, &'static str)> {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        bail!("no Temurin build for this system")
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        bail!("no Temurin build for this processor")
    };
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    Ok((os, arch, ext))
}

/// Downloads Eclipse Temurin 25 (JRE) into `data/java` and selects it.
pub async fn install_temurin(state: Shared, task: String) -> Result<String> {
    let (os, arch, ext) = adoptium_os_arch()?;
    let url = format!("https://api.adoptium.net/v3/binary/latest/{REQUIRED_MAJOR}/ga/{os}/{arch}/jre/hotspot/normal/eclipse");
    state.task_log(&task, format!("Downloading Temurin {REQUIRED_MAJOR} JRE from {url}"));
    let archive = state.paths.tmp.join(format!("temurin-{REQUIRED_MAJOR}.{ext}"));
    crate::download::to_file(&state, &task, &url, &archive).await?;
    state.task_update(&task, Some("Extracting…"), None);
    let target = state.paths.java.clone();
    let a2 = archive.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::create_dir_all(&target)?;
        if a2.extension().is_some_and(|e| e == "zip") {
            crate::archive::extract_zip(&a2, &target, |_| {})
        } else {
            let f = std::fs::File::open(&a2)?;
            tar::Archive::new(flate2::read::GzDecoder::new(f)).unpack(&target)?;
            Ok(())
        }
    })
    .await??;
    let _ = std::fs::remove_file(&archive);
    let java = find_java_in(&state.paths.java).ok_or_else(|| anyhow!("the download has no bin/java"))?;
    // the managed copy is picked automatically when no explicit path is set
    let info = detect(&state).await;
    if info.major < REQUIRED_MAJOR {
        bail!("installed {} but it does not run: {}", java.display(), info.error);
    }
    Ok(format!("Java ready: {}", info.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_versions() {
        assert_eq!(parse_major("openjdk version \"25.0.1\" 2025-10-21 LTS\nOpenJDK Runtime"), Some(25));
        assert_eq!(parse_major("java version \"1.8.0_391\""), Some(8));
        assert_eq!(parse_major("openjdk version \"21\" 2023-09-19"), Some(21));
        assert_eq!(parse_major("nonsense"), None);
    }

    #[test]
    fn finds_java_in_nested_folders() {
        let dir = std::env::temp_dir().join(format!("hyo-java-{}", crate::state::new_id()));
        let bin = dir.join("jdk-25.0.1+8-jre").join("Contents").join("Home").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(exe_name()), b"").unwrap();
        assert_eq!(find_java_in(&dir), Some(bin.join(exe_name())));
        let _ = std::fs::remove_dir_all(dir);
    }
}
