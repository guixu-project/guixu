// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use qdrant_client::qdrant::{Distance, PointStruct, SearchPointsBuilder, UpsertPointsBuilder};
use qdrant_client::Qdrant;
use std::sync::Arc;

pub struct VectorIndex {
    client: Arc<Qdrant>,
    collection_name: String,
}

impl VectorIndex {
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

    #[cfg(not(feature = "real-embeddings"))]
    fn embed_text(&self, text: &str) -> Vec<f32> {
        use rand::Rng;
        use rand::SeedableRng;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        let mut rng = rand::rngs::StdRng::seed_from_u64(hash);
        (0..384).map(|_| rng.gen::<f32>()).collect()
    }

    #[cfg(feature = "real-embeddings")]
    fn embed_text(&self, text: &str) -> Vec<f32> {
        use ndarray::Array2;
        use ort::session::Session;
        use tokenizers::Tokenizer;

        let model_path = std::env::var("EMBEDDING_MODEL_PATH")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".to_string());
        let tokenizer_path = std::env::var("TOKENIZER_PATH")
            .unwrap_or_else(|_| "models/sentencepiece.bpe.model".to_string());

        let mut session = Session::builder()
            .expect("failed to create session builder")
            .commit_from_file(&model_path)
            .expect("failed to load embedding model");

        let tokenizer = Tokenizer::from_file(&tokenizer_path).expect("failed to load tokenizer");

        let encoding = tokenizer.encode(text, true).expect("tokenization failed");

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();

        let seq_len = input_ids.len();
        let batch_size = 1;

        let input_array = Array2::from_shape_vec((batch_size, seq_len), input_ids)
            .expect("failed to create input array");
        let mask_array = Array2::from_shape_vec((batch_size, seq_len), attention_mask)
            .expect("failed to create mask array");

        let input_tensor = ort::value::Tensor::from_array(input_array.into_owned())
            .expect("failed to create input tensor");
        let mask_tensor = ort::value::Tensor::from_array(mask_array.into_owned())
            .expect("failed to create mask tensor");

        let outputs = session
            .run(ort::inputs![input_tensor, mask_tensor])
            .expect("model inference failed");

        let output_array = outputs[0]
            .try_extract_array::<f32>()
            .expect("failed to extract output tensor");
        let seq_len = output_array.shape()[1];
        let mut embedding = vec![0.0f32; 384];
        for i in 0..384 {
            let mut sum = 0.0f32;
            for j in 0..seq_len {
                sum += output_array[[0, j, i]];
            }
            embedding[i] = sum / seq_len as f32;
        }
        Self::normalize(&mut embedding);
        embedding
    }

    #[cfg(feature = "real-embeddings")]
    fn normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

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
