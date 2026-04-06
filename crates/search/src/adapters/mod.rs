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
mod open_data_skill;
pub mod pan_search;
mod postgresql;
mod rwa_xyz;
mod semantic_scholar;
mod sql_endpoint;
pub(crate) mod util;

use anyhow::Result;
use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog};
use data_core::types::{SearchResult, SkillCapability, SourceFamily};

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
pub use open_data_skill::{
    load_data_skill_profiles, load_open_data_skills, DataSkillProfile, OpenDataSkillSpec,
};
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
    fn skill_id(&self) -> &str {
        self.name()
    }
    fn source_family(&self) -> SourceFamily {
        infer_source_family_for_skill_id(self.skill_id())
    }
    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![SkillCapability::Search]
    }
    fn labels(&self) -> Vec<String> {
        vec![]
    }
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

pub fn infer_source_family_for_skill_id(skill_id: &str) -> SourceFamily {
    match skill_id.trim().to_ascii_lowercase().as_str() {
        "kaggle" | "huggingface" | "guixu_hub" | "guixu-hub" => SourceFamily::Marketplace,
        "arxiv" | "dblp" | "semantic_scholar" | "datacite_commons" => SourceFamily::Academic,
        "ipfs" | "bittorrent" => SourceFamily::Decentralized,
        "postgresql" | "duckdb" | "spark" | "flink" | "presto" | "sql_endpoint" => {
            SourceFamily::DbCatalog
        }
        "local_file" | "localfile" => SourceFamily::Local,
        "google_dataset_search"
        | "pan_search"
        | "open_data_skill"
        | "opendataskill"
        | "defillama"
        | "rwa_xyz"
        | "rwaxyz"
        | "thegraph" => SourceFamily::WebRegistry,
        _ => SourceFamily::Custom,
    }
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
    open_data_skill::adapters_from_open_data_skills(
        disabled,
        duckdb_catalogs,
        pg_catalogs,
        sql_catalogs,
    )
}

/// Create all default adapters (no filtering).
pub fn default_adapters() -> Vec<Box<dyn ExternalAdapter>> {
    default_adapters_filtered(&[])
}
