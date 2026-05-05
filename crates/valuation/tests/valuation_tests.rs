// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::*;
use data_test_utils::builders::{
    CommunitySignalBuilder, DatasetMetadataBuilder, QualityScoreBuilder,
};

fn make_metadata(title: &str, columns: &[(&str, &str)], price: f64) -> DatasetMetadata {
    DatasetMetadataBuilder::new(&format!("cid-{title}"))
        .title(title)
        .columns(columns)
        .price(price)
        .access(AccessMode::Open)
        .build()
}

fn make_signal(total: u64, pos_rate: f64, neg_rate: f64) -> CommunitySignal {
    CommunitySignalBuilder::new("cid-test")
        .reviews(total, pos_rate, neg_rate)
        .build()
}

fn make_quality(total: f64) -> QualityScore {
    QualityScoreBuilder::new(total).build()
}

// ============================================================
// TCV — KNN-Shapley information gain
// ============================================================
mod tcv_tests {
    use super::*;
    use data_valuation::tcv::{TaskContext, TcvEngine, TcvVerdict};

    fn task(required: &[&str], existing: &[&str]) -> TaskContext {
        TaskContext {
            task_description: "classify sentiment".into(),
            task_type: "classification".into(),
            required_columns: required.iter().map(|s| s.to_string()).collect(),
            time_range: None,
            existing_data_cids: existing.iter().map(|s| s.to_string()).collect(),
            budget: 10.0,
        }
    }

