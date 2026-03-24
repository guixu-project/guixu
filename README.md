# Guixu

On-chain data valuation protocol for AI agents. Publish datasets to a P2P network, let agents search/evaluate/purchase them with built-in privacy protection and community feedback.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/guixu-project/guixu.git
cd guixu && cargo build --release
cp target/release/data-node ~/.local/bin/guixu
```

## Quick Start

```bash
# 1. Initialize (generates identity + config)
guixu init

# 2. Start node (P2P network + Web UI + file watcher)
guixu start
# → Open http://localhost:3927 to publish datasets via drag & drop

# 3. Or just drop files into the watched directory
cp my_data.csv ~/shared-datasets/   # auto-published to network!
```

## Web UI

Start the node and open `http://localhost:3927` in your browser:

- **Drag & drop** CSV/Parquet/JSON files to publish
- **Configure privacy** level (Off / Standard / Strict)
- **View** all published datasets with schema, quality scores, and CIDs
- **Monitor** P2P network status

## AI Agent Integration (MCP)

Guixu exposes an [MCP](https://modelcontextprotocol.io) server that any AI agent can connect to:

```bash
# stdio mode (for Claude, Cursor, etc.)
guixu mcp

# HTTP mode (for custom agents)
guixu mcp --mode http   # POST http://localhost:3927/rpc
```

### MCP Tools

| Tool | Description |
|------|-------------|
| `dataset_search` | Multi-source search (P2P, Kaggle, HuggingFace, IPFS, DuckDB) |
| `dataset_evaluate` | Task-Conditioned Value (TCV) scoring [-100, +100] |
| `dataset_publish` | Publish local file to P2P network |
| `dataset_purchase` | Auto-pay via x402 / MPP / ERC-8183 escrow |
| `dataset_feedback` | Submit on-chain usage attestation |
| `dataset_verify` | Cryptographic integrity + provenance check |
| `dataset_reviews` | View community feedback for a dataset |

### Example: Claude Desktop

Add to `claude_desktop_config.json`:

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

## Privacy

Guixu protects your data by default. Only **metadata** (schema, statistics, CID) is published — raw data never leaves your machine unless explicitly shared.

Configure in `~/.data-node/config.toml`:

```toml
privacy_level = "standard"   # off | standard | strict
privacy_epsilon = 1.0        # differential privacy epsilon
disable_mdns = true          # prevent local IP broadcast
ephemeral_dids = false       # unlinkable per-dataset identities
```

| Level | Stats | Column Names | Min/Max | mDNS |
|-------|-------|-------------|---------|------|
| `off` | Raw | Raw | Raw | On |
| `standard` | DP noise (ε) | Sensitive cols hashed | DP noise | Off |
| `strict` | DP noise (ε) | All cols hashed | Suppressed | Off |

## Architecture

```
crates/
├── core/        # Types, config, identity (Ed25519 + did:key)
├── auth/        # Privacy (DP), watermarking, W3C VC, verification
├── storage/     # RocksDB metadata + feedback stores
├── p2p/         # libp2p (Kademlia DHT + GossipSub + mDNS)
├── search/      # Multi-source search engine + adapters
├── valuation/   # TCV engine, quality scorer, pricing, ROI evaluator
├── trading/     # Payment routing (x402, MPP, ERC-8183 escrow)
├── mcp-server/  # MCP protocol + HTTP bridge + embedded Web UI
└── node/        # CLI binary (init / start / mcp)
```

## TCV — Task-Conditioned Value

The core valuation formula:

```
TCV(D, T, C) = α·SchemaFit + β·TemporalFit + γ·InfoGain
             + δ·Quality + ε·CommunitySignal − ζ·RiskPenalty
```

Range: [-100, +100]. Negative means the dataset would likely harm the task.

## License

Apache-2.0
