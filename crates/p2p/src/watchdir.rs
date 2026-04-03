// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::info;

/// Events from the file watcher.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    FileCreated(PathBuf),
    FileModified(PathBuf),
}

const SUPPORTED_EXTENSIONS: &[&str] = &["csv", "parquet", "json", "tsv"];

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext))
}

/// Start watching a directory for new/modified data files.
/// Returns a receiver of WatchEvents. Runs until the sender is dropped.
pub fn watch(data_dir: &Path) -> Result<mpsc::Receiver<WatchEvent>> {
    let (tx, rx) = mpsc::channel(64);
    let dir = data_dir.to_path_buf();

    // Scan existing files first
    let tx2 = tx.clone();
    let dir2 = dir.clone();
    tokio::spawn(async move {
        if let Ok(entries) = std::fs::read_dir(&dir2) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_supported(&path) {
                    let _ = tx2.send(WatchEvent::FileCreated(path)).await;
                }
            }
        }
    });

    // Watch for new changes
    std::fs::create_dir_all(&dir)?;
    let rt_handle = tokio::runtime::Handle::current();
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                for path in &event.paths {
                    if !path.is_file() || !is_supported(path) {
                        continue;
                    }
                    let we = match event.kind {
                        EventKind::Create(_) => Some(WatchEvent::FileCreated(path.clone())),
                        EventKind::Modify(_) => Some(WatchEvent::FileModified(path.clone())),
                        _ => None,
                    };
                    if let Some(we) = we {
                        let tx = tx.clone();
                        rt_handle.spawn(async move {
                            let _ = tx.send(we).await;
                        });
                    }
                }
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(&dir, RecursiveMode::Recursive)?;

    // Keep watcher alive by leaking it (it runs in a background thread)
    std::mem::forget(watcher);

    info!(dir = %dir.display(), "watching for dataset files");
    Ok(rx)
}
