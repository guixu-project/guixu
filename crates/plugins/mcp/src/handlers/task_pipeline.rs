use std::cmp::Ordering;

use anyhow::Result;
use serde_json::{json, Value};

use data_core::feedback::CommunitySignal;
use data_core::types::DatasetCid;
use data_search::engine::SearchFilters;
use data_search::intent::{IntentParser, QueryProfile};

use crate::server::AppState;
use crate::state::ToolProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopAfter {
    IntentParse,
    DatasetSearch,
    DatasetEvaluate,
}

impl StopAfter {
    fn as_str(self) -> &'static str {
        match self {
            Self::IntentParse => "intent_parse",
            Self::DatasetSearch => "dataset_search",
            Self::DatasetEvaluate => "dataset_evaluate",
        }
    }
}

fn parse_stop_after(args: &Value) -> Result<StopAfter> {
    if let Some(value) = args.get("stop_after").and_then(|v| v.as_str()) {
        return match value.trim() {
            "intent_parse" => Ok(StopAfter::IntentParse),
            "dataset_search" => Ok(StopAfter::DatasetSearch),
            "dataset_evaluate" => Ok(StopAfter::DatasetEvaluate),
            other => anyhow::bail!(
                "invalid stop_after '{other}': expected intent_parse, dataset_search, or dataset_evaluate"
            ),
        };
    }

    let pipeline = args.get("pipeline").cloned().unwrap_or_default();
    let run_evaluate = pipeline
        .get("dataset_evaluate")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let run_search = pipeline
        .get("dataset_search")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if run_evaluate {
        Ok(StopAfter::DatasetEvaluate)
    } else if run_search {
        Ok(StopAfter::DatasetSearch)
    } else {
        Ok(StopAfter::IntentParse)
    }
}

fn compact_intent(profile: &QueryProfile) -> Value {
    json!({
        "task_type": profile.task_type,
        "task_description": profile.task_description,
        "budget": profile.budget,
        "target_entity": profile.target_entity,
        "keywords": profile.keywords,
        "data_standard": {
            "sample_unit": profile.data_standard.sample_unit,
            "canonical_columns": profile.data_standard.canonical_columns,
            "extra_columns": profile.data_standard.extra_columns,
            "metadata_fields": profile.data_standard.metadata_fields,
        }
    })
}

fn compact_candidate(result: &Value) -> Value {
    json!({
        "cid": result.get("cid").cloned().unwrap_or(Value::Null),
        "title": result.get("title").cloned().unwrap_or(Value::Null),
        "description": result.get("description").cloned().unwrap_or(Value::Null),
        "source": result.get("source").cloned().unwrap_or(Value::Null),
        "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
        "price": result.get("price").cloned().unwrap_or(Value::Null),
        "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
        "rank": result.get("rank").cloned().unwrap_or(Value::Null),
        "rank_score": result.get("rank_score").cloned().unwrap_or(Value::Null),
    })
}

fn compact_selected_dataset(result: &Value) -> Value {
    json!({
        "cid": result.get("cid").cloned().unwrap_or(Value::Null),
        "title": result.get("title").cloned().unwrap_or(Value::Null),
        "source": result.get("source").cloned().unwrap_or(Value::Null),
        "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
        "price": result.get("price").cloned().unwrap_or(Value::Null),
        "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
        "evaluation_mode": result.get("evaluation_mode").cloned().unwrap_or(Value::Null),
        "tcv_score": result.get("tcv_score").cloned().unwrap_or(Value::Null),
        "verdict": result.get("verdict").cloned().unwrap_or(Value::Null),
    })
}

fn parse_json_or_text(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!({ "text": raw }))
}

fn collect_search_filters(search_args: &Value) -> (SearchFilters, usize) {
    let filter_obj = search_args.get("filters").cloned().unwrap_or_default();
    let filters = SearchFilters {
        topic: filter_obj
            .get("topic")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_rows: filter_obj.get("min_rows").and_then(|v| v.as_u64()),
        max_price: filter_obj.get("max_price").and_then(|v| v.as_f64()),
        license: filter_obj
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_quality: filter_obj.get("min_quality").and_then(|v| v.as_f64()),
        source: filter_obj
            .get("source")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    let limit = search_args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    (filters, limit)
}

fn default_required_columns(profile: &QueryProfile) -> Vec<String> {
    let mut columns = profile.data_standard.canonical_columns.clone();
    for column in &profile.data_standard.extra_columns {
        if !columns.contains(column) {
            columns.push(column.clone());
        }
    }
    columns.retain(|column| !column.trim().is_empty());
    columns
}

fn evaluate_required_columns(evaluate_args: &Value, profile: &QueryProfile) -> Vec<String> {
    let explicit: Vec<String> = evaluate_args
        .get("required_columns")
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    if explicit.is_empty() {
        default_required_columns(profile)
    } else {
        explicit
    }
}

fn extract_candidate_cids(results: &[Value]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| result.get("cid").and_then(|v| v.as_str()))
        .map(String::from)
        .collect()
}

