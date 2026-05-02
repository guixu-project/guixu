#!/bin/bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

# Test script for plugin packaging

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Testing plugin packaging..."

# Test 1: Run packaging script
echo "Test 1: Running packaging script..."
$SCRIPT_DIR/package-plugins.sh --output /tmp/test-guixu-plugins --clean

# Test 2: Check if Codex plugin was created
echo "Test 2: Checking Codex plugin..."
if [ -d "/tmp/test-guixu-plugins/guixu-codex-plugin" ]; then
    echo "  ✓ Codex plugin directory exists"
else
    echo "  ✗ Codex plugin directory missing"
    exit 1
fi

# Test 3: Check if Claude plugin was created
echo "Test 3: Checking Claude plugin..."
if [ -d "/tmp/test-guixu-plugins/guixu-claude-plugin" ]; then
    echo "  ✓ Claude plugin directory exists"
else
    echo "  ✗ Claude plugin directory missing"
    exit 1
fi

# Test 4: Check plugin.json files
echo "Test 4: Checking plugin.json files..."
if [ -f "/tmp/test-guixu-plugins/guixu-codex-plugin/.codex-plugin/plugin.json" ]; then
    echo "  ✓ Codex plugin.json exists"
else
    echo "  ✗ Codex plugin.json missing"
    exit 1
fi

if [ -f "/tmp/test-guixu-plugins/guixu-claude-plugin/.claude-plugin/plugin.json" ]; then
    echo "  ✓ Claude plugin.json exists"
else
    echo "  ✗ Claude plugin.json missing"
    exit 1
fi

# Test 5: Check MCP configuration
echo "Test 5: Checking MCP configuration..."
if [ -f "/tmp/test-guixu-plugins/guixu-codex-plugin/.mcp.json" ]; then
    echo "  ✓ Codex .mcp.json exists"
else
    echo "  ✗ Codex .mcp.json missing"
    exit 1
fi

if [ -f "/tmp/test-guixu-plugins/guixu-claude-plugin/.mcp.json" ]; then
    echo "  ✓ Claude .mcp.json exists"
else
    echo "  ✗ Claude .mcp.json missing"
    exit 1
fi

# Test 6: Check skills directory
echo "Test 6: Checking skills directory..."
if [ -d "/tmp/test-guixu-plugins/guixu-codex-plugin/skills/guixu" ]; then
    echo "  ✓ Codex skills directory exists"
else
    echo "  ✗ Codex skills directory missing"
    exit 1
fi

if [ -d "/tmp/test-guixu-plugins/guixu-claude-plugin/skills/guixu" ]; then
    echo "  ✓ Claude skills directory exists"
else
    echo "  ✗ Claude skills directory missing"
    exit 1
fi

# Test 7: Validate JSON files
echo "Test 7: Validating JSON files..."
if python3 -m json.tool /tmp/test-guixu-plugins/guixu-codex-plugin/.codex-plugin/plugin.json > /dev/null; then
    echo "  ✓ Codex plugin.json is valid JSON"
else
    echo "  ✗ Codex plugin.json is invalid JSON"
    exit 1
fi

if python3 -m json.tool /tmp/test-guixu-plugins/guixu-claude-plugin/.claude-plugin/plugin.json > /dev/null; then
    echo "  ✓ Claude plugin.json is valid JSON"
else
    echo "  ✗ Claude plugin.json is invalid JSON"
    exit 1
fi

# Test 8: Check combined package
echo "Test 8: Checking combined package..."
if [ -d "/tmp/test-guixu-plugins/guixu-plugins" ]; then
    echo "  ✓ Combined package exists"
else
    echo "  ✗ Combined package missing"
    exit 1
fi

echo ""
echo "All tests passed! ✓"
echo ""
echo "Plugin packages created in: /tmp/test-guixu-plugins"
echo "To install:"
echo "  Codex: codex plugin install /tmp/test-guixu-plugins/guixu-codex-plugin"
echo "  Claude: claude plugin install /tmp/test-guixu-plugins/guixu-claude-plugin"