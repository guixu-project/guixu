# 1. Compilation (Slow at first, fast in subsequent increments) cd ~/Codebase/guixu && cargo build --release

# 2. Initialize the node (if it hasn't been init yet)./target/release/data-node init

# 3. Full debug start RUST_LOG=debug./target/release/data-node start