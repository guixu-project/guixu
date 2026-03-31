mod bittorrent;
mod datacite_commons;
mod dblp;
mod defillama;
mod duckdb;
mod google_dataset_search;
mod guixu_hub;
mod huggingface;
mod ipfs;
mod kaggle;
mod local_file;
pub mod pan_search;
mod postgresql;
mod rwa_xyz;
pub(crate) mod util;

use anyhow::Result;
use data_core::types::{DataSource, SearchResult};

pub use bittorrent::BitTorrentAdapter;
pub use datacite_commons::DataCiteCommonsAdapter;
pub use dblp::DblpAdapter;
pub use defillama::DefiLlamaAdapter;
pub use duckdb::DuckDbAdapter;
pub use google_dataset_search::GoogleDatasetSearchAdapter;
pub use guixu_hub::GuixuHubAdapter;
pub use huggingface::HuggingFaceAdapter;
pub use ipfs::IpfsAdapter;
pub use kaggle::KaggleAdapter;
pub use local_file::LocalFileAdapter;
pub use pan_search::PanSearchAdapter;
pub use postgresql::PostgreSqlAdapter;
pub use rwa_xyz::RwaXyzAdapter;

#[cfg(test)]
pub(crate) use util::infer_data_type_from_title;

/// Trait for external dataset platform adapters.
#[async_trait::async_trait]
pub trait ExternalAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn source_type(&self) -> DataSource;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

/// Create all default adapters, filtering out any whose name is in `disabled`.
pub fn default_adapters_filtered(disabled: &[String]) -> Vec<Box<dyn ExternalAdapter>> {
    let all: Vec<Box<dyn ExternalAdapter>> = vec![
        Box::new(KaggleAdapter::default()),
        Box::new(HuggingFaceAdapter::default()),
        Box::new(IpfsAdapter::default()),
        Box::new(GuixuHubAdapter::default()),
        Box::new(BitTorrentAdapter::default()),
        Box::new(PostgreSqlAdapter::default()),
        Box::new(DuckDbAdapter::default()),
        Box::new(LocalFileAdapter::default()),
        Box::new(GoogleDatasetSearchAdapter::default()),
        Box::new(DataCiteCommonsAdapter::default()),
        Box::new(DefiLlamaAdapter::default()),
        Box::new(RwaXyzAdapter::default()),
        Box::new(PanSearchAdapter::default()),
        Box::new(DblpAdapter::default()),
    ];
    if disabled.is_empty() {
        return all;
    }
    all.into_iter()
        .filter(|a| !disabled.iter().any(|d| d.eq_ignore_ascii_case(a.name())))
        .collect()
}

/// Create all default adapters (no filtering).
pub fn default_adapters() -> Vec<Box<dyn ExternalAdapter>> {
    default_adapters_filtered(&[])
}
