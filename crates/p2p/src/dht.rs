use anyhow::Result;
use data_auth::privacy::{safe_tags_for_dht, PrivacyConfig};
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use tracing::debug;

use crate::network::NetworkHandle;

/// DHT index operations — wraps NetworkHandle's DHT with typed metadata API.
pub struct DhtIndex {
    net: NetworkHandle,
    privacy_config: PrivacyConfig,
}

impl DhtIndex {
    pub fn new(net: NetworkHandle) -> Self {
        Self {
            net,
            privacy_config: PrivacyConfig::default(),
        }
    }

    pub fn with_privacy(net: NetworkHandle, privacy_config: PrivacyConfig) -> Self {
        Self {
            net,
            privacy_config,
        }
    }

    pub fn handle(&self) -> &NetworkHandle {
        &self.net
    }

    /// Store dataset metadata in DHT, keyed by CID.
    /// Tags are filtered through privacy config before indexing.
    pub async fn put_metadata(&self, metadata: &DatasetMetadata) -> Result<()> {
        let key = format!("meta:{}", metadata.cid.0).into_bytes();
        let value = serde_json::to_vec(metadata)?;
        debug!(cid = %metadata.cid.0, "DHT PUT metadata");
        self.net.dht_put(key.clone(), value).await?;

        // Filter tags through privacy config before DHT indexing
        let safe_tags = safe_tags_for_dht(&metadata.tags, &self.privacy_config);
        for tag in &safe_tags {
            let tag_key = format!("tag:{}:{}", tag.to_lowercase(), metadata.cid.0).into_bytes();
            let cid_bytes = metadata.cid.0.as_bytes().to_vec();
            self.net.dht_put(tag_key, cid_bytes).await?;
        }
        Ok(())
    }

    /// Lookup metadata by CID from DHT.
    pub async fn get_metadata(&self, cid: &DatasetCid) -> Result<Option<DatasetMetadata>> {
        let key = format!("meta:{}", cid.0).into_bytes();
        match self.net.dht_get(key).await? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Broadcast metadata via GossipSub.
    pub async fn broadcast_metadata(&self, metadata: &DatasetMetadata) -> Result<()> {
        let data = serde_json::to_vec(metadata)?;
        self.net
            .gossip_publish(crate::network::DATASETS_TOPIC.to_string(), data)
            .await
    }
}
