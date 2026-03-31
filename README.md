# Guixu

Guixu is the Data Discovery and Market Platform for Autonomous AI Agents. Guixu empowers AI agents with P2P data discovery and decentralized data trading on Blockchain. Sellers upload encrypted data, buyers pay via smart contract and leave on‑chain reviews. AI agents can autonomously discover both free and paid datasets through a unified interface on Guixu.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
```

The installer downloads a prebuilt binary (or builds from source), initializes a local node, and auto-detects installed AI clients to register the Guixu MCP server.

## Quick Start

```bash
guixu start                # Start node + Web UI
open http://localhost:3927  # Drag & drop to publish datasets
```

## AI Agent Integration (MCP)

Guixu exposes an MCP server so AI agents can discover, evaluate, and purchase datasets.

### One-Command Setup

```bash
guixu mcp install claude     # Claude Desktop
guixu mcp install cursor     # Cursor
guixu mcp install codex      # Codex
guixu mcp install kiro       # Kiro
guixu mcp install windsurf   # Windsurf
```

To remove: `guixu mcp uninstall <client>`

### Manual Configuration

Add to your client's MCP config (e.g. `claude_desktop_config.json`, `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "guixu": {
      "command": "guixu",
      "args": ["mcp"]
    }
  }
}
```

### Available Tools

| Tool | Description |
|------|-------------|
| `intent_parse` | Parse natural-language data requests |
| `dataset_search` | Search the Guixu network for datasets |
| `dataset_evaluate` | Evaluate dataset suitability and value |
| `dataset_purchase` | Purchase a dataset via smart contract |
| `dataset_feedback` | Record post-use feedback on-chain |

### HTTP Mode

```bash
guixu mcp --mode http   # MCP over HTTP on :3927/rpc
```

## License

Apache-2.0
