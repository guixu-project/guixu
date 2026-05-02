#!/bin/bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

# Example: Package and install Guixu plugins

echo "=== Guixu Plugin Packaging Example ==="
echo ""

# Step 1: Package plugins
echo "Step 1: Packaging plugins..."
./scripts/package-plugins.sh --output example-dist --clean

echo ""
echo "Step 2: Listing created files..."
find example-dist -type f | sort

echo ""
echo "Step 3: Installation instructions..."
echo ""
echo "To install for Codex Desktop:"
echo "  codex plugin install example-dist/guixu-codex-plugin"
echo ""
echo "To install for Claude Cowork:"
echo "  claude plugin install example-dist/guixu-claude-plugin"
echo ""
echo "Or use the automated installer:"
echo "  ./scripts/install-plugins.sh --output example-dist --both"
echo ""
echo "=== Example completed ==="