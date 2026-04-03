<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# 1. Compilation (Slow at first, fast in subsequent increments) cd ~/Codebase/guixu && cargo build --release

# 2. Initialize the node (if it hasn't been init yet)./target/release/data-node init

# 3. Full debug start RUST_LOG=debug./target/release/data-node start


cd demo-ui && python3 -m http.server 8090