fn extract_score(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

fn extract_percentage(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| {
            value.as_str().and_then(|raw| {
                raw.trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|parsed| parsed / 100.0)
            })
        })
        .unwrap_or(0.0)
}

fn heuristic_verdict(score: f64) -> &'static str {
    match score {
        s if s > 60.0 => "strongpositive",
        s if s > 30.0 => "positive",
        s if s > 0.0 => "neutral",
        s if s > -30.0 => "negative",
        _ => "strongnegative",
    }
}

fn heuristic_report(result: &Value) -> Value {
    let columns = result
        .get("schema")
        .and_then(|schema| schema.get("columns"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let rows = result
        .get("schema")
        .and_then(|schema| schema.get("rows"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reviews = result
        .get("community")
        .and_then(|community| community.get("total_reviews"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let positive_rate = result
        .get("community")
        .and_then(|community| community.get("positive_rate"))
        .map(extract_percentage)
        .unwrap_or(0.0);
    let negative_rate = result
        .get("community")
        .and_then(|community| community.get("negative_rate"))
        .map(extract_percentage)
        .unwrap_or(0.0);

    let size_score = ((rows.max(1) as f64) / 100.0).min(100.0);
    let schema_fit = if columns > 0 { 60.0 } else { 20.0 };
    let temporal_fit = 50.0;
    let information_gain = 60.0;
    let quality_score = (30.0 + size_score * 0.5).min(80.0);
    let community_signal = if reviews > 0 {
        positive_rate * 80.0
    } else {
        30.0
    };
    let risk_penalty = negative_rate * 100.0;
    let raw = 0.25 * schema_fit
        + 0.15 * temporal_fit
        + 0.15 * information_gain
        + 0.10 * quality_score
        + 0.15 * community_signal
        - 0.20 * risk_penalty;
    let tcv_score = raw.clamp(-100.0, 100.0);
    let verdict = heuristic_verdict(tcv_score);

    json!({
        "tcv": {
            "tcv_score": tcv_score,
            "schema_fit": schema_fit,
            "temporal_fit": temporal_fit,
            "information_gain": information_gain,
            "quality_score": quality_score,
            "community_signal": community_signal,
            "risk_penalty": risk_penalty,
            "verdict": verdict,
            "explanation": "Heuristic TCV fallback derived from the demo UI for externally discovered datasets."
        },
        "community_feedback": {
            "total_reviews": reviews,
            "avg_relevance": result
                .get("community")
                .and_then(|community| community.get("avg_relevance"))
                .map(extract_score)
                .unwrap_or(0.0),
            "positive_rate": positive_rate,
            "negative_rate": negative_rate,
            "task_specific_signals": Vec::<Value>::new(),
        }
    })
}

fn report_tcv_score(report: &Value) -> f64 {
    report
        .get("tcv")
        .and_then(|tcv| tcv.get("tcv_score"))
        .map(extract_score)
        .unwrap_or(0.0)
}

fn report_verdict(report: &Value) -> Value {
    report
        .get("tcv")
        .and_then(|tcv| tcv.get("verdict"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn build_normalized_results(results: &[Value], profile: &QueryProfile) -> Vec<Value> {
    let standard = json!({
        "sample_unit": profile.data_standard.sample_unit,
        "metadata_fields": profile.data_standard.metadata_fields,
        "canonical_columns": profile.data_standard.canonical_columns,
        "extra_columns": profile.data_standard.extra_columns,
    });

    results
        .iter()
        .map(|result| {
            json!({
                "dataset_id": result.get("cid").cloned().unwrap_or(Value::Null),
                "name": result.get("title").cloned().unwrap_or(Value::Null),
                "description": result.get("description").cloned().unwrap_or(Value::Null),
                "source": result.get("source").cloned().unwrap_or(Value::Null),
                "modality": result.get("data_type").cloned().unwrap_or(Value::Null),
                "price": result.get("price").cloned().unwrap_or(Value::Null),
                "observed_schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
                "target_standard": standard.clone(),
            })
        })
        .collect()
}

async fn search_results_json(
    state: &AppState,
    profile: &QueryProfile,
    filters: &SearchFilters,
    limit: usize,
) -> Result<(Vec<Value>, Vec<String>)> {
    let local_metadata = state.store.list_all()?;

    let fb_store = state.feedback_store.clone();
    let signal_fetcher: data_search::engine::SignalFetcher = Box::new(move |cid_str: &str| {
        let cid = DatasetCid(cid_str.to_string());
        fb_store
            .compute_signal(&cid)
            .unwrap_or_else(|_| CommunitySignal {
                dataset_cid: cid,
                total_reviews: 0,
                avg_relevance: 0.0,
                avg_quality: 0.0,
                positive_rate: 0.0,
                negative_rate: 0.0,
                task_signals: vec![],
            })
    });

    let search_output = state
        .search_engine
        .search_with_profile(profile, filters, &local_metadata, &signal_fetcher, limit)
        .await?;

    let results = search_output
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "rank": i + 1,
                "cid": r.result.cid.0,
                "title": r.result.title,
                "description": r.result.description,
                "source": r.result.source,
                "data_type": r.result.data_type,
                "price": r.result.price,
                "schema": {
                    "columns": r.result.schema.columns.len(),
                    "rows": r.result.schema.row_count,
                    "size_bytes": r.result.schema.size_bytes,
                },
                "rank_score": format!("{:.1}", r.rank_score),
                "community": {
                    "total_reviews": r.signal.total_reviews,
                    "avg_relevance": format!("{:.2}", r.signal.avg_relevance),
                    "positive_rate": format!("{:.0}%", r.signal.positive_rate * 100.0),
                    "negative_rate": format!("{:.0}%", r.signal.negative_rate * 100.0),
                }
            })
        })
        .collect();

    Ok((results, search_output.errors))
}

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let raw_query = args
        .get("raw_query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let intent_query = raw_query.or(query).ok_or_else(|| anyhow::anyhow!("missing query"))?;
    let task_type_override = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let stop_after = parse_stop_after(&args)?;

    let search_args = args.get("search").cloned().unwrap_or_default();
    let evaluate_args = args.get("evaluate").cloned().unwrap_or_default();
    let (mut filters, limit) = collect_search_filters(&search_args);
    if matches!(state.tool_profile, ToolProfile::CodexWorkflow) {
        filters.source = None;
    }
    let requested_top_k = evaluate_args
        .get("top_k")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let parser = IntentParser::default();
    let mut profile = match parser.profile(intent_query).await {
        Ok(profile) => profile,
        Err(e) => {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "failed",
                "stop_after": stop_after.as_str(),
                "completed_steps": [],
                "failed_stage": StopAfter::IntentParse.as_str(),
                "error": e.to_string(),
            }))?)
        }
    };
    if let Some(task_type) = task_type_override {
        profile.task_type = Some(task_type);
    }

    if stop_after == StopAfter::IntentParse {
        return Ok(serde_json::to_string_pretty(&json!({
            "status": "completed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str()],
            "intent": compact_intent(&profile),
        }))?);
    }

    let (search_results, search_errors) = match search_results_json(state, &profile, &filters, limit).await {
        Ok(result) => result,
        Err(e) => {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "failed",
                "stop_after": stop_after.as_str(),
                "completed_steps": [StopAfter::IntentParse.as_str()],
                "failed_stage": StopAfter::DatasetSearch.as_str(),
                "error": e.to_string(),
                "intent": compact_intent(&profile),
            }))?)
        }
    };
    let search_candidate_cids = extract_candidate_cids(&search_results);
    let compact_candidates: Vec<Value> = search_results.iter().map(compact_candidate).collect();

    if stop_after == StopAfter::DatasetSearch {
        if compact_candidates.is_empty() && !search_errors.is_empty() {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "failed",
                "stop_after": stop_after.as_str(),
                "completed_steps": [StopAfter::IntentParse.as_str()],
                "failed_stage": StopAfter::DatasetSearch.as_str(),
                "error": search_errors.first().cloned().unwrap_or_else(|| "dataset search failed".to_string()),
                "intent": compact_intent(&profile),
                "candidate_count": 0,
                "candidates": [],
            }))?);
        }

        return Ok(serde_json::to_string_pretty(&json!({
            "status": "completed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "intent": compact_intent(&profile),
            "candidate_count": compact_candidates.len(),
            "candidates": compact_candidates,
        }))?);
    }

    let _normalized_results = build_normalized_results(&search_results, &profile);

    if search_candidate_cids.is_empty() {
        let (status, failed_stage, error) = if search_errors.is_empty() {
            (
                "blocked",
                StopAfter::DatasetEvaluate.as_str(),
                "no candidate datasets available for evaluation".to_string(),
            )
        } else {
            (
                "failed",
                StopAfter::DatasetSearch.as_str(),
                search_errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "dataset search failed".to_string()),
            )
        };
        return Ok(serde_json::to_string_pretty(&json!({
            "status": status,
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "failed_stage": failed_stage,
            "error": error,
            "intent": compact_intent(&profile),
            "candidate_count": 0,
            "selected_dataset": Value::Null,
        }))?);
    }

    let required_columns = evaluate_required_columns(&evaluate_args, &profile);
    let budget = evaluate_args
        .get("budget")
        .and_then(|v| v.as_f64())
        .unwrap_or(profile.budget);
    let task_description = profile
        .task_description
        .clone()
        .unwrap_or_else(|| profile.raw_query.clone());
    let task_type = profile
        .task_type
        .clone()
        .unwrap_or_else(|| "general".to_string());
    let evaluate_top_k = requested_top_k.unwrap_or(search_results.len());

    let mut evaluated_results = Vec::new();
    let mut stage_errors = Vec::new();

    for result in search_results.iter().take(evaluate_top_k) {
        let cid = result
            .get("cid")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if cid.is_empty() {
            continue;
        }

        let dataset_cid = DatasetCid(cid.clone());
        let report = if state.store.get(&dataset_cid)?.is_some() {
            match crate::handlers::evaluate::handle(
                json!({
                    "cid": cid,
                    "task_description": task_description,
                    "task_type": task_type,
                    "required_columns": required_columns,
                    "budget": budget,
                }),
                state,
            )
            .await
            {
                Ok(report) => parse_json_or_text(&report),
                Err(e) => {
                    stage_errors.push(e.to_string());
                    continue;
                }
            }
        } else {
            heuristic_report(result)
        };

        evaluated_results.push(json!({
            "cid": cid,
            "title": result.get("title").cloned().unwrap_or(Value::Null),
            "source": result.get("source").cloned().unwrap_or(Value::Null),
            "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
            "price": result.get("price").cloned().unwrap_or(Value::Null),
            "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
            "evaluation_mode": if state.store.get(&dataset_cid)?.is_some() {
                "local_tcv"
            } else {
                "heuristic"
            },
            "tcv_score": report_tcv_score(&report),
            "verdict": report_verdict(&report),
        }));
    }

    evaluated_results.sort_by(|left, right| {
        let left_score = left.get("tcv_score").map(extract_score).unwrap_or(0.0);
        let right_score = right.get("tcv_score").map(extract_score).unwrap_or(0.0);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(Ordering::Equal)
    });

    if evaluated_results.is_empty() {
        return Ok(serde_json::to_string_pretty(&json!({
            "status": "failed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "failed_stage": StopAfter::DatasetEvaluate.as_str(),
            "error": stage_errors
                .first()
                .cloned()
                .unwrap_or_else(|| "dataset evaluation produced no usable result".to_string()),
            "intent": compact_intent(&profile),
            "candidate_count": compact_candidates.len(),
            "selected_dataset": Value::Null,
        }))?);
    }

    let selected_dataset = evaluated_results
        .first()
        .map(compact_selected_dataset)
        .unwrap_or(Value::Null);

    Ok(serde_json::to_string_pretty(&json!({
        "status": "completed",
        "stop_after": stop_after.as_str(),
        "completed_steps": [
            StopAfter::IntentParse.as_str(),
            StopAfter::DatasetSearch.as_str(),
            StopAfter::DatasetEvaluate.as_str(),
        ],
        "intent": compact_intent(&profile),
        "candidate_count": compact_candidates.len(),
        "selected_dataset": selected_dataset,
    }))?)
}

