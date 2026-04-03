// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::*;

use super::ExternalAdapter;

// Local file adapter (Parquet / CSV / JSON / TSV)
// ---------------------------------------------------------------------------

/// Scans user-specified directories for data files and matches by filename /
/// column names against the search query.  Supports Parquet, CSV, TSV and JSON.
pub struct LocalFileAdapter {
    /// Directories to scan. Empty → adapter is a no-op.
    pub dirs: Vec<std::path::PathBuf>,
}

impl Default for LocalFileAdapter {
    fn default() -> Self {
        // Honour GUIXU_DATA_DIRS env (colon-separated) if set
        let dirs = std::env::var("GUIXU_DATA_DIRS")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
        Self { dirs }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for LocalFileAdapter {
    fn name(&self) -> &str {
        "local_file"
    }
    fn source_type(&self) -> DataSource {
        DataSource::LocalFile
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if self.dirs.is_empty() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        let mut results = Vec::new();

        for dir in &self.dirs {
            for ext in &["csv", "tsv", "parquet", "json", "ndjson"] {
                let pattern = format!("{}/**/*.{ext}", dir.display());
                let paths = glob::glob(&pattern).unwrap_or_else(|_| glob::glob("").unwrap());
                for entry in paths.flatten() {
                    if results.len() >= limit {
                        break;
                    }
                    if let Some(r) = Self::probe_file(&entry, &query_lower, &keywords) {
                        results.push(r);
                    }
                }
            }
        }

        Ok(results)
    }
}

impl LocalFileAdapter {
    fn probe_file(path: &std::path::Path, query: &str, keywords: &[&str]) -> Option<SearchResult> {
        let file_name = path.file_stem()?.to_str()?.to_lowercase();
        let ext = path.extension()?.to_str()?;

        let (columns, row_count) = match ext {
            "parquet" => Self::read_parquet_schema(path).ok()?,
            "csv" | "tsv" => Self::read_csv_schema(path, ext == "tsv").ok()?,
            "json" | "ndjson" => Self::read_json_schema(path).ok()?,
            _ => return None,
        };

        let col_text = columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let all_text = format!("{file_name} {col_text}");

        // Match: any keyword appears in filename or column names
        let matched = all_text.contains(query) || keywords.iter().any(|kw| all_text.contains(kw));
        if !matched {
            return None;
        }

        let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let cid_hash = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(path.to_string_lossy().as_bytes());
            hex::encode(h.finalize())
        };

        Some(SearchResult {
            cid: DatasetCid(cid_hash),
            title: path.file_name()?.to_str()?.to_string(),
            description: Some(format!("local file: {}", path.display())),
            tags: vec![],
            schema: DatasetSchema {
                columns,
                row_count,
                size_bytes,
            },
            quality: None,
            price: Price::free(),
            license: License {
                spdx_id: "proprietary".into(),
                commercial_use: false,
                derivative_allowed: false,
            },
            provider: Did("did:local:self".into()),
            source: DataSource::LocalFile,
            market: None,
            data_type: DataType::from_ext(ext),
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: None,
        })
    }

    fn read_parquet_schema(path: &std::path::Path) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let file = std::fs::File::open(path)?;
        let reader = ParquetReader::new(file);
        let df = reader.finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }

    fn read_csv_schema(path: &std::path::Path, is_tsv: bool) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let sep = if is_tsv { b'\t' } else { b',' };
        let df = CsvReadOptions::default()
            .with_parse_options(CsvParseOptions::default().with_separator(sep))
            .with_n_rows(Some(256)) // only peek
            .try_into_reader_with_file_path(Some(path.into()))?
            .finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }

    fn read_json_schema(path: &std::path::Path) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let file = std::fs::File::open(path)?;
        let df = JsonReader::new(file).finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }
}
