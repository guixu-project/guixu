// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

fn main() {
    // Tell cargo to re-run if any demo-ui file changes (used by include_str!).
    println!("cargo::rerun-if-changed=../../../demo-ui/");
}
