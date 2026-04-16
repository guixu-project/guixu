// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::config::DaemonConfig;
use data_core::types::HealthReport;
use data_p2p::network::NetworkCommand;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::warn;

/// Internal watchdog that periodically checks node health.
pub struct WatchdogTask {
    cmd_tx: mpsc::Sender<NetworkCommand>,
    http_port: u16,
    data_dir: PathBuf,
    config: DaemonConfig,
    consecutive_failures: u32,
}

impl WatchdogTask {
    pub fn new(
        cmd_tx: mpsc::Sender<NetworkCommand>,
        http_port: u16,
        data_dir: PathBuf,
        config: DaemonConfig,
    ) -> Self {
        Self {
            cmd_tx,
            http_port,
            data_dir,
            config,
            consecutive_failures: 0,
        }
    }

    /// Run the watchdog loop. Call this from a spawned task.
    pub async fn run(mut self) {
        let interval = Duration::from_secs(self.config.watchdog_interval_secs);
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let report = self.health_check().await;
            if report.p2p_ok && report.http_ok && report.db_ok && report.disk_ok && report.memory_ok
            {
                self.consecutive_failures = 0;
            } else {
                self.consecutive_failures += 1;
                self.on_unhealthy(&report).await;
            }
        }
    }

    async fn health_check(&self) -> HealthReport {
        let mut report = HealthReport::default();

        // 1. P2P network: try_send a Ping to check channel is alive
        let (tx, rx) = tokio::sync::oneshot::channel();
        report.p2p_ok = self
            .cmd_tx
            .try_send(NetworkCommand::Ping { reply: tx })
            .is_ok();
        if report.p2p_ok {
            report.p2p_ok = tokio::time::timeout(Duration::from_secs(2), rx)
                .await
                .is_ok();
        }

        // 2. HTTP service: GET /api/node/status
        report.http_ok = check_http(self.http_port).await;

        // 3. RocksDB: try opening the store (lightweight check)
        report.db_ok = data_storage::metadata_store::MetadataStore::open(
            &data_core::config::NodeConfig::db_path(),
        )
        .is_ok();

        // 4. Disk space
        report.disk_ok = check_disk_space(&self.data_dir, self.config.disk_min_free_mb);

        // 5. Memory RSS
        report.memory_ok = check_memory(self.config.memory_limit_mb);

        report
    }

    async fn on_unhealthy(&mut self, report: &HealthReport) {
        if !report.p2p_ok {
            warn!("watchdog: P2P network unhealthy");
        }
        if !report.http_ok {
            warn!("watchdog: HTTP service unhealthy");
        }
        if !report.disk_ok {
            warn!("watchdog: disk space low, pausing auto-publish");
        }
        if !report.memory_ok {
            warn!("watchdog: memory usage high");
        }

        if self.consecutive_failures >= self.config.watchdog_max_failures {
            warn!(
                failures = self.consecutive_failures,
                "watchdog: max failures reached, exiting for L2 restart"
            );
            std::process::exit(1);
        }
    }
}

async fn check_http(port: u16) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build();
    let Ok(client) = client else { return false };
    client
        .get(format!("http://127.0.0.1:{port}/api/node/status"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn check_disk_space(path: &std::path::Path, min_free_mb: u64) -> bool {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    for disk in disks.list() {
        if path.starts_with(disk.mount_point()) {
            let free_mb = disk.available_space() / (1024 * 1024);
            return free_mb >= min_free_mb;
        }
    }
    true // If we can't determine, assume OK
}

fn check_memory(limit_mb: u64) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    if let Some(proc) = sys.process(pid) {
        let rss_mb = proc.memory() / (1024 * 1024);
        return rss_mb < limit_mb;
    }
    true
}
