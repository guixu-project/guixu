use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;

/// Local vector index for semantic search over dataset metadata.
/// Uses Qdrant in embedded mode.
pub struct VectorIndex;

impl VectorIndex {
    /// Initialize the vector index (create collection if not exists).
    pub async fn init() -> Result<Self> {
        // TODO(milestone-2):
        // 1. Start Qdrant embedded
        // 2. Create "datasets" collection with 384-dim vectors
        Ok(Self)
    }

    /// Index a dataset's metadata (embed title+description+tags → vector).
    pub async fn upsert(&self, _metadata: &DatasetMetadata) -> Result<()> {
        // TODO(milestone-2):
        // 1. Concatenate title + description + tags
        // 2. Run through all-MiniLM-L6-v2 (ONNX)
        // 3. Upsert vector + payload into Qdrant
        Ok(())
    }

    /// Semantic search: embed query → nearest neighbors.
    pub async fn search(&self, _query_embedding: &[f32], _limit: usize) -> Result<Vec<DatasetCid>> {
        // TODO(milestone-2): Qdrant search → return CIDs
        Ok(vec![])
    }
}
