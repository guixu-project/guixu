use data_core::metadata::DatasetMetadata;
use data_core::types::VideoMeta;

/// Video-specific quality dimensions for TCV scoring.
pub struct VideoQualityReport {
    pub resolution_score: f64,
    pub duration_score: f64,
    pub diversity_score: f64,
    pub annotation_score: f64,
    pub total: f64,
}

/// Evaluate video datasets with domain-specific heuristics.
pub struct VideoEvaluator;

impl VideoEvaluator {
    pub fn evaluate(metadata: &DatasetMetadata) -> VideoQualityReport {
        let vm = metadata.video_meta.as_ref();
        let resolution = Self::score_resolution(vm);
        let duration = Self::score_duration(vm);
        let diversity = Self::score_diversity(vm);
        let annotation = Self::score_annotation(vm);

        let total = resolution * 0.30 + duration * 0.25 + diversity * 0.25 + annotation * 0.20;
        VideoQualityReport {
            resolution_score: resolution,
            duration_score: duration,
            diversity_score: diversity,
            annotation_score: annotation,
            total,
        }
    }

    /// Higher resolution → higher score, capped at 4K.
    fn score_resolution(vm: Option<&VideoMeta>) -> f64 {
        let Some(vm) = vm else { return 30.0 };
        let pixels = vm.width as f64 * vm.height as f64;
        // 1080p ≈ 2M pixels → 70, 4K ≈ 8M → 100
        (pixels / 8_294_400.0 * 100.0).clamp(10.0, 100.0)
    }

    /// Longer content has more training value, diminishing returns past 1h.
    fn score_duration(vm: Option<&VideoMeta>) -> f64 {
        let Some(vm) = vm else { return 30.0 };
        let hours = vm.duration_secs / 3600.0;
        (hours.ln_1p() * 50.0).clamp(5.0, 100.0)
    }

    /// More scenes → more visual diversity.
    fn score_diversity(vm: Option<&VideoMeta>) -> f64 {
        let Some(vm) = vm else { return 30.0 };
        match vm.scene_count {
            Some(n) => ((n as f64).sqrt() * 15.0).clamp(10.0, 100.0),
            None => 40.0,
        }
    }

    /// Labelled / annotated video is far more valuable.
    fn score_annotation(vm: Option<&VideoMeta>) -> f64 {
        let Some(vm) = vm else { return 10.0 };
        if vm.labels.is_empty() {
            10.0
        } else {
            (vm.labels.len() as f64 * 10.0).clamp(20.0, 100.0)
        }
    }
}
