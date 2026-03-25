#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

if ! command -v docker &>/dev/null; then
  echo "Error: docker not found. Install Docker first: https://docs.docker.com/get-docker/" >&2
  exit 1
fi

# Detect if Docker Hub is reachable
if curl -s --connect-timeout 3 https://registry-1.docker.io/v2/ >/dev/null 2>&1; then
  echo "Docker Hub reachable, using default registry."
  REGISTRY=""
else
  echo "Docker Hub unreachable, using China mirror..."
  REGISTRY="docker.zju.edu.cn/"
fi

docker build --build-arg REGISTRY="$REGISTRY" -t guixu .
echo "Demo UI → http://localhost:$PORT/demo"
exec docker run --rm -it -p "$PORT:3927" guixu
