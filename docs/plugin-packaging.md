<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Guixu Plugin Packaging and Installation

This document describes how to package and install Guixu as a plugin for Codex Desktop and Claude Cowork.

## Prerequisites

1. **Guixu CLI**: Ensure the `guixu` CLI is installed and in your PATH.
2. **Codex Desktop** (optional): For Codex plugin installation.
3. **Claude Code** (optional): For Claude plugin installation.

## Packaging

### Automated Packaging

Use the automated packaging script to create plugin packages:

```bash
# Package plugins for both Codex Desktop and Claude Cowork
make package

# Or use the script directly
./scripts/package-plugins.sh --output dist --clean

# Package only for Codex Desktop
make package-codex

# Package only for Claude Cowork
make package-claude
```

### Manual Packaging

If you prefer to package manually:

1. Run the packaging script:
   ```bash
   python3 scripts/package_plugins.py --output dist --clean
   ```

2. The script will create:
   - `dist/guixu-codex-plugin/`: Codex Desktop plugin
   - `dist/guixu-claude-plugin/`: Claude Cowork plugin
   - `dist/guixu-plugins/`: Combined package

### Plugin Structure

#### Codex Desktop Plugin
```
guixu-codex-plugin/
  .codex-plugin/
    plugin.json          # Plugin metadata
  skills/
    guixu/
      SKILL.md           # Skill definition
  .mcp.json              # MCP server configuration
  marketplace.json       # Marketplace metadata (optional)
  README.md              # Plugin documentation
```

#### Claude Cowork Plugin
```
guixu-claude-plugin/
  .claude-plugin/
    plugin.json          # Plugin metadata
  skills/
    guixu/
      SKILL.md           # Skill definition
  .mcp.json              # MCP server configuration
  marketplace.json       # Marketplace metadata (optional)
  README.md              # Plugin documentation
```

## Installation

### Automated Installation

Use the installation script:

```bash
# Install both plugins
make install-plugins

# Or use the script directly
./scripts/install-plugins.sh --both

# Install only Codex plugin
./scripts/install-plugins.sh --codex

# Install only Claude plugin
./scripts/install-plugins.sh --claude
```

### Manual Installation

#### Codex Desktop
```bash
codex plugin install dist/guixu-codex-plugin
```

#### Claude Cowork
```bash
claude plugin install dist/guixu-claude-plugin
```

## Testing

Run the test script to verify packaging:

```bash
./scripts/test-packaging.sh
```

This will:
1. Run the packaging script
2. Verify all required files are created
3. Validate JSON configuration files
4. Check directory structure

## Configuration

### MCP Server Configuration

The `.mcp.json` file configures the MCP server:

```json
{
  "mcpServers": {
    "guixu": {
      "command": "guixu",
      "args": ["mcp"],
      "description": "Guixu dataset discovery and acquisition MCP server"
    }
  }
}
```

### Plugin Metadata

#### Codex Desktop (`plugin.json`)
```json
{
  "name": "guixu",
  "version": "0.1.0",
  "description": "Dataset discovery, valuation, and acquisition for AI agents",
  "skills": "./skills/",
  "mcpServers": "./.mcp.json"
}
```

#### Claude Cowork (`plugin.json`)
```json
{
  "name": "guixu",
  "description": "Dataset discovery, valuation, and acquisition for AI agents",
  "version": "0.1.0",
  "author": {
    "name": "Guixu Project"
  }
}
```

## Troubleshooting

### Common Issues

1. **`guixu` command not found**
   - Ensure Guixu CLI is installed: `cargo install --path crates/node`
   - Add to PATH: `export PATH="$HOME/.cargo/bin:$PATH"`

2. **Plugin installation fails**
   - Check if the plugin directory exists
   - Verify JSON files are valid
   - Ensure you have write permissions

3. **MCP server fails to start**
   - Test MCP server manually: `guixu mcp`
   - Check for port conflicts
   - Verify Guixu binary is accessible

### Debugging

1. **Test MCP server**:
   ```bash
   guixu mcp --mode http
   curl http://localhost:3927/rpc
   ```

2. **Check plugin installation**:
   ```bash
   # Codex
   codex plugin list
   
   # Claude
   claude plugin list
   ```

3. **View logs**:
   ```bash
   # Check MCP server logs
   guixu mcp 2>&1 | tee mcp.log
   ```

## Distribution

### Creating a Release

1. Update version in `Cargo.toml`
2. Run packaging script:
   ```bash
   make package
   ```
3. Create distribution archive:
   ```bash
   tar -czf guixu-plugins-v0.1.0.tar.gz -C dist guixu-plugins
   ```

### Submitting to Marketplaces

#### Claude Marketplace
- Use Claude.ai or Console's in-app submission forms
- Ensure plugin meets quality and security standards

#### Codex Marketplace
- Create custom marketplace JSON
- Host on GitHub or other accessible location
- Users can install via: `codex plugin marketplace add <url>`

## Version History

- **v0.1.0**: Initial release
  - Dataset discovery, valuation, and acquisition
  - Support for Codex Desktop and Claude Cowork
  - MCP server with 5 tools

## Support

For issues or questions:
- GitHub: https://github.com/guixu-project/guixu
- Documentation: docs/guixu-plugin.md