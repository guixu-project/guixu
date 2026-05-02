#!/usr/bin/env python3
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

"""
Automated packaging script for Guixu plugins.

This script creates plugin packages for Codex Desktop and Claude Cowork
from the Guixu source tree.
"""

import argparse
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Dict, Any


def get_project_root() -> Path:
    """Find the project root directory."""
    current = Path(__file__).resolve().parent
    while current != current.parent:
        if (current / "Cargo.toml").exists() and (current / "crates").exists():
            return current
        current = current.parent
    raise RuntimeError("Could not find project root")


def load_version() -> str:
    """Load version from Cargo.toml."""
    root = get_project_root()
    cargo_toml = root / "Cargo.toml"
    
    # Simple parsing - look for version in workspace.package
    with open(cargo_toml, 'r') as f:
        in_workspace_package = False
        for line in f:
            line = line.strip()
            if line == '[workspace.package]':
                in_workspace_package = True
            elif line.startswith('[') and in_workspace_package:
                break
            elif in_workspace_package and line.startswith('version'):
                # Extract version value
                _, value = line.split('=', 1)
                return value.strip().strip('"')
    
    return "0.1.0"  # fallback


def create_codex_plugin(output_dir: Path, version: str) -> Path:
    """Create Codex Desktop plugin package."""
    plugin_dir = output_dir / "guixu-codex-plugin"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    
    # Create .codex-plugin directory
    codex_plugin_dir = plugin_dir / ".codex-plugin"
    codex_plugin_dir.mkdir(exist_ok=True)
    
    # Create plugin.json
    plugin_json = {
        "name": "guixu",
        "version": version,
        "description": "Dataset discovery, valuation, and acquisition for AI agents. Search, evaluate, and purchase datasets from Kaggle, HuggingFace, IPFS, and more.",
        "skills": "./skills/",
        "mcpServers": "./.mcp.json"
    }
    
    with open(codex_plugin_dir / "plugin.json", 'w') as f:
        json.dump(plugin_json, f, indent=2)
    
    # Create .mcp.json
    mcp_json = {
        "mcpServers": {
            "guixu": {
                "command": "guixu",
                "args": ["mcp"],
                "description": "Guixu dataset discovery and acquisition MCP server"
            }
        }
    }
    
    with open(plugin_dir / ".mcp.json", 'w') as f:
        json.dump(mcp_json, f, indent=2)
    
    # Copy skills directory
    src_skills = get_project_root() / "skills"
    dst_skills = plugin_dir / "skills"
    if src_skills.exists():
        shutil.copytree(src_skills, dst_skills)
    
    # Copy marketplace.json if exists
    marketplace_file = get_project_root() / "marketplace.json"
    if marketplace_file.exists():
        shutil.copy(marketplace_file, plugin_dir / "marketplace.json")
    
    # Create README
    readme_content = f"""# Guixu Plugin for Codex Desktop

## Overview

Guixu provides dataset discovery, valuation, and acquisition capabilities for AI agents.

## Installation

1. Install the Guixu CLI:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
   ```

2. Install the plugin in Codex Desktop:
   ```bash
   codex plugin install ./guixu-codex-plugin
   ```

## Usage

The plugin provides the following tools:
- `intent_parse`: Parse natural language data requests
- `dataset_search`: Search datasets across multiple sources
- `dataset_evaluate`: Evaluate dataset quality and relevance
- `dataset_purchase`: Purchase datasets
- `dataset_feedback`: Record post-use feedback

## Configuration

The MCP server is configured to run `guixu mcp` by default. 
Ensure the `guixu` CLI is in your PATH.

## Version

{version}
"""
    
    with open(plugin_dir / "README.md", 'w') as f:
        f.write(readme_content)
    
    return plugin_dir


