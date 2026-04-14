#!/bin/bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

set -e

echo "=== Guixu Setup ==="

# Configure git hooks
git config core.hooksPath .githooks
echo "[OK] Git hooks configured: $(git config --get core.hooksPath)"

# Fetch Rust dependencies
cargo fetch --quiet 2>/dev/null || true

echo "=== Setup Complete ==="
