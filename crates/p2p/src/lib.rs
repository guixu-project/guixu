// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

pub mod dht;
pub mod gossip;
pub mod network;
pub mod publish;
pub mod torrent;
pub mod watchdir;

// Re-export from data-storage for backward compatibility
pub use data_storage::feedback_store;
pub use data_storage::metadata_store as storage;
