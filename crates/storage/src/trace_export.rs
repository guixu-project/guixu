// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! JSONL export for guixu traces.
//!
//! Exports traces in a format compatible with the import pipeline,
//! so traces can be round-tripped through the OpenAI/Claude importer.

use crate::trace_store::{SpanRecord, TraceStore};
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Export utility for guixu traces.
pub struct TraceExporter;

impl TraceExporter {
    /// Export all spans for a single trace to a JSONL file.
    ///
    /// Returns the number of spans written.
    pub fn export_trace(
        trace_id: &str,
        source: &str,
        store: &TraceStore,
        path: &Path,
    ) -> Result<usize> {
        let spans = store.get_trace_spans(trace_id, source)?;
        Self::export_spans(&spans, path)
    }

    /// Export spans to a JSONL file (one JSON object per line).
    ///
    /// Returns the number of spans written.
    pub fn export_spans(spans: &[SpanRecord], path: &Path) -> Result<usize> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut writer = BufWriter::new(file);
        let count = spans.len();
        for span in spans {
            serde_json::to_writer(&mut writer, span)?;
            writeln!(&mut writer)?;
        }
        writer.flush()?;
        Ok(count)
    }

    /// Export all traces for a given source to a directory (one file per trace).
    ///
    /// File names are `{trace_id}.jsonl`.
    /// Returns the number of traces exported.
    pub fn export_all_traces(source: &str, store: &TraceStore, dir: &Path) -> Result<usize> {
        std::fs::create_dir_all(dir)?;
        let traces = store.list_traces(source, 10000)?;
        let mut count = 0;
        for trace in traces {
            let path = dir.join(format!("{}.jsonl", trace.trace_id));
            Self::export_trace(&trace.trace_id, source, store, &path)?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_store::{SpanRecord, SpanType, TraceSource, TraceStore};
    use chrono::Utc;

    #[test]
    fn test_export_spans() {
        let store = TraceStore::open_in_memory().unwrap();
        let trace_id = "test_export";
        let span = SpanRecord::new(trace_id, "s1", None::<String>, "root", SpanType::Agent)
            .with_source(TraceSource::Guixu);
        store.insert_span(&span).unwrap();

        let tempdir = std::env::temp_dir().join("guixu_trace_export_test");
        let path = tempdir.join("export.jsonl");
        std::fs::create_dir_all(&tempdir).unwrap();

        let count = TraceExporter::export_trace(trace_id, "guixu", &store, &path).unwrap();
        assert_eq!(count, 1);

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SpanRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.trace_id, trace_id);

        std::fs::remove_dir_all(&tempdir).ok();
    }
}
