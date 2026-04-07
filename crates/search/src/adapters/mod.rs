// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

mod arxiv;
mod bittorrent;
mod datacite_commons;
mod defillama;
mod google_dataset_search;
mod local_file;
mod open_data_skill;
pub mod pan_search;
mod rwa_xyz;
pub(crate) mod sql_catalog;
pub(crate) mod util;

use anyhow::{anyhow, Result};
use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog};
use data_core::types::{SearchResult, SkillCapability, SourceFamily};

pub use arxiv::ArxivAdapter;
pub use bittorrent::BitTorrentAdapter;
pub use datacite_commons::DataCiteCommonsAdapter;
pub use defillama::DefiLlamaAdapter;
pub use google_dataset_search::GoogleDatasetSearchAdapter;
pub use local_file::LocalFileAdapter;
pub use open_data_skill::{
    execute_skill_operation, load_data_skill_profiles, load_open_data_skills, DataSkillProfile,
    OpenDataSkillSpec, SkillProvider,
};
pub use pan_search::PanSearchAdapter;
pub use rwa_xyz::RwaXyzAdapter;

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
    async fn lookup(&self, _id: &str) -> Result<Vec<serde_json::Value>> {
        Err(anyhow!(
            "lookup unsupported for adapter: {}",
            self.skill_id()
        ))
    }
    async fn download(&self, _id: &str) -> Result<Vec<serde_json::Value>> {
        Err(anyhow!(
            "download unsupported for adapter: {}",
            self.skill_id()
        ))
    }
    async fn schema_probe(&self, _id: &str) -> Result<Vec<serde_json::Value>> {
        Err(anyhow!(
            "schema_probe unsupported for adapter: {}",
            self.skill_id()
        ))
    }
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
