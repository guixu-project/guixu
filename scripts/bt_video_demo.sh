#!/usr/bin/env bash
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

# bt_video_demo.sh — Search BT network for video datasets, evaluate, and download
set -euo pipefail

API="https://bitsearch.to/api/v1"
QUERY="${1:-machine learning video dataset}"
LIMIT=5

echo "🔍 Searching BitTorrent network for: \"$QUERY\""
echo "   (via bitsearch.to API, category=1 subCategory=2 = Other/Video)"
echo ""

# Search for video content (category=1, subCategory=2)
RESULTS=$(curl -s "${API}/search?q=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$QUERY'))")&category=1&subCategory=2&sort=seeders&limit=${LIMIT}")

SUCCESS=$(echo "$RESULTS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success', False))" 2>/dev/null || echo "False")

if [ "$SUCCESS" != "True" ]; then
    echo "❌ Search failed or returned no results"
    echo "$RESULTS" | python3 -m json.tool 2>/dev/null || echo "$RESULTS"
    exit 1
fi

echo "📋 Results:"
echo ""
echo "$RESULTS" | python3 -c "
import sys, json

data = json.load(sys.stdin)
results = data.get('results', [])
pagination = data.get('pagination', {})

print(f'   Found {pagination.get(\"total\", len(results))} results, showing top {len(results)}:')
print()

for i, r in enumerate(results, 1):
    title = r.get('title', 'N/A')[:70]
    infohash = r.get('infohash', 'N/A')
    size_mb = r.get('size', 0) / (1024*1024)
    seeders = r.get('seeders', 0)
    leechers = r.get('leechers', 0)
    category = r.get('category', 'N/A')
    created = r.get('createdAt', 'N/A')[:10]

    print(f'   [{i}] {title}')
    print(f'       infohash: {infohash}')
    print(f'       size: {size_mb:.1f} MB | seeders: {seeders} | leechers: {leechers}')
    print(f'       category: {category} | date: {created}')
    print()
"

echo "---"
echo ""
echo "💡 To download via Guixu MCP, send this JSON-RPC request:"
echo ""

FIRST_HASH=$(echo "$RESULTS" | python3 -c "
import sys, json
data = json.load(sys.stdin)
results = data.get('results', [])
if results:
    print(results[0].get('infohash', ''))
" 2>/dev/null)

if [ -n "$FIRST_HASH" ]; then
    cat <<EOF
   {
     "jsonrpc": "2.0",
     "id": 1,
     "method": "tools/call",
     "params": {
       "name": "dataset_bt_download",
       "arguments": { "info_hash": "$FIRST_HASH" }
     }
   }
EOF
fi

echo ""
echo "🎬 To evaluate a video dataset for your task:"
echo ""
cat <<EOF
   {
     "jsonrpc": "2.0",
     "id": 2,
     "method": "tools/call",
     "params": {
       "name": "dataset_evaluate",
       "arguments": {
         "cid": "$FIRST_HASH",
         "task_description": "video classification training data",
         "task_type": "video_classification"
       }
     }
   }
EOF
