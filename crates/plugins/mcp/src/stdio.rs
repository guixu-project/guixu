// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use crate::protocol::{McpRequest, McpResponse};
use crate::rpc::handle_request;
use crate::sampling::{PendingSampling, SamplingHandle};
use crate::server::McpServer;

enum MessageFormat {
    Framed,
    LineDelimited,
}

/// MCP Server — reads JSON-RPC from stdin, writes to stdout.
/// Supports bidirectional sampling: tool handlers can send
/// `sampling/createMessage` requests to the host via [`SamplingHandle`].
pub async fn run_stdio(server: Arc<McpServer>) -> Result<()> {
    let mut stdin = BufReader::new(io::stdin());
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let session_id = format!("stdio-{}", std::process::id());

    // Channel for outbound sampling requests from tool handlers.
    let (sampling_tx, mut sampling_rx) = mpsc::channel::<PendingSampling>(16);
    let sampling_handle = SamplingHandle::new(sampling_tx);
    server.set_sampling_handle(sampling_handle);

    // Pending sampling responses keyed by request id.
    let pending: Arc<
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<serde_json::Value>>>>,
    > = Arc::new(Mutex::new(HashMap::new()));

    info!("MCP server started on stdio");

    loop {
        tokio::select! {
            // Outbound: a tool handler wants to send a sampling request to the host.
            Some(req) = sampling_rx.recv() => {
                let id_key = req.id.to_string();
                pending.lock().await.insert(id_key, req.reply);
                let body = serde_json::to_vec(&req.body)?;
                let mut out = stdout.lock().await;
                let header = format!("Content-Length: {}\r\n\r\n", body.len());
                out.write_all(header.as_bytes()).await?;
                out.write_all(&body).await?;
                out.flush().await?;
            }

            // Inbound: a message from the host (could be a request or a sampling response).
            msg = read_message(&mut stdin) => {
                let Some((payload, format)) = msg? else {
                    break; // EOF
                };

                // Try to parse as a generic JSON value first to check if it's a response.
                let parsed: serde_json::Value = match serde_json::from_str(&payload) {
                    Ok(v) => v,
                    Err(e) => {
                        let resp = McpResponse::error(
                            serde_json::Value::Null,
                            -32700,
                            format!("Parse error: {e}"),
                        );
                        write_message(&stdout, &resp, format).await?;
                        continue;
                    }
                };

                // If it has a "result" or "error" field but no "method", it's a response
                // to one of our outbound sampling requests.
                let is_response = parsed.get("result").is_some() || parsed.get("error").is_some();
                let has_method = parsed.get("method").is_some();

                if is_response && !has_method {
                    let id_key = parsed.get("id").cloned().unwrap_or(serde_json::Value::Null).to_string();
                    if let Some(reply) = pending.lock().await.remove(&id_key) {
                        let result = if let Some(result) = parsed.get("result") {
                            Ok(result.clone())
                        } else {
                            let msg = parsed.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                                .unwrap_or("unknown sampling error");
                            Err(anyhow::anyhow!("host sampling error: {msg}"))
                        };
                        let _ = reply.send(result);
                    }
                    continue;
                }

                // Otherwise it's a normal MCP request from the host.
                let response = match serde_json::from_value::<McpRequest>(parsed) {
                    Ok(req) => handle_request(req, &server, &session_id).await,
                    Err(e) => Some(McpResponse::error(
                        serde_json::Value::Null,
                        -32700,
                        format!("Parse error: {e}"),
                    )),
                };

                if let Some(response) = response {
                    write_message(&stdout, &response, format).await?;
                }
            }
        }
    }

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
    stdout: &Arc<Mutex<io::Stdout>>,
    response: &McpResponse,
    format: MessageFormat,
) -> Result<()> {
    let body = serde_json::to_vec(response)?;
    let mut out = stdout.lock().await;
    match format {
        MessageFormat::Framed => {
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            out.write_all(header.as_bytes()).await?;
            out.write_all(&body).await?;
        }
        MessageFormat::LineDelimited => {
            out.write_all(&body).await?;
            out.write_all(b"\n").await?;
        }
    }
    out.flush().await?;
    Ok(())
}
