#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

# Install Rust if missing or too old (need >= 1.82)
install_rust() {
  if command -v rustup &>/dev/null; then
    echo "Updating Rust..."
    rustup update stable
  else
    echo "Installing Rust..."
    if curl -s --connect-timeout 3 https://sh.rustup.rs >/dev/null 2>&1; then
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    else
      # China mirror
      curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh -s -- -y
    fi
    . "$HOME/.cargo/env"
  fi
}

rust_ok() {
  command -v cargo &>/dev/null || return 1
  local ver
  ver=$(rustc --version | grep -oE '[0-9]+\.[0-9]+')
  [ "$(printf '%s\n' "1.82" "$ver" | sort -V | head -1)" = "1.82" ]
}

# Setup crates.io mirror if needed
setup_cargo_mirror() {
  local cfg="$HOME/.cargo/config.toml"
  [ -f "$cfg" ] && grep -q 'rsproxy' "$cfg" && return
  if ! curl -s --connect-timeout 3 https://crates.io >/dev/null 2>&1; then
    echo "Setting up crates.io China mirror..."
    mkdir -p "$HOME/.cargo"
    cat >> "$cfg" <<'EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/crates.io-index/"
[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
EOF
  fi
}

# Install system deps
install_deps() {
  if command -v apt-get &>/dev/null; then
    local pkgs=()
    command -v clang &>/dev/null || pkgs+=(libclang-dev)
    command -v pkg-config &>/dev/null || pkgs+=(pkg-config)
    [ ${#pkgs[@]} -gt 0 ] && sudo apt-get update && sudo apt-get install -y "${pkgs[@]}"
  fi
}

rust_ok || install_rust
rust_ok || { echo "Error: Rust install failed." >&2; exit 1; }

setup_cargo_mirror
install_deps

# Build
if [ ! -f target/release/data-node ] || [ "$(find crates -name '*.rs' -newer target/release/data-node 2>/dev/null | head -1)" ]; then
  echo "Building..."
  cargo build --release
fi

# Init if first run
[ -d ~/.data-node ] || ./target/release/data-node init

echo "Web UI  → http://localhost:$PORT"
echo "Demo UI → http://localhost:$PORT/demo"
exec ./target/release/data-node start
