<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Guixu Python SDK

A Python SDK for the Guixu P2P Data Platform — a decentralized marketplace for datasets.

## Installation

```bash
pip install guixu
```

Or install from source:

```bash
cd python
pip install -e .
```

## Quick Start

```python
import guixu

# Search for datasets
results = guixu.search("sentiment analysis")
print(f"Found {len(results)} datasets")

# Load a dataset by CID
ds = guixu.load_dataset("QmXxx...")
print(f"Dataset: {ds.title}")

# Convert to pandas DataFrame
df = ds.to_pandas()
print(df.head())

# Or to Polars DataFrame
df = ds.to_polars()
```

## Search API

```python
import guixu

# Basic search
results = guixu.search("machine learning")

# Search with limit
results = guixu.search("image classification", limit=5)

# Access search result metadata
for r in results:
    print(f"{r.title} ({r.cid})")
    print(f"  Tags: {r.tags}")
    print(f"  License: {r.license}")
    print(f"  Size: {r.schema.size_bytes / 1024 / 1024:.1f} MB")
```

## Dataset API

```python
import guixu

# Load a dataset
ds = guixu.load_dataset("QmXxx...")

# Get schema information
print(f"Columns: {[c.name for c in ds.schema.columns]}")
print(f"Row count: {ds.schema.row_count}")

# Convert to DataFrame
df = ds.to_pandas()
```

## Publishing Datasets

```python
import guixu

# Publish a local dataset to the P2P network
cid = guixu.publish(
    file_path="data.csv",
    title="My Dataset",
    description="A dataset for testing",
    tags=["test", "sample"],
    license="MIT"
)
print(f"Published to {cid}")
```

## Configuration

The SDK connects to the local Guixu MCP server by default.

### Environment Variables

- `GUIXU_MCP_COMMAND`: Path to the data-node binary (default: "data-node")
- `GUIXU_MCP_URL`: HTTP URL of the MCP server (e.g., "http://localhost:3927/mcp")

### Custom MCP Server

```python
from guixu import GuixuClient

# Use a custom MCP server binary
client = GuixuClient(mcp_command="/path/to/data-node")

# Or use HTTP endpoint
import os
os.environ["GUIXU_MCP_URL"] = "http://localhost:3927/mcp"
client = GuixuClient()
```

## Requirements

- Python 3.10+
- pandas (optional, for `to_pandas()`)
- polars (optional, for `to_polars()`)
- requests (for HTTP transport)

## License

Apache-2.0
