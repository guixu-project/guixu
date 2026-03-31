#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY_PATH="$ROOT_DIR/target/release/data-node"
CODEX_DIR="${CODEX_HOME_DIR:-${HOME}/.codex}"
CONFIG_PATH="${CODEX_DIR}/config.toml"
TMP_PATH="${CONFIG_PATH}.tmp.$$"
AGENTS_PATH="$ROOT_DIR/AGENTS.md"
AGENTS_TMP_PATH="${AGENTS_PATH}.tmp.$$"
AGENTS_BEGIN="<!-- BEGIN GUIXU MCP -->"
AGENTS_END="<!-- END GUIXU MCP -->"

ensure_rust_env() {
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  elif [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

need_build() {
  [ ! -x "$BINARY_PATH" ] && return 0
  find "$ROOT_DIR/crates" -name '*.rs' -newer "$BINARY_PATH" -print -quit | grep -q .
}

toml_escape() {
  local value="$1"
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  printf '%s' "$value"
}

update_agents_md() {
  local had_agents=0
  if [ -f "$AGENTS_PATH" ]; then
    had_agents=1
    awk -v begin="$AGENTS_BEGIN" -v end="$AGENTS_END" '
      BEGIN {
        skip = 0;
      }
      $0 == begin {
        skip = 1;
        next;
      }
      skip && $0 == end {
        skip = 0;
        next;
      }
      !skip {
        print;
      }
    ' "$AGENTS_PATH" > "$AGENTS_TMP_PATH"
  else
    : > "$AGENTS_TMP_PATH"
  fi

  if [ "$had_agents" -eq 0 ]; then
    cat > "$AGENTS_TMP_PATH" <<EOF
# AGENTS.md

EOF
  elif [ -s "$AGENTS_TMP_PATH" ]; then
    printf '\n' >> "$AGENTS_TMP_PATH"
  fi

  cat >> "$AGENTS_TMP_PATH" <<EOF
$AGENTS_BEGIN
Use the Guixu MCP server by default for dataset acquisition workflows.

A request does not need to explicitly mention datasets. If the task implies a need for external training data, labeled examples, benchmark data, or candidate dataset selection, treat it as a Guixu MCP workflow trigger.

This includes requests to:
- train, build, write, improve, or evaluate a model, classifier, detector, segmenter, retriever, or ranking system when the task depends on acquiring or selecting data
- parse a natural-language data request
- search for datasets
- evaluate dataset suitability or value
- purchase a dataset
- record post-use dataset feedback

Before writing model code or proposing a training implementation, first determine whether the task requires dataset acquisition or dataset evaluation through Guixu MCP. If it does, use Guixu MCP first.

When relevant, use these tools in order:
1. intent_parse
2. dataset_search
3. dataset_evaluate
4. dataset_purchase
5. dataset_feedback

Do not use Guixu MCP when the task is purely about local code changes, refactoring, formatting, UI polish, or debugging unrelated to data selection or data procurement, unless the user explicitly asks to use MCP.
$AGENTS_END
EOF

  mv "$AGENTS_TMP_PATH" "$AGENTS_PATH"
}

ensure_rust_env

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is not available in PATH." >&2
  exit 1
fi

if need_build; then
  echo "Building data-node release binary..."
  cargo build --release -p data-node
fi

if [ ! -d "$HOME/.data-node" ]; then
  echo "Initializing local data-node state..."
  "$BINARY_PATH" init
fi

mkdir -p "$CODEX_DIR"

if [ -f "$CONFIG_PATH" ]; then
  awk '
    BEGIN {
      skip = 0;
    }
    /^\[mcp_servers\.guixu\][[:space:]]*$/ {
      skip = 1;
      next;
    }
    skip && /^\[/ {
      skip = 0;
    }
    !skip {
      print;
    }
  ' "$CONFIG_PATH" > "$TMP_PATH"
else
  : > "$TMP_PATH"
fi

if [ -s "$TMP_PATH" ]; then
  printf '\n' >> "$TMP_PATH"
fi

ESCAPED_BINARY_PATH="$(toml_escape "$BINARY_PATH")"
cat >> "$TMP_PATH" <<EOF
[mcp_servers.guixu]
command = "$ESCAPED_BINARY_PATH"
args = ["mcp", "--mode", "codex"]
EOF

mv "$TMP_PATH" "$CONFIG_PATH"
update_agents_md

echo "✅ Codex MCP configured"
echo "   Config:   $CONFIG_PATH"
echo "   Command:  $BINARY_PATH"
echo "   Args:     mcp --mode codex"
echo "   Agents:   $AGENTS_PATH"
echo
echo "Next step: restart Codex or open a new Codex session."
