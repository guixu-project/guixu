#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

if ! command -v docker &>/dev/null; then
  echo "Error: docker not found. Install Docker first: https://docs.docker.com/get-docker/" >&2
  exit 1
fi

docker build -t guixu .
echo "Demo UI → http://localhost:$PORT/demo"
exec docker run --rm -it -p "$PORT:3927" guixu
