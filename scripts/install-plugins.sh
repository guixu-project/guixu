#!/bin/bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

# Install Guixu plugins for Codex Desktop and Claude Cowork

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Default values
OUTPUT_DIR="$PROJECT_ROOT/dist"
INSTALL_CODEX=false
INSTALL_CLAUDE=false
INSTALL_BOTH=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    -o|--output)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --codex)
      INSTALL_CODEX=true
      shift
      ;;
    --claude)
      INSTALL_CLAUDE=true
      shift
      ;;
    --both)
      INSTALL_BOTH=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Install Guixu plugins for Codex Desktop and Claude Cowork"
      echo ""
      echo "Options:"
      echo "  -o, --output DIR    Plugin directory (default: dist)"
      echo "  --codex             Install Codex Desktop plugin"
      echo "  --claude            Install Claude Cowork plugin"
      echo "  --both              Install both plugins (default)"
      echo "  -h, --help          Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Default to both if none specified
if [[ "$INSTALL_CODEX" == "false" && "$INSTALL_CLAUDE" == "false" && "$INSTALL_BOTH" == "false" ]]; then
  INSTALL_BOTH=true
fi

# Package plugins if not already packaged
if [[ ! -d "$OUTPUT_DIR" ]]; then
  echo "Packaging plugins..."
  $SCRIPT_DIR/package-plugins.sh --output "$OUTPUT_DIR" --clean
fi

# Install plugins
if [[ "$INSTALL_BOTH" == "true" || "$INSTALL_CODEX" == "true" ]]; then
  echo "Installing Codex Desktop plugin..."
  if command -v codex &> /dev/null; then
    codex plugin install "$OUTPUT_DIR/guixu-codex-plugin"
    echo "✓ Codex Desktop plugin installed"
  else
    echo "⚠ codex command not found. Please install Codex Desktop first."
    echo "  Plugin package is at: $OUTPUT_DIR/guixu-codex-plugin"
  fi
fi

if [[ "$INSTALL_BOTH" == "true" || "$INSTALL_CLAUDE" == "true" ]]; then
  echo "Installing Claude Cowork plugin..."
  if command -v claude &> /dev/null; then
    claude plugin install "$OUTPUT_DIR/guixu-claude-plugin"
    echo "✓ Claude Cowork plugin installed"
  else
    echo "⚠ claude command not found. Please install Claude Code first."
    echo "  Plugin package is at: $OUTPUT_DIR/guixu-claude-plugin"
  fi
fi

echo ""
echo "Installation completed!"
echo ""
echo "To use the plugins:"
echo "  1. Ensure 'guixu' CLI is in your PATH"
echo "  2. Start Codex Desktop or Claude Cowork"
echo "  3. The plugins will be available automatically"