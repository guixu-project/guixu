use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Evaluates Agent Memory/Skills for task fitness.
pub struct MemoryEvaluator;

/// Metadata describing an Agent Memory or Skill asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub cid: String,
    pub memory_type: MemoryType,
    pub description: String,
    pub capabilities: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub historical_success_rate: Option<f64>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
    ToolChain,
    PromptTemplate,
    LoraAdapter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub current_version: Option<String>, // filled during evaluation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFitnessReport {
    pub total_score: f64,
    pub task_relevance: f64,
    pub historical_success: f64,
    pub capability_coverage: f64,
    pub temporal_relevance: f64,
    pub transferability: f64,
    pub missing_capabilities: Vec<String>,
    pub outdated_dependencies: Vec<String>,
    pub recommendation: String,
}

impl MemoryEvaluator {
    /// Evaluate whether a Memory/Skill is suitable for the current task.
    pub async fn evaluate(
        &self,
        memory: &MemoryMetadata,
        task_description: &str,
        agent_capabilities: &[String],
    ) -> Result<MemoryFitnessReport> {
        // Task relevance: semantic similarity between memory description and task
        let task_relevance = self.compute_task_relevance(memory, task_description);

        // Historical success rate from on-chain attestations
        let historical_success = memory.historical_success_rate.unwrap_or(0.5) * 100.0;

        // Capability coverage: what % of needed capabilities does this memory provide
        let (capability_coverage, missing) =
            self.compute_capability_coverage(memory, agent_capabilities);

        // Temporal relevance: decay based on age and dependency freshness
        let (temporal_relevance, outdated) = self.compute_temporal_relevance(memory);

        // Transferability: how general is this memory
        let transferability = self.compute_transferability(memory);

        let total = task_relevance * 0.35
            + historical_success * 0.25
            + capability_coverage * 0.20
            + temporal_relevance * 0.10
            + transferability * 0.10;

        let recommendation = if total > 75.0 {
            "Highly suitable for this task".into()
        } else if total > 50.0 {
            format!(
                "Partially suitable. Missing: {}",
                missing.join(", ")
            )
        } else {
            "Not recommended for this task".into()
        };

        Ok(MemoryFitnessReport {
            total_score: total,
            task_relevance,
            historical_success,
            capability_coverage,
            temporal_relevance,
            transferability,
            missing_capabilities: missing,
            outdated_dependencies: outdated,
            recommendation,
        })
    }

    fn compute_task_relevance(&self, memory: &MemoryMetadata, task: &str) -> f64 {
        // TODO(milestone-4): embedding cosine similarity
        // Fallback: keyword overlap
        let task_words: Vec<&str> = task.split_whitespace().collect();
        let desc_words: Vec<String> = memory.description.split_whitespace().map(|s| s.to_lowercase()).collect();
        let overlap = task_words
            .iter()
            .filter(|w| desc_words.contains(&w.to_lowercase()))
            .count();
        ((overlap as f64 / task_words.len().max(1) as f64) * 100.0).min(100.0)
    }

    fn compute_capability_coverage(
        &self,
        memory: &MemoryMetadata,
        needed: &[String],
    ) -> (f64, Vec<String>) {
        if needed.is_empty() {
            return (50.0, vec![]);
        }
        let provided: Vec<String> = memory.capabilities.iter().map(|c| c.to_lowercase()).collect();
        let mut missing = vec![];
        let mut matched = 0;
        for cap in needed {
            if provided.iter().any(|p| p.contains(&cap.to_lowercase())) {
                matched += 1;
            } else {
                missing.push(cap.clone());
            }
        }
        let score = (matched as f64 / needed.len() as f64) * 100.0;
        (score, missing)
    }

    fn compute_temporal_relevance(&self, memory: &MemoryMetadata) -> (f64, Vec<String>) {
        let age_days = memory
            .last_verified_at
            .unwrap_or(memory.created_at)
            .signed_duration_since(chrono::Utc::now())
            .num_days()
            .unsigned_abs() as f64;

        // Half-life of 30 days
        let time_score = 100.0 * (0.5_f64).powf(age_days / 30.0);

        let outdated: Vec<String> = memory
            .dependencies
            .iter()
            .filter(|d| {
                d.current_version
                    .as_ref()
                    .is_some_and(|cv| cv != &d.version)
            })
            .map(|d| format!("{} ({} → {})", d.name, d.version, d.current_version.as_deref().unwrap_or("?")))
            .collect();

        let dep_penalty = outdated.len() as f64 * 10.0;
        ((time_score - dep_penalty).max(0.0), outdated)
    }

    fn compute_transferability(&self, memory: &MemoryMetadata) -> f64 {
        // More capabilities → more transferable
        let cap_score = (memory.capabilities.len() as f64 * 10.0).min(100.0);
        // Procedural/ToolChain are more transferable than Episodic
        let type_bonus = match memory.memory_type {
            MemoryType::ToolChain | MemoryType::PromptTemplate => 20.0,
            MemoryType::Procedural | MemoryType::Semantic => 10.0,
            _ => 0.0,
        };
        ((cap_score + type_bonus) / 2.0).min(100.0)
    }
}