#[cfg(test)]
mod tests {
    use super::{extract_percentage, heuristic_report, parse_stop_after, StopAfter};
    use serde_json::json;

    #[test]
    fn heuristic_report_matches_demo_ui_defaults_without_reviews() {
        let report = heuristic_report(&json!({
            "community": {
                "total_reviews": 0,
                "avg_relevance": "0.00",
                "positive_rate": "0%",
                "negative_rate": "0%"
            },
            "schema": {
                "columns": 0,
                "rows": 0,
                "size_bytes": 1024
            }
        }));

        let score = report["tcv"]["tcv_score"].as_f64().unwrap();
        assert!(
            (score - 29.0005).abs() < 1e-6,
            "unexpected heuristic score: {score}"
        );
        assert_eq!(report["tcv"]["verdict"], "neutral");
    }

    #[test]
    fn extract_percentage_parses_percent_strings() {
        let value = json!("75%");
        let parsed = extract_percentage(&value);
        assert!((parsed - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_stop_after_defaults_to_dataset_evaluate() {
        let stop_after = parse_stop_after(&json!({})).unwrap();
        assert_eq!(stop_after, StopAfter::DatasetEvaluate);
    }

    #[test]
    fn parse_stop_after_supports_legacy_pipeline_flags() {
        let stop_after = parse_stop_after(&json!({
            "pipeline": {
                "intent_parse": true,
                "dataset_search": true,
                "dataset_evaluate": false
            }
        }))
        .unwrap();
        assert_eq!(stop_after, StopAfter::DatasetSearch);
    }
}
