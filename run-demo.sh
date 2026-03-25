#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

if ! command -v docker &>/dev/null; then
  echo "Error: docker not found. Install Docker first: https://docs.docker.com/get-docker/" >&2
  exit 1
fi

# Detect reachable Docker registry
REGISTRY=""
MIRRORS=(
  ""                                    # Docker Hub
  "mirror.iscas.ac.cn/"                 # 中科院
  "docker.m.daocloud.io/"              # DaoCloud
  "dockerhub.timeweb.cloud/"           # TimeWeb
)

for m in "${MIRRORS[@]}"; do
  host="${m%/}"
  [ -z "$host" ] && host="registry-1.docker.io"
  if curl -s --connect-timeout 3 "https://$host/v2/" >/dev/null 2>&1; then
    REGISTRY="$m"
    echo "Using registry: ${host}"
    break
  fi
done

if [ -z "$REGISTRY" ] && ! curl -s --connect-timeout 3 https://registry-1.docker.io/v2/ >/dev/null 2>&1; then
  echo "Error: no reachable Docker registry found. Configure a mirror manually." >&2
  exit 1
fi

docker build --build-arg REGISTRY="$REGISTRY" -t guixu .
echo "Demo UI → http://localhost:$PORT/demo"
exec docker run --rm -it -p "$PORT:3927" guixu
