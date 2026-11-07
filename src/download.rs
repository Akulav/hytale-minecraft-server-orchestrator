//! HTTP downloads with progress reported to a task.

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

use crate::state::{Shared, human_bytes};

/// Downloads `url` to `dest` (via `dest.part`, renamed when complete).
pub async fn to_file(state: &Shared, task: &str, url: &str, dest: &Path) -> Result<()> {
    if let Some(d) = dest.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    let resp = state.http.get(url).send().await.with_context(|| format!("cannot reach {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} answered {}", resp.status());
    }
    let total = resp.content_length();
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
    let mut done: u64 = 0;
    let mut last_report = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if state.is_quitting() {
            bail!("cancelled: the program is closing");
        }
        let chunk = chunk.context("download interrupted")?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if done - last_report > 512 * 1024 || Some(done) == total {
            last_report = done;
            let msg = match total {
                Some(t) => format!("Downloading… {} / {}", human_bytes(done), human_bytes(t)),
                None => format!("Downloading… {}", human_bytes(done)),
            };
            state.task_update(task, Some(&msg), total.map(|t| done as f32 / t.max(1) as f32));
        }
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}
