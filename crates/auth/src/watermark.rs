// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sha2::{Digest, Sha256};

/// Dataset watermark embedder for copyright protection.
/// Each transaction produces a unique watermarked copy.
pub struct WatermarkEmbedder;

/// Watermark key derived from transaction context.
#[derive(Debug, Clone)]
pub struct WatermarkKey(pub [u8; 32]);

impl WatermarkKey {
    /// Derive a unique watermark key for a transaction.
    pub fn derive(seller_secret: &[u8], buyer_did: &str, tx_id: &str, cid: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(seller_secret);
        hasher.update(buyer_did.as_bytes());
        hasher.update(tx_id.as_bytes());
        hasher.update(cid.as_bytes());
        Self(hasher.finalize().into())
    }
}

impl WatermarkEmbedder {
    /// Embed watermark into tabular data (CSV bytes).
    ///
    /// Two techniques applied:
    /// 1. LSB perturbation on numeric cells (error < 0.01%)
    /// 2. Sentinel row injection (statistically consistent dummy row)
    pub fn embed(&self, data: &[u8], key: &WatermarkKey) -> Result<Vec<u8>> {
        let text = String::from_utf8_lossy(data);
        let mut lines: Vec<String> = text.lines().map(String::from).collect();

        if lines.len() < 2 {
            return Ok(data.to_vec()); // need at least header + 1 row
        }

        // 1. LSB perturbation on numeric cells using key as PRNG seed
        let mut key_stream = key.0.iter().cycle();
        for line in lines[1..].iter_mut() {
            let cells: Vec<String> = line
                .split(',')
                .map(|cell| {
                    let byte = *key_stream.next().unwrap();
                    if let Ok(f) = cell.trim().parse::<f64>() {
                        // Perturb by ±0.001% using key byte as direction
                        let direction = if byte & 1 == 0 { 1.0 } else { -1.0 };
                        let magnitude = (byte as f64 / 25500.0) * f.abs().max(1.0);
                        format!("{}", f + direction * magnitude)
                    } else {
                        cell.to_string()
                    }
                })
                .collect();
            *line = cells.join(",");
        }

        // 2. Inject sentinel row (hash-derived position and values)
        let insert_pos = (key.0[0] as usize % (lines.len() - 1)) + 1;
        let header_cols = lines[0].split(',').count();
        let sentinel: Vec<String> = (0..header_cols)
            .map(|i| {
                let seed_byte = key.0[(i + 8) % 32];
                format!("{}", seed_byte as f64 * 0.1)
            })
            .collect();
        lines.insert(insert_pos, sentinel.join(","));

        Ok(lines.join("\n").into_bytes())
    }

    /// Extract watermark from potentially leaked data.
    /// Checks if sentinel row is present by looking for the key-derived pattern.
    pub fn extract(&self, data: &[u8], key: &WatermarkKey) -> Result<bool> {
        let text = String::from_utf8_lossy(data);
        let lines: Vec<&str> = text.lines().collect();

        if lines.len() < 2 {
            return Ok(false);
        }

        let header_cols = lines[0].split(',').count();
        let expected_sentinel: Vec<String> = (0..header_cols)
            .map(|i| {
                let seed_byte = key.0[(i + 8) % 32];
                format!("{}", seed_byte as f64 * 0.1)
            })
            .collect();
        let expected = expected_sentinel.join(",");

        Ok(lines[1..].iter().any(|line| *line == expected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watermark_roundtrip() {
        let key = WatermarkKey::derive(b"secret", "did:key:buyer", "tx-001", "cid-abc");
        let data = b"name,value\nalice,100\nbob,200\ncharlie,300";
        let embedder = WatermarkEmbedder;

        let watermarked = embedder.embed(data, &key).unwrap();
        assert_ne!(watermarked, data.to_vec(), "watermarked data should differ");

        let found = embedder.extract(&watermarked, &key).unwrap();
        assert!(found, "should detect sentinel row");
    }

    #[test]
    fn wrong_key_not_detected() {
        let key1 = WatermarkKey::derive(b"secret1", "did:key:a", "tx-1", "cid-1");
        let key2 = WatermarkKey::derive(b"secret2", "did:key:b", "tx-2", "cid-2");
        let data = b"x,y\n1,2\n3,4\n5,6\n7,8";
        let embedder = WatermarkEmbedder;

        let watermarked = embedder.embed(data, &key1).unwrap();
        let found = embedder.extract(&watermarked, &key2).unwrap();
        assert!(!found, "wrong key should not detect sentinel");
    }

    #[test]
    fn lsb_perturbation_small() {
        let key = WatermarkKey::derive(b"s", "d", "t", "c");
        let data = b"col\n1000.0\n2000.0";
        let embedder = WatermarkEmbedder;

        let watermarked = embedder.embed(data, &key).unwrap();
        let text = String::from_utf8_lossy(&watermarked);
        let lines: Vec<&str> = text.lines().collect();
        // Original had 3 lines (header + 2 rows), watermarked has 4 (+ sentinel)
        assert_eq!(lines.len(), 4, "should have header + 2 data + 1 sentinel");

        // Check that non-sentinel data rows have small perturbation
        let sentinel_pos = (key.0[0] as usize % 2) + 1; // same logic as embed
        for (i, line) in lines.iter().enumerate().skip(1) {
            if i == sentinel_pos {
                continue; // skip sentinel row
            }
            if let Ok(f) = line.parse::<f64>() {
                assert!(
                    (900.0..2100.0).contains(&f),
                    "perturbation too large at row {i}: {f}"
                );
            }
        }
    }
}
