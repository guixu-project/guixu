use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

use crate::protocol::{McpRequest, McpResponse};
use crate::rpc::handle_request;
use crate::server::McpServer;

enum MessageFormat {
    Framed,
    LineDelimited,
}

/// MCP Server — reads JSON-RPC from stdin, writes to stdout.
pub async fn run_stdio(server: Arc<McpServer>) -> Result<()> {
    let mut stdin = BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    let session_id = format!("stdio-{}", std::process::id());

    info!("MCP server started on stdio");

    while let Some((payload, format)) = read_message(&mut stdin).await? {
        let response = match serde_json::from_str::<McpRequest>(&payload) {
            Ok(req) => handle_request(req, &server, &session_id).await,
            Err(e) => Some(McpResponse::error(
                serde_json::Value::Null,
                -32700,
                format!("Parse error: {e}"),
            )),
        };

        if let Some(response) = response {
            write_message(&mut stdout, &response, format).await?;
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
    stdout: &mut io::Stdout,
    response: &McpResponse,
    format: MessageFormat,
) -> Result<()> {
    let body = serde_json::to_vec(response)?;
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
