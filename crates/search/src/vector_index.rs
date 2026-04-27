// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use qdrant_client::client::QdrantClient;
use qdrant_client::qdrant::{CreateCollection, Distance, PointStruct, VectorParams, VectorsConfig};
use std::sync::Arc;

/// Local vector index for semantic search over dataset metadata.
/// Uses Qdrant in embedded/standalone mode.
pub struct VectorIndex {
    client: Arc<QdrantClient>,
    collection_name: String,
}

impl VectorIndex {
    /// Initialize the vector index (create collection if not exists).
    pub async fn init() -> Result<Self> {
        let client = QdrantClient::from_url("http://127.0.0.1:6334").build()?;

        // Create "datasets" collection with 384-dim vectors (all-MiniLM-L6-v2 output dim)
        let collection_name = "datasets";
        let collection_exists = client.collection_exists(collection_name).await.unwrap_or(false);

        if !collection_exists {
            client
                .create_collection(
                    &CreateCollection {
                        collection_name: collection_name.to_string(),
                        vectors_config: Some(VectorsConfig {
                            config: Some(qdrant_client::qdrant::vectors_config::Config::Params(
                                VectorParams {
                                    size: 384,
                                    distance: Distance::Cosine.into(),
                                    ..Default::default()
                                },
                            )),
                        }),
                        ..Default::default()
                    },
                )
                .await?;
        }

        Ok(Self {
            client: Arc::new(client),
            collection_name: collection_name.to_string(),
        })
    }

    /// Index a dataset's metadata (embed title+description+tags → vector).
    pub async fn upsert(&self, metadata: &DatasetMetadata) -> Result<()> {
        // 1. Concatenate title + description + tags
        let text = [
            metadata.title.as_str(),
            metadata.description.as_deref().unwrap_or(""),
            &metadata.tags.join(" "),
        ]
        .join(" ");

        // 2. Generate embedding (384-dim vector for all-MiniLM-L6-v2)
        let embedding = self.embed_text(&text);

        // 3. Upsert vector + payload into Qdrant
        let point = PointStruct {
            id: Some(metadata.cid.0.clone().into()),
            vectors: Some(embedding),
            payload: Some(
                serde_json::json!({
                    "cid": metadata.cid.0,
                    "title": metadata.title,
                    "description": metadata.description,
                    "tags": metadata.tags,
                })
                .try_into()?,
            ),
        };

        self.client
            .upsert_points(&self.collection_name, vec![point], None)
            .await?;

        Ok(())
    }

    /// Generate a 384-dimensional embedding for text.
    /// Note: This is a placeholder that generates random vectors.
    /// In production, integrate with all-MiniLM-L6-v2 ONNX model via ort crate.
    fn embed_text(&self, text: &str) -> Vec<f32> {
        // Placeholder: use simple hash-based pseudo-embedding for development
        // TODO(milestone-2): Replace with actual ONNX model inference
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate deterministic pseudo-random 384-dim vector from hash
        let mut rng = rand::rngs::StdRng::seed_from_u64(hash);
        use rand::Rng;
        (0..384).map(|_| rng.gen::<f32>()).collect()
    }

    /// Semantic search: embed query → nearest neighbors.
    pub async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<DatasetCid>> {
        let search_result = self
            .client
            .search_points(&qdrant_client::qdrant::SearchPoints {
                collection_name: self.collection_name.clone(),
                vector: query_embedding.to_vec(),
                limit: limit as u64,
                ..Default::default()
            })
            .await?;

        let cids: Vec<DatasetCid> = search_result
            .result
            .iter()
            .filter_map(|point| {
                point
                    .payload
                    .get("cid")
                    .and_then(|v| v.as_str())
                    .map(|s| DatasetCid(s.to_string()))
            })
            .collect();

        Ok(cids)
    }
}