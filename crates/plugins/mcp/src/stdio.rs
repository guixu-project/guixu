// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use crate::protocol::{McpRequest, McpResponse};
use crate::rpc::handle_request;
use crate::server::McpServer;

#[derive(Clone, Copy)]
enum MessageFormat {
    Framed,
    LineDelimited,
}

/// MCP Server — reads JSON-RPC from stdin, writes to stdout.
pub async fn run_stdio(server: Arc<McpServer>) -> Result<()> {
    let mut stdin = BufReader::new(io::stdin());
    let session_id = format!("stdio-{}", std::process::id());
    let active_format = Arc::new(RwLock::new(MessageFormat::LineDelimited));
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Value>();
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<McpRequest>();

    if let Some(runtime) = server.state().sampling_runtime.as_ref() {
        runtime.attach_sender(outbound_tx.clone());
    }

    let writer_format = active_format.clone();
    let writer_task = tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Some(payload) = outbound_rx.recv().await {
            let format = *writer_format.read().await;
            write_message(&mut stdout, &payload, format).await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    info!("MCP server started on stdio");

    let worker_server = server.clone();
    let worker_session_id = session_id.clone();
    let worker_outbound_tx = outbound_tx.clone();
    let worker_task = tokio::spawn(async move {
        while let Some(request) = request_rx.recv().await {
            let response = handle_request(request, &worker_server, &worker_session_id).await;
            if let Some(response) = response {
                worker_outbound_tx
                    .send(serde_json::to_value(response)?)
                    .map_err(|_| anyhow::anyhow!("failed to send stdio response"))?;
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    while let Some((payload, format)) = read_message(&mut stdin).await? {
        *active_format.write().await = format;

        let value = match serde_json::from_str::<Value>(&payload) {
            Ok(value) => value,
            Err(error) => {
                let response = McpResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {error}"),
                );
                let _ = outbound_tx.send(serde_json::to_value(response)?);
                continue;
            }
        };

        if value.get("method").is_none() {
            let handled = server
                .state()
                .sampling_runtime
                .as_ref()
                .is_some_and(|runtime| runtime.handle_response(&value));
            if !handled {
                warn!(payload = %value, "ignoring unexpected JSON-RPC response on stdio");
            }
            continue;
        }

        let request = match serde_json::from_value::<McpRequest>(value) {
            Ok(request) => request,
            Err(error) => {
                let response = McpResponse::error(
                    serde_json::Value::Null,
                    -32600,
                    format!("invalid JSON-RPC request: {error}"),
                );
                let _ = outbound_tx.send(serde_json::to_value(response)?);
                continue;
            }
        };

        request_tx
            .send(request)
            .map_err(|_| anyhow::anyhow!("failed to queue stdio request"))?;
    }

    if let Some(runtime) = server.state().sampling_runtime.as_ref() {
        runtime.shutdown("MCP stdio input closed");
    }

    drop(request_tx);
    worker_task.await??;

    if let Some(runtime) = server.state().sampling_runtime.as_ref() {
        runtime.detach_sender();
    }
    drop(outbound_tx);
    writer_task.await??;
    Ok(())
}

async fn read_message(stdin: &mut BufReader<io::Stdin>) -> Result<Option<(String, MessageFormat)>> {
    let mut first_line = String::new();

    loop {
        first_line.clear();
        let bytes_read = stdin.read_line(&mut first_line).await?;
        if bytes_read == 0 {
            return Ok(None);
        }

        if first_line.trim().is_empty() {
            continue;
        }

        if first_line.trim_start().starts_with('{') {
            return Ok(Some((first_line, MessageFormat::LineDelimited)));
        }

        let mut content_length = parse_content_length(&first_line);
        let mut header_line = String::new();
        loop {
            header_line.clear();
            let bytes_read = stdin.read_line(&mut header_line).await?;
            if bytes_read == 0 {
                anyhow::bail!("unexpected EOF while reading MCP headers");
            }

            let trimmed = header_line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }

            if let Some(value) = parse_content_length(&header_line) {
                content_length = Some(value);
            }
        }

        if let Some(len) = content_length {
            let mut body = vec![0u8; len];
            stdin
                .read_exact(&mut body)
                .await
                .context("failed to read MCP message body")?;
            let payload = String::from_utf8(body).context("MCP message body is not valid UTF-8")?;
            return Ok(Some((payload, MessageFormat::Framed)));
        }

        warn!(
            header = first_line.trim_end(),
            "ignoring unsupported stdio input while waiting for MCP frame"
        );
    }
}

fn parse_content_length(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse().ok()
}

async fn write_message(
    stdout: &mut io::Stdout,
    payload: &Value,
    format: MessageFormat,
) -> Result<()> {
    let body = serde_json::to_vec(payload)?;
    match format {
        MessageFormat::Framed => {
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            stdout.write_all(header.as_bytes()).await?;
            stdout.write_all(&body).await?;
        }
        MessageFormat::LineDelimited => {
            stdout.write_all(&body).await?;
            stdout.write_all(b"\n").await?;
        }
    }
    stdout.flush().await?;
    Ok(())
}
