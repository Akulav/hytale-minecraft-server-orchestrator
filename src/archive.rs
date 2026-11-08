//! Zip helpers: extract (safely, inside the target), pack folders, copy trees with progress.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

/// Extracts every entry of `zip_path` into `dest`. Entries that would land outside `dest`
/// (`../`, absolute paths) are skipped. `progress` gets 0..1.
pub fn extract_zip(zip_path: &Path, dest: &Path, mut progress: impl FnMut(f32)) -> Result<()> {
    let file = File::open(zip_path).with_context(|| format!("cannot open {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).with_context(|| format!("{} is not a zip file", zip_path.display()))?;
    let total: u64 = (0..archive.len()).filter_map(|i| archive.by_index_raw(i).ok().map(|e| e.size())).sum::<u64>().max(1);
    let mut done = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut f = File::create(&out).with_context(|| format!("cannot write {}", out.display()))?;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n])?;
            done += n as u64;
            progress(done as f32 / total as f32);
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777));
        }
    }
    progress(1.0);
    Ok(())
}

/// Packs `items` (files or folders, relative to `base`; missing ones are skipped) into `zip_path`.
/// Returns the number of files written.
pub fn zip_paths(base: &Path, items: &[&str], zip_path: &Path) -> Result<usize> {
    if let Some(p) = zip_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let part = zip_path.with_extension("zip.part");
    let result = zip_into(base, items, &part);
    match result {
        Ok(n) => {
            std::fs::rename(&part, zip_path)?;
            Ok(n)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Lock files a running server holds open (Minecraft's `session.lock`): never needed in a backup.
fn skip_in_backup(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "session.lock")
}

fn zip_into(base: &Path, items: &[&str], part: &Path) -> Result<usize> {
    let file = File::create(part)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let mut count = 0usize;
    for item in items {
        let path = base.join(item);
        if path.is_file() {
            add_file(&mut zw, &path, item, opts)?;
            count += 1;
        } else if path.is_dir() {
            for f in walk_files(&path).into_iter().filter(|f| !skip_in_backup(f)) {
                let rel = f.strip_prefix(base).unwrap_or(&f);
                let name = rel.to_string_lossy().replace('\\', "/");
                add_file(&mut zw, &f, &name, opts)?;
                count += 1;
            }
            zw.add_directory(format!("{}/", item.trim_end_matches('/')), opts)?;
        }
    }
    zw.finish()?;
    Ok(count)
}

fn add_file(zw: &mut zip::ZipWriter<File>, path: &Path, name: &str, opts: SimpleFileOptions) -> Result<()> {
    zw.start_file(name, opts)?;
    let mut f = File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    std::io::copy(&mut f, zw)?;
    Ok(())
}

/// All files below `dir` (no symlink following).
pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

/// Copies the tree `src` into `dst` (overwriting files), with progress over bytes.
pub fn copy_tree(src: &Path, dst: &Path, mut progress: impl FnMut(f32)) -> Result<()> {
    let files = walk_files(src);
    let total: u64 = files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum::<u64>().max(1);
    let mut done = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    for f in files {
        let rel = f.strip_prefix(src).unwrap_or(&f);
        let out = dst.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut r = File::open(&f)?;
        let mut w = File::create(&out).with_context(|| format!("cannot write {}", out.display()))?;
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            done += n as u64;
            progress(done as f32 / total as f32);
        }
    }
    progress(1.0);
    Ok(())
}

/// A zip entry's text (e.g. `META-INF/MANIFEST.MF` inside a jar).
pub fn read_entry(zip_path: &Path, name: &str) -> Option<String> {
    let mut archive = zip::ZipArchive::new(File::open(zip_path).ok()?).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_roundtrip_keeps_tree_and_skips_missing() {
        let root = std::env::temp_dir().join(format!("hyo-zip-{}", crate::state::new_id()));
        let src = root.join("src");
        std::fs::create_dir_all(src.join("universe/worlds/default")).unwrap();
        std::fs::write(src.join("universe/worlds/default/config.json"), b"{\"Seed\": 1}").unwrap();
        std::fs::create_dir_all(src.join("mods")).unwrap();
        std::fs::write(src.join("mods/a.jar"), b"jar").unwrap();
        std::fs::write(src.join("config.json"), b"{}").unwrap();
        std::fs::write(src.join("universe/session.lock"), b"x").unwrap();
        let z = root.join("b.zip");
        let n = zip_paths(&src, &["universe", "mods", "config.json", "missing.json"], &z).unwrap();
        assert_eq!(n, 3);
        assert!(!z.with_extension("zip.part").exists());
        let out = root.join("out");
        let mut last = 0.0;
        extract_zip(&z, &out, |p| last = p).unwrap();
        assert_eq!(last, 1.0);
        assert_eq!(std::fs::read(out.join("universe/worlds/default/config.json")).unwrap(), b"{\"Seed\": 1}");
        assert_eq!(std::fs::read(out.join("mods/a.jar")).unwrap(), b"jar");
        assert_eq!(read_entry(&z, "config.json").as_deref(), Some("{}"));
        let _ = std::fs::remove_dir_all(root);
    }
}
