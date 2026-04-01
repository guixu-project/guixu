# Guixu

Guixu is the Data Discovery and Market Platform for Autonomous AI Agents. Guixu empowers AI agents with P2P data discovery and decentralized data trading on Blockchain. Sellers upload encrypted data, buyers pay via smart contract and leave on‑chain reviews. AI agents can autonomously discover both free and paid datasets through a unified interface on Guixu.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
```

That's it. The installer downloads the binary, initializes a local node, auto-detects your AI clients (Codex, Cursor, Claude Code, OpenCode), and registers the Guixu MCP server with each one.


## AI Agent Integration (MCP)

Guixu exposes an MCP server so AI agents can discover, evaluate, and purchase datasets.

### Register / Unregister

```bash
guixu mcp install codex       # Codex
guixu mcp install cursor      # Cursor
guixu mcp install claude-code # Claude Code
guixu mcp install opencode    # OpenCode
```

To remove: `guixu mcp uninstall <client>`

### Manual Configuration

Add to your client's MCP config:

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

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or <https://www.apache.org/licenses/LICENSE-2.0>).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you shall be licensed under the Apache-2.0 license, without any additional terms or conditions.
