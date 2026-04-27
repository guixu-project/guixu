// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use qdrant_client::qdrant::{Distance, PointStruct, SearchPointsBuilder, UpsertPointsBuilder};
use qdrant_client::Qdrant;
use rand::SeedableRng;
use std::sync::Arc;

/// Local vector index for semantic search over dataset metadata.
/// Uses Qdrant in embedded/standalone mode.
pub struct VectorIndex {
    client: Arc<Qdrant>,
    collection_name: String,
}

impl VectorIndex {
    /// Initialize the vector index (create collection if not exists).
    pub async fn init() -> Result<Self> {
        let client = Qdrant::from_url("http://127.0.0.1:6334").build()?;

        let collection_name = "datasets";
        let collection_exists = client
            .collection_exists(collection_name)
            .await
            .unwrap_or(false);

        if !collection_exists {
            client
                .create_collection(
                    qdrant_client::qdrant::CreateCollectionBuilder::new(collection_name)
                        .vectors_config(qdrant_client::qdrant::VectorParamsBuilder::new(
                            384,
                            Distance::Cosine,
                        )),
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
        let text = [
            metadata.title.as_str(),
            metadata.description.as_deref().unwrap_or(""),
            &metadata.tags.join(" "),
        ]
        .join(" ");

        let embedding = self.embed_text(&text);

        let mut payload = qdrant_client::Payload::with_capacity(4);
        payload.insert("cid", metadata.cid.0.clone());
        payload.insert("title", metadata.title.clone());
        payload.insert(
            "description",
            metadata.description.clone().unwrap_or_default(),
        );
        payload.insert("tags", metadata.tags.join(","));

        let point = PointStruct::new(metadata.cid.0.clone(), embedding, payload);

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection_name, vec![point]))
            .await?;

        Ok(())
    }

    /// Generate a 384-dimensional embedding for text.
    /// Note: This is a placeholder that generates random vectors.
    /// In production, integrate with all-MiniLM-L6-v2 ONNX model via ort crate.
    fn embed_text(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        let mut rng = rand::rngs::StdRng::seed_from_u64(hash);
        use rand::Rng;
        (0..384).map(|_| rng.gen::<f32>()).collect()
    }

    /// Semantic search: embed query → nearest neighbors.
    pub async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<DatasetCid>> {
        let search_result = self
            .client
            .search_points(SearchPointsBuilder::new(
                &self.collection_name,
                query_embedding.to_vec(),
                limit as u64,
            ))
            .await?;

        let cids: Vec<DatasetCid> = search_result
            .result
            .iter()
            .filter_map(|point| {
                point.payload.get("cid").and_then(|v| {
                    if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = &v.kind {
                        Some(DatasetCid(s.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        Ok(cids)
    }
}
