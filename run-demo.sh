#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

# Build if needed
if [ ! -f target/release/data-node ] || [ "$(find crates -name '*.rs' -newer target/release/data-node 2>/dev/null | head -1)" ]; then
  echo "Building..."
  cargo build --release
fi

# Init if first run
if [ ! -d ~/.data-node ]; then
  ./target/release/data-node init
fi

echo "Starting Guixu..."
echo "  Web UI → http://localhost:3927"
echo "  Demo UI → http://localhost:3927/demo"
exec ./target/release/data-node start
