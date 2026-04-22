// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! CLI baseline tests for data-node.
//!
//! Tests CLI argument parsing, help output, and basic command structure.
//!
//! These tests do NOT require a built binary or any runtime services.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn data_node_binary() -> Option<PathBuf> {
    // Try to find the debug binary
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = PathBuf::from(manifest_dir)
        .parent()?
        .parent()?
        .parent()?
        .to_path_buf();
    let binary = workspace_root.join("target/debug/data-node");
    if binary.exists() {
        Some(binary)
    } else {
        None
    }
}

fn cmd() -> Command {
    if let Some(binary) = data_node_binary() {
        Command::new(binary)
    } else {
        // Fall back to cargo run for testing
        Command::cargo_bin("data-node").expect("data-node binary not found")
    }
}

#[test]
fn cli_has_help() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Guixu: On-Chain Data Valuation for AI Agents",
        ));
}

#[test]
fn cli_init_subcommand_exists() {
    cmd()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialize a new node"));
}

#[test]
fn cli_start_subcommand_exists() {
    cmd()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Start the full node"));
}

#[test]
fn cli_stop_subcommand_exists() {
    cmd()
        .args(["stop", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stop a running daemon"));
}

#[test]
fn cli_status_subcommand_exists() {
    cmd()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Show node status"));
}

#[test]
fn cli_publish_subcommand_exists() {
    cmd()
        .args(["publish", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Publish a local file"));
}

#[test]
fn cli_unpublish_subcommand_exists() {
    cmd()
        .args(["unpublish", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove a dataset"));
}

#[test]
fn cli_list_subcommand_exists() {
    cmd()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List published datasets"));
}

#[test]
fn cli_preview_subcommand_exists() {
    cmd()
        .args(["preview", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Preview a dataset"));
}

#[test]
fn cli_mcp_subcommand_exists() {
    cmd()
        .args(["mcp", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run as MCP server only"));
}

#[test]
fn cli_trace_subcommand_exists() {
    cmd()
        .args(["trace", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Manage AI agent traces"));
}

#[test]
fn cli_trace_import_subcommand_exists() {
    cmd()
        .args(["trace", "import", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Import traces"));
}

#[test]
fn cli_trace_export_subcommand_exists() {
    cmd()
        .args(["trace", "export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Export traces"));
}

#[test]
fn cli_trace_list_subcommand_exists() {
    cmd()
        .args(["trace", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List recent traces"));
}

#[test]
fn cli_trace_query_subcommand_exists() {
    cmd()
        .args(["trace", "query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Query spans"));
}

#[test]
fn cli_mcp_install_subcommand_exists() {
    cmd()
        .args(["mcp", "install", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Register Guixu MCP"));
}

#[test]
fn cli_mcp_uninstall_subcommand_exists() {
    cmd()
        .args(["mcp", "uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove Guixu MCP"));
}

#[test]
fn cli_publish_requires_path() {
    // Running publish without a path should fail
    cmd()
        .args(["publish"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
}

#[test]
fn cli_preview_requires_cid() {
    // Running preview without cid should fail
    cmd()
        .args(["preview"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
}

#[test]
fn cli_trace_import_requires_provider_and_file() {
    cmd()
        .args(["trace", "import"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
}
