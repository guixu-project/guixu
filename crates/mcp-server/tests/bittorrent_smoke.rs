use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use data_p2p::torrent::TorrentEngine;
use data_search::adapters::{BitTorrentAdapter, ExternalAdapter};
use serde_json::Value;
use tokio::time::{sleep, timeout};

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_millis();
    let dir = std::env::temp_dir()
        .join("guixu-bt-smoke")
        .join(name)
        .join(format!("{}-{ts}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

#[tokio::test]
#[ignore = "network-dependent smoke test against public bittorrent search and peers"]
async fn cat_bittorrent_search_preview_and_download_first_result() {
    let adapter = BitTorrentAdapter::default();
    let results = timeout(Duration::from_secs(20), adapter.search("cat", 5))
        .await
        .expect("BT search timed out")
        .expect("BT search failed");

    assert!(
        !results.is_empty(),
        "expected bittorrent search for 'cat' to return at least one result"
    );

    let first = &results[0];
    eprintln!(
        "selected first BT result: title={:?} info_hash={} description={:?}",
        first.title, first.cid.0, first.description
    );

    let engine = TorrentEngine::new(temp_dir("cat-preview-download"))
        .await
        .expect("create torrent engine");

    let preview_ok = match timeout(
        Duration::from_secs(60),
        engine.download_preview(&first.cid.0, 4096),
    )
    .await
    {
        Ok(Ok(preview)) => {
            eprintln!("preview bytes read: {}", preview.len());
            !preview.is_empty()
        }
        Ok(Err(err)) => {
            eprintln!("preview failed: {err}");
            false
        }
        Err(err) => {
            eprintln!("preview timed out: {err:?}");
            false
        }
    };

    let start_download_ok =
        match timeout(Duration::from_secs(20), engine.start_download(&first.cid.0)).await {
            Ok(Ok(())) => true,
            Ok(Err(err)) => {
                eprintln!("start download failed: {err}");
                false
            }
            Err(err) => {
                eprintln!("start download timed out: {err:?}");
                false
            }
        };

    let mut saw_session = false;
    let mut saw_progress = false;
    if start_download_ok {
        for attempt in 1..=30 {
            match engine.get_stats(&first.cid.0) {
                Ok(stats) => {
                    saw_session = true;
                    let progress_bytes = json_u64(&stats, "progress_bytes");
                    let total_bytes = json_u64(&stats, "total_bytes");
                    let finished = stats
                        .get("finished")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let state = stats
                        .get("state")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let speed = stats
                        .get("download_speed")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    eprintln!(
                        "attempt={attempt} state={state} progress_bytes={progress_bytes} total_bytes={total_bytes} speed={speed} finished={finished}"
                    );

                    if progress_bytes > 0 || finished {
                        saw_progress = true;
                        break;
                    }
                }
                Err(err) => {
                    eprintln!("attempt={attempt} stats error: {err}");
                }
            }

            sleep(Duration::from_secs(2)).await;
        }
    }

    assert!(
        start_download_ok,
        "start download for first BT result did not succeed"
    );
    assert!(
        saw_session,
        "download never became visible in torrent session"
    );
    assert!(
        saw_progress,
        "download for first BT result made no progress within 60 seconds"
    );
    assert!(preview_ok, "preview for first BT result did not succeed");
}
