#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

if command -v cargo &>/dev/null; then
  echo "Detected Rust toolchain, building natively..."
  if [ ! -f target/release/data-node ] || [ "$(find crates -name '*.rs' -newer target/release/data-node 2>/dev/null | head -1)" ]; then
    cargo build --release
  fi
  [ -d ~/.data-node ] || ./target/release/data-node init
  echo "Web UI  → http://localhost:$PORT"
  echo "Demo UI → http://localhost:$PORT/demo"
  exec ./target/release/data-node start
elif command -v docker &>/dev/null; then
  echo "No Rust toolchain found, using Docker..."
  docker build -t guixu .
  exec docker run --rm -it -p "$PORT:3927" guixu
else
  echo "Error: neither cargo nor docker found. Install one of them first." >&2
  exit 1
fi
