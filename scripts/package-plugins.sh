#!/bin/bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

# Package Guixu plugins for Codex Desktop and Claude Cowork

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Default values
OUTPUT_DIR="$PROJECT_ROOT/dist"
VERSION=""
PLUGIN_TYPE="both"
CLEAN=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    -o|--output)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -v|--version)
      VERSION="$2"
      shift 2
      ;;
    -t|--type)
      PLUGIN_TYPE="$2"
      shift 2
      ;;
    -c|--clean)
      CLEAN=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Package Guixu plugins for Codex Desktop and Claude Cowork"
      echo ""
      echo "Options:"
      echo "  -o, --output DIR    Output directory (default: dist)"
      echo "  -v, --version VER   Plugin version (default: from Cargo.toml)"
      echo "  -t, --type TYPE     Plugin type: codex, claude, or both (default: both)"
      echo "  -c, --clean         Clean output directory before packaging"
      echo "  -h, --help          Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Build command
CMD="python3 $SCRIPT_DIR/package_plugins.py --output $OUTPUT_DIR --type $PLUGIN_TYPE"

if [[ -n "$VERSION" ]]; then
  CMD="$CMD --version $VERSION"
fi

if [[ "$CLEAN" == "true" ]]; then
  CMD="$CMD --clean"
fi

# Run packaging script
echo "Running: $CMD"
exec $CMD