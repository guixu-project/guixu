# Guixu

Guixu: Valuation-Driven Data Discovery for Autonomous AI Agents with On-Chain Attestation

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
```

### install local guixu MCP（still have some bugs, will affect run-demo.sh if we install local guixu MCP）
```bash
cargo build --release -p data-node
./target/release/data-node mcp --mode codex
```

## License

Apache-2.0
