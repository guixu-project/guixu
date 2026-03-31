# Guixu

Guixu is the Data Discovery and Market Platform for Autonomous AI Agents. Guixu empowers AI agents with P2P data discovery and decentralized data trading on Blockchain. Sellers upload encrypted data, buyers pay via smart contract and leave on‑chain reviews. AI agents can autonomously discover both free and paid datasets through a unified interface on Guixu.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
```

### configure local guixu MCP for Codex
```bash
./scripts/configure_codex_mcp.sh
```

This script configures `~/.codex/config.toml` and writes a managed Guixu MCP guidance block into `AGENTS.md`.

## License

Apache-2.0