def create_claude_plugin(output_dir: Path, version: str) -> Path:
    """Create Claude Cowork plugin package."""
    plugin_dir = output_dir / "guixu-claude-plugin"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    
    # Create .claude-plugin directory
    claude_plugin_dir = plugin_dir / ".claude-plugin"
    claude_plugin_dir.mkdir(exist_ok=True)
    
    # Create plugin.json
    plugin_json = {
        "name": "guixu",
        "description": "Dataset discovery, valuation, and acquisition for AI agents. Search, evaluate, and purchase datasets from Kaggle, HuggingFace, IPFS, and more.",
        "version": version,
        "author": {
            "name": "Guixu Project"
        }
    }
    
    with open(claude_plugin_dir / "plugin.json", 'w') as f:
        json.dump(plugin_json, f, indent=2)
    
    # Create .mcp.json
    mcp_json = {
        "mcpServers": {
            "guixu": {
                "command": "guixu",
                "args": ["mcp"],
                "description": "Guixu dataset discovery and acquisition MCP server"
            }
        }
    }
    
    with open(plugin_dir / ".mcp.json", 'w') as f:
        json.dump(mcp_json, f, indent=2)
    
    # Copy skills directory
    src_skills = get_project_root() / "skills"
    dst_skills = plugin_dir / "skills"
    if src_skills.exists():
        shutil.copytree(src_skills, dst_skills)
    
    # Copy marketplace.json if exists
    marketplace_file = get_project_root() / "marketplace.json"
    if marketplace_file.exists():
        shutil.copy(marketplace_file, plugin_dir / "marketplace.json")
    
    # Create README
    readme_content = f"""# Guixu Plugin for Claude Cowork

## Overview

Guixu provides dataset discovery, valuation, and acquisition capabilities for AI agents.

## Installation

1. Install the Guixu CLI:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
   ```

2. Install the plugin in Claude Cowork:
   ```bash
   claude plugin install ./guixu-claude-plugin
   ```

## Usage

The plugin provides the following tools:
- `intent_parse`: Parse natural language data requests
- `dataset_search`: Search datasets across multiple sources
- `dataset_evaluate`: Evaluate dataset quality and relevance
- `dataset_purchase`: Purchase datasets
- `dataset_feedback`: Record post-use feedback

## Configuration

The MCP server is configured to run `guixu mcp` by default. 
Ensure the `guixu` CLI is in your PATH.

## Version

{version}
"""
    
    with open(plugin_dir / "README.md", 'w') as f:
        f.write(readme_content)
    
    return plugin_dir


def create_combined_package(output_dir: Path, version: str) -> Path:
    """Create a combined package with both plugins."""
    combined_dir = output_dir / "guixu-plugins"
    combined_dir.mkdir(parents=True, exist_ok=True)
    
    # Create Codex plugin
    codex_dir = combined_dir / "codex"
    codex_dir.mkdir(exist_ok=True)
    create_codex_plugin(codex_dir, version)
    
    # Create Claude plugin
    claude_dir = combined_dir / "claude"
    claude_dir.mkdir(exist_ok=True)
    create_claude_plugin(claude_dir, version)
    
    # Create combined README
    readme_content = f"""# Guixu Plugins

This package contains plugins for both Codex Desktop and Claude Cowork.

## Contents

- `codex/`: Codex Desktop plugin
- `claude/`: Claude Cowork plugin

## Installation

### Codex Desktop

```bash
cd codex
codex plugin install ./guixu-codex-plugin
```

### Claude Cowork

```bash
cd claude
claude plugin install ./guixu-claude-plugin
```

## Version

{version}
"""
    
    with open(combined_dir / "README.md", 'w') as f:
        f.write(readme_content)
    
    return combined_dir


def main():
    parser = argparse.ArgumentParser(description="Package Guixu plugins for Codex Desktop and Claude Cowork")
    parser.add_argument("--output", "-o", type=Path, default=Path("dist"),
                       help="Output directory for plugin packages")
    parser.add_argument("--version", "-v", type=str, default=None,
                       help="Plugin version (default: from Cargo.toml)")
    parser.add_argument("--type", "-t", choices=["codex", "claude", "both"], default="both",
                       help="Which plugin to create (default: both)")
    parser.add_argument("--clean", "-c", action="store_true",
                       help="Clean output directory before packaging")
    
    args = parser.parse_args()
    
    # Get version
    version = args.version or load_version()
    
    # Clean output directory if requested
    if args.clean and args.output.exists():
        shutil.rmtree(args.output)
    
    # Create output directory
    args.output.mkdir(parents=True, exist_ok=True)
    
    print(f"Packaging Guixu plugins v{version}")
    print(f"Output directory: {args.output}")
    
    try:
        if args.type in ["codex", "both"]:
            print("Creating Codex Desktop plugin...")
            codex_path = create_codex_plugin(args.output, version)
            print(f"  Created: {codex_path}")
        
        if args.type in ["claude", "both"]:
            print("Creating Claude Cowork plugin...")
            claude_path = create_claude_plugin(args.output, version)
            print(f"  Created: {claude_path}")
        
        if args.type == "both":
            print("Creating combined package...")
            combined_path = create_combined_package(args.output, version)
            print(f"  Created: {combined_path}")
        
        print("\nPackaging completed successfully!")
        print(f"\nTo install:")
        if args.type in ["codex", "both"]:
            print(f"  Codex: codex plugin install {args.output}/guixu-codex-plugin")
        if args.type in ["claude", "both"]:
            print(f"  Claude: claude plugin install {args.output}/guixu-claude-plugin")
    
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()