    #[test]
    fn info_gain_max_when_no_existing_data() {
        let engine = TcvEngine;
        let meta = make_metadata("a", &[("text", "utf8"), ("label", "int64")], 0.0);
        let signal = make_signal(0, 0.0, 0.0);
        let report = engine.evaluate(&meta, &task(&["text", "label"], &[]), &signal);
        // With no existing data, information_gain should be 100
        assert!((report.information_gain - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn info_gain_zero_when_same_cid() {
        let engine = TcvEngine;
        let meta = make_metadata("a", &[("text", "utf8")], 0.0);
        let signal = make_signal(0, 0.0, 0.0);
        let report = engine.evaluate(&meta, &task(&["text"], &["cid-a"]), &signal);
        assert!((report.information_gain).abs() < f64::EPSILON);
    }

    #[test]
    fn info_gain_higher_for_better_schema_match() {
        let engine = TcvEngine;
        let good = make_metadata("good", &[("text", "utf8"), ("label", "int64")], 0.0);
        let bad = make_metadata("bad", &[("foo", "utf8"), ("bar", "int64")], 0.0);
        let signal = make_signal(0, 0.0, 0.0);
        let t = task(&["text", "label"], &["cid-other"]);

        let gain_good = engine.evaluate(&good, &t, &signal).information_gain;
        let gain_bad = engine.evaluate(&bad, &t, &signal).information_gain;
        assert!(
            gain_good > gain_bad,
            "good match ({gain_good}) should have higher info gain than bad match ({gain_bad})"
        );
    }

    #[test]
    fn info_gain_decreases_with_more_existing_data() {
        let engine = TcvEngine;
        let meta = make_metadata("a", &[("text", "utf8"), ("label", "int64")], 0.0);
        let signal = make_signal(0, 0.0, 0.0);

        let gain_1 = engine
            .evaluate(&meta, &task(&["text", "label"], &["cid-x"]), &signal)
            .information_gain;
        let gain_3 = engine
            .evaluate(
                &meta,
                &task(&["text", "label"], &["cid-x", "cid-y", "cid-z"]),
                &signal,
            )
            .information_gain;
        assert!(
            gain_1 > gain_3,
            "gain with 1 existing ({gain_1}) should exceed gain with 3 existing ({gain_3})"
        );
    }

    #[test]
    fn partial_column_match_gives_partial_gain() {
        let engine = TcvEngine;
        // "text_content" partially matches "text"
        let meta = make_metadata("a", &[("text_content", "utf8"), ("id", "int64")], 0.0);
        let signal = make_signal(0, 0.0, 0.0);
        let t = task(&["text", "label"], &["cid-other"]);
        let report = engine.evaluate(&meta, &t, &signal);
        // Should get partial credit, not zero
        assert!(report.information_gain > 0.0);
        // But not full credit
        assert!(report.information_gain < 100.0);
    }

    #[test]
    fn strong_positive_verdict_for_perfect_dataset() {
        let engine = TcvEngine;
        let meta = make_metadata("perf", &[("text", "utf8"), ("label", "int64")], 0.0);
        let signal = make_signal(20, 0.9, 0.05);
        let t = task(&["text", "label"], &[]);
        let report = engine.evaluate(&meta, &t, &signal);
        assert_eq!(report.verdict, TcvVerdict::StrongPositive);
        assert!(report.tcv_score > 80.0);
    }

    #[test]
    fn tcv_score_is_normalized_to_zero_to_hundred() {
        let engine = TcvEngine;
        let meta = make_metadata("poor", &[("foo", "utf8")], 0.0);
        let signal = make_signal(20, 0.1, 0.9);
        let t = task(&["text", "label"], &["cid-x", "cid-y", "cid-z"]);
        let report = engine.evaluate(&meta, &t, &signal);

        assert!(report.tcv_score >= 0.0);
        assert!(report.tcv_score <= 100.0);
    }
}

// ============================================================
// FreeDataEvaluator — PMI-based information gain
// ============================================================
mod free_evaluator_tests {
    use super::*;
    use data_valuation::free_evaluator::{FreeDataEvaluator, TaskContext};

    fn task(required: &[&str]) -> TaskContext {
        TaskContext {
            task_description: "classify sentiment".into(),
            required_columns: required.iter().map(|s| s.to_string()).collect(),
            time_range: None,
            existing_data_cids: vec![],
        }
    }

    #[tokio::test]
    async fn pmi_high_for_exact_column_match() {
        let eval = FreeDataEvaluator;
        let meta = make_metadata("a", &[("text", "utf8"), ("label", "int64")], 0.0);
        let report = eval
            .evaluate(&meta, &task(&["text", "label"]))
            .await
            .unwrap();
        // Exact match on all columns → high information gain
        assert!(
            report.information_gain > 50.0,
            "exact match info gain ({}) should be > 50",
            report.information_gain
        );
    }

    #[tokio::test]
    async fn pmi_low_for_no_column_match() {
        let eval = FreeDataEvaluator;
        let meta = make_metadata("a", &[("foo", "utf8"), ("bar", "int64")], 0.0);
        let report = eval
            .evaluate(&meta, &task(&["text", "label"]))
            .await
            .unwrap();
        assert!(
            report.information_gain < 10.0,
            "no match info gain ({}) should be < 10",
            report.information_gain
        );
    }

    #[tokio::test]
    async fn pmi_partial_for_substring_match() {
        let eval = FreeDataEvaluator;
        let meta = make_metadata("a", &[("text_content", "utf8"), ("id", "int64")], 0.0);
        let report = eval
            .evaluate(&meta, &task(&["text", "label"]))
            .await
            .unwrap();
        // Partial match: "text_content" contains "text"
        assert!(report.information_gain > 0.0);
        assert!(report.information_gain < 70.0);
    }

    #[tokio::test]
    async fn pmi_neutral_for_empty_columns() {
        let eval = FreeDataEvaluator;
        let meta = make_metadata("a", &[("text", "utf8")], 0.0);
        let report = eval.evaluate(&meta, &task(&[])).await.unwrap();
        assert!((report.information_gain - 50.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn total_score_reflects_all_dimensions() {
        let eval = FreeDataEvaluator;
        let meta = make_metadata("a", &[("text", "utf8"), ("label", "int64")], 0.0);
        let report = eval
            .evaluate(&meta, &task(&["text", "label"]))
            .await
            .unwrap();
        assert!(report.total_score > 0.0);
        assert!(report.total_score <= 100.0);
    }
}

// ============================================================
// PaidDataEvaluator — budget-constrained value maximization
// ============================================================
mod paid_evaluator_tests {
    use super::*;
    use data_valuation::paid_evaluator::{
        PaidDataEvaluator, PaidDatasetCandidate, PortfolioConstraints,
    };

    #[tokio::test]
    async fn mmd_higher_value_when_no_free_alternative() {
        let eval = PaidDataEvaluator;
        let meta = make_metadata("paid", &[("text", "utf8"), ("label", "int64")], 1.0);
        let quality = make_quality(80.0);
        let signal = make_signal(10, 0.8, 0.1);

        let report_no_free = eval.evaluate(&meta, &quality, &[], &signal).await.unwrap();

        let free_meta = make_metadata("free", &[("text", "utf8")], 0.0);
        let free_q = make_quality(70.0);
        let report_with_free = eval
            .evaluate(&meta, &quality, &[(&free_meta, &free_q)], &signal)
            .await
            .unwrap();

        assert!(
            report_no_free.estimated_value > 0.0,
            "should have positive value without free alt"
        );
        assert!(report_no_free.scarcity_premium > report_with_free.scarcity_premium);
    }

    #[tokio::test]
    async fn mmd_detects_identical_datasets() {
        let eval = PaidDataEvaluator;
        let meta = make_metadata("paid", &[("text", "utf8"), ("label", "int64")], 1.0);
        let quality = make_quality(80.0);
        let signal = make_signal(5, 0.8, 0.1);

        // Free alternative with identical schema and quality
        let free_meta = make_metadata("free", &[("text", "utf8"), ("label", "int64")], 0.0);
        let free_q = make_quality(80.0);

        let report = eval
            .evaluate(&meta, &quality, &[(&free_meta, &free_q)], &signal)
            .await
            .unwrap();

        // When datasets are very similar, MMD is small → low estimated value
        // Should recommend skipping
        assert!(report.has_free_alternative);
    }

    #[tokio::test]
    async fn mmd_high_value_for_different_distributions() {
        let eval = PaidDataEvaluator;
        let meta = make_metadata(
            "paid",
            &[
                ("text", "utf8"),
                ("label", "int64"),
                ("embedding", "float64"),
            ],
            1.0,
        );
        let quality = make_quality(90.0);
        let signal = make_signal(10, 0.9, 0.05);

        // Free alternative with very different schema and lower quality
        let free_meta = make_metadata("free", &[("id", "int64")], 0.0);
        let free_q = make_quality(30.0);

        let report = eval
            .evaluate(&meta, &quality, &[(&free_meta, &free_q)], &signal)
            .await
            .unwrap();

        assert!(
            report.estimated_value > 0.0,
            "different distributions should yield positive value"
        );
    }

    #[tokio::test]
    async fn negative_reviews_trigger_caution() {
        let eval = PaidDataEvaluator;
        let meta = make_metadata("paid", &[("text", "utf8")], 1.0);
        let quality = make_quality(80.0);
        let signal = make_signal(20, 0.3, 0.5); // 50% negative

        let report = eval.evaluate(&meta, &quality, &[], &signal).await.unwrap();
        assert!(report.recommendation.contains("Caution"));
    }

    #[tokio::test]
    async fn portfolio_selection_prefers_higher_total_value_under_budget() {
        let eval = PaidDataEvaluator;
        let signal = make_signal(0, 0.0, 0.0);
        let no_free: &[(&DatasetMetadata, &QualityScore)] = &[];

        let dataset_a = make_metadata("dataset-a", &[("text", "utf8"), ("label", "int64")], 0.60);
        let dataset_b = make_metadata("dataset-b", &[("text", "utf8"), ("label", "int64")], 0.35);
        let dataset_c = make_metadata("dataset-c", &[("text", "utf8"), ("label", "int64")], 0.35);
        let quality_a = make_quality(60.0);
        let quality_b = make_quality(75.0);
        let quality_c = make_quality(70.0);

        let candidates = vec![
            PaidDatasetCandidate {
                metadata: &dataset_a,
                quality: &quality_a,
                free_alternatives: no_free,
                signal: &signal,
            },
            PaidDatasetCandidate {
                metadata: &dataset_b,
                quality: &quality_b,
                free_alternatives: no_free,
                signal: &signal,
            },
            PaidDatasetCandidate {
                metadata: &dataset_c,
                quality: &quality_c,
                free_alternatives: no_free,
                signal: &signal,
            },
        ];

        let report = eval
            .select_portfolio(&candidates, &PortfolioConstraints { max_budget: 0.70 })
            .await
            .unwrap();

        let selected_titles = report
            .selected
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(selected_titles, vec!["dataset-b", "dataset-c"]);
        assert!(report.total_spend <= 0.70);
        assert_eq!(report.selected.len(), 2);
        assert!(
            report.selected[0].estimated_value >= report.selected[1].estimated_value,
            "selected items should be sorted by estimated value descending"
        );
    }

    #[tokio::test]
    async fn portfolio_selection_maximizes_total_value_under_budget() {
        let eval = PaidDataEvaluator;
        let signal = make_signal(0, 0.0, 0.0);
        let no_free: &[(&DatasetMetadata, &QualityScore)] = &[];

        let high_value = make_metadata("high-value", &[("text", "utf8"), ("label", "int64")], 0.55);
        let lower_value_a = make_metadata(
            "lower-value-a",
            &[("text", "utf8"), ("label", "int64")],
            0.30,
        );
        let lower_value_b = make_metadata(
            "lower-value-b",
            &[("text", "utf8"), ("label", "int64")],
            0.30,
        );
        let quality_high = make_quality(90.0);
        let quality_a = make_quality(40.0);
        let quality_b = make_quality(30.0);

        let candidates = vec![
            PaidDatasetCandidate {
                metadata: &high_value,
                quality: &quality_high,
                free_alternatives: no_free,
                signal: &signal,
            },
            PaidDatasetCandidate {
                metadata: &lower_value_a,
                quality: &quality_a,
                free_alternatives: no_free,
                signal: &signal,
            },
            PaidDatasetCandidate {
                metadata: &lower_value_b,
                quality: &quality_b,
                free_alternatives: no_free,
                signal: &signal,
            },
        ];

        let report = eval
            .select_portfolio(&candidates, &PortfolioConstraints { max_budget: 0.60 })
            .await
            .unwrap();

        let selected_titles = report
            .selected
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(selected_titles, vec!["high-value"]);
        assert!(report.total_spend <= 0.60);
        assert!(
            report.total_estimated_value >= report.selected[0].estimated_value,
            "portfolio value should reflect the chosen high-value item"
        );
    }
}

// ============================================================
// MemoryEvaluator — TF-IDF cosine similarity
// ============================================================
mod memory_evaluator_tests {
    use super::*;
    use data_valuation::memory_evaluator::{MemoryEvaluator, MemoryMetadata, MemoryType};

    fn make_memory(desc: &str, caps: &[&str]) -> MemoryMetadata {
        MemoryMetadata {
            cid: "cid-mem".into(),
            memory_type: MemoryType::Procedural,
            description: desc.into(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            created_at: Utc::now(),
            last_verified_at: Some(Utc::now()),
            historical_success_rate: Some(0.8),
            dependencies: vec![],
        }
    }

    #[tokio::test]
    async fn tfidf_high_for_similar_descriptions() {
        let eval = MemoryEvaluator;
        let memory = make_memory(
            "sentiment classification using text data with neural networks",
            &["classification", "nlp"],
        );
        let report = eval
            .evaluate(&memory, "classify sentiment from text data", &[])
            .await
            .unwrap();

        assert!(
            report.task_relevance > 15.0,
            "similar descriptions should have relevance > 15, got {}",
            report.task_relevance
        );
    }

    #[tokio::test]
    async fn tfidf_low_for_unrelated_descriptions() {
        let eval = MemoryEvaluator;
        let memory = make_memory(
            "image segmentation using convolutional neural networks for medical imaging",
            &["segmentation", "cv"],
        );
        let report = eval
            .evaluate(&memory, "financial time series forecasting with ARIMA", &[])
            .await
            .unwrap();

        assert!(
            report.task_relevance < 30.0,
            "unrelated descriptions should have relevance < 30, got {}",
            report.task_relevance
        );
    }

    #[tokio::test]
    async fn tfidf_zero_for_empty_task() {
        let eval = MemoryEvaluator;
        let memory = make_memory("some description", &[]);
        let report = eval.evaluate(&memory, "", &[]).await.unwrap();
        assert!(
            report.task_relevance < f64::EPSILON,
            "empty task should yield 0 relevance"
        );
    }

    #[tokio::test]
    async fn tfidf_distinguishes_similar_vs_different() {
        let eval = MemoryEvaluator;
        let memory = make_memory(
            "data preprocessing pipeline for tabular classification tasks",
            &["preprocessing"],
        );

        let report_similar = eval
            .evaluate(&memory, "preprocess tabular data for classification", &[])
            .await
            .unwrap();
        let report_different = eval
            .evaluate(&memory, "render 3D graphics with ray tracing", &[])
            .await
            .unwrap();

        assert!(
            report_similar.task_relevance > report_different.task_relevance,
            "similar ({}) should beat different ({})",
            report_similar.task_relevance,
            report_different.task_relevance
        );
    }

    #[tokio::test]
    async fn overall_recommendation_reflects_fitness() {
        let eval = MemoryEvaluator;
        let memory = make_memory(
            "sentiment classification using text data",
            &["classification", "nlp", "text"],
        );
        let report = eval
            .evaluate(
                &memory,
                "classify sentiment from text",
                &["classification".into(), "nlp".into(), "text".into()],
            )
            .await
            .unwrap();

        assert!(report.total_score > 50.0);
        assert!(
            report.recommendation.contains("suitable") || report.recommendation.contains("Highly"),
            "good fit should be recommended, got: {}",
            report.recommendation
        );
    }
}

// ============================================================
// QualityScorer
// ============================================================
mod scorer_tests {
    use super::*;
    use data_valuation::scorer::QualityScorer;

    #[test]
    fn high_quality_metadata_scores_high() {
        let scorer = QualityScorer::new();
        let meta = make_metadata("good", &[("a", "int64"), ("b", "utf8")], 0.0);
        let score = scorer.score_from_metadata(&meta);
        assert!(
            score.total > 50.0,
            "good metadata should score > 50, got {}",
            score.total
        );
    }

    #[test]
    fn missing_stats_gives_moderate_score() {
        let scorer = QualityScorer::new();
        let mut meta = make_metadata("ok", &[("a", "int64")], 0.0);
        meta.stats = None;
        let score = scorer.score_from_metadata(&meta);
        assert!(score.completeness == 50.0);
        assert!(score.consistency == 50.0);
    }

    #[test]
    fn vc_boosts_provenance() {
        let scorer = QualityScorer::new();
        let mut meta = make_metadata("vc", &[("a", "int64")], 0.0);
        meta.verifiable_credential = Some(serde_json::json!({"type": "vc"}));
        let score = scorer.score_from_metadata(&meta);
        assert!((score.provenance - 80.0).abs() < f64::EPSILON);
    }
}
