// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

/// Generate platform-specific service files for auto-start and crash recovery.
pub fn generate_service_files() -> Result<()> {
    let guixu_bin =
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/local/bin/guixu"));
    let log_dir = data_core::config::NodeConfig::config_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    #[cfg(target_os = "macos")]
    generate_launchd_plist(&guixu_bin, &log_dir)?;

    #[cfg(target_os = "linux")]
    generate_systemd_service(&guixu_bin)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn generate_launchd_plist(guixu_bin: &std::path::Path, log_dir: &std::path::Path) -> Result<()> {
    let plist_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join("Library/LaunchAgents");
    std::fs::create_dir_all(&plist_dir)?;
    let plist_path = plist_dir.join("org.guixu.node.plist");

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>org.guixu.node</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>start</string>
  </array>
  <key>KeepAlive</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>StandardOutPath</key>
  <string>{log_dir}/stdout.log</string>
  <key>StandardErrorPath</key>
  <string>{log_dir}/stderr.log</string>
  <key>RunAtLoad</key>
  <false/>
</dict>
</plist>
"#,
        bin = guixu_bin.display(),
        log_dir = log_dir.display(),
    );

    std::fs::write(&plist_path, content)?;
    info!(path = %plist_path.display(), "generated launchd plist");
    Ok(())
}

#[cfg(target_os = "linux")]
fn generate_systemd_service(guixu_bin: &std::path::Path) -> Result<()> {
    let service_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir)?;
    let service_path = service_dir.join("guixu.service");

    let content = format!(
        r#"[Unit]
Description=Guixu P2P Data Node

[Service]
ExecStart={bin} start
Restart=on-failure
RestartSec=10
WatchdogSec=120

[Install]
WantedBy=default.target
"#,
        bin = guixu_bin.display(),
    );

    std::fs::write(&service_path, content)?;
    info!(path = %service_path.display(), "generated systemd service");
    Ok(())
}
