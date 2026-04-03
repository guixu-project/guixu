// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

mod arxiv;
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
mod semantic_scholar;
mod sql_endpoint;
pub(crate) mod util;

use anyhow::Result;
use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog};
use data_core::types::{DataSource, SearchResult};

pub use arxiv::ArxivAdapter;
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
pub use semantic_scholar::SemanticScholarAdapter;
pub use sql_endpoint::SqlEndpointAdapter;

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
    adapters_with_config(disabled, &[], &[], &[])
}

/// Create adapters with external database catalogs configured.
pub fn adapters_with_config(
    disabled: &[String],
    duckdb_catalogs: &[DuckDbCatalog],
    pg_catalogs: &[PostgreSqlCatalog],
    sql_catalogs: &[SqlEndpointCatalog],
) -> Vec<Box<dyn ExternalAdapter>> {
    let all: Vec<Box<dyn ExternalAdapter>> = vec![
        Box::new(KaggleAdapter::default()),
        Box::new(HuggingFaceAdapter::default()),
        Box::new(IpfsAdapter::default()),
        Box::new(GuixuHubAdapter::default()),
        Box::new(BitTorrentAdapter::default()),
        Box::new(PostgreSqlAdapter::with_catalogs(pg_catalogs.to_vec())),
        Box::new(DuckDbAdapter::with_catalogs(duckdb_catalogs.to_vec())),
        Box::new(SqlEndpointAdapter::with_catalogs(sql_catalogs.to_vec())),
        Box::new(LocalFileAdapter::default()),
        Box::new(GoogleDatasetSearchAdapter::default()),
        Box::new(DataCiteCommonsAdapter::default()),
        Box::new(DefiLlamaAdapter::default()),
        Box::new(RwaXyzAdapter::default()),
        Box::new(PanSearchAdapter::default()),
        Box::new(DblpAdapter::default()),
        Box::new(SemanticScholarAdapter::default()),
        Box::new(ArxivAdapter::default()),
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
