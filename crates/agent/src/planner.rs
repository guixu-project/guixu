// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use data_core::agent::contracts::{DelegatedDataTask, JobId};
use data_core::agent::memory::AgentMemory;
use data_core::types::{DataType, LatencyClass, SignalFamily, SkillCapability, SourceFamily};
use data_search::adapters::{load_data_skill_profiles, load_open_data_skills, DataSkillProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    OneShot {
        timeout_ms: u64,
        max_results: usize,
    },
    Continuous {
        watch_duration_secs: u64,
        evaluation_interval_ms: u64,
        opportunity_book_size: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConfig {
    pub hot_path: HotPathConfig,
    pub warm_path: WarmPathConfig,
    pub cold_path: ColdPathConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPathConfig {
    pub latency_budget_ms: u64,
    pub include_signal_families: Vec<SignalFamily>,
    pub exclude_llm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmPathConfig {
    pub latency_budget_ms: u64,
    pub llm_explanation: bool,
    pub cross_reference: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdPathConfig {
    pub latency_budget_ms: u64,
    pub historical_analysis: bool,
    pub backtest_enabled: bool,
}

impl Default for PathConfig {
    fn default() -> Self {
        Self {
            hot_path: HotPathConfig {
                latency_budget_ms: 1000,
                include_signal_families: vec![
                    SignalFamily::Mempool,
                    SignalFamily::Swap,
                    SignalFamily::WhaleFlow,
                ],
                exclude_llm: true,
            },
            warm_path: WarmPathConfig {
                latency_budget_ms: 30_000,
                llm_explanation: true,
                cross_reference: true,
            },
            cold_path: ColdPathConfig {
                latency_budget_ms: 300_000,
                historical_analysis: true,
                backtest_enabled: true,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnrichedSkillProfile {
    pub profile: DataSkillProfile,
    pub latency_class: Option<LatencyClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExecutionPlan {
    pub job_id: JobId,
    pub mode: PlanMode,
    pub stages: Vec<PlanStage>,
    pub stop_conditions: Vec<StopCondition>,
    pub budget_policy: BudgetPolicy,
    pub path_config: PathConfig,
    pub rationale: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStage {
    pub stage_id: String,
    pub name: String,
    pub strategy: ExecutionStrategy,
    pub tasks: Vec<PlannedSkillTask>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStrategy {
    Parallel,
    Sequential,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedSkillTask {
    pub task_id: String,
    pub skill_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_family: Option<SourceFamily>,
    pub operation: PlannedOperation,
    pub priority: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlannedOperation {
    Search,
    Evaluate,
    Download,
    ApprovalGate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopCondition {
    pub kind: StopConditionKind,
    pub threshold: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopConditionKind {
    EnoughCandidates,
    BudgetExceeded,
    TimeoutExceeded,
    CompatibleDatasetFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetPolicy {
    pub max_budget_usd: Option<f64>,
    pub free_first: bool,
    pub allow_purchase: bool,
}

pub struct Planner;

impl Planner {
    pub fn build(task: &DelegatedDataTask, memory: Option<&AgentMemory>) -> SkillExecutionPlan {
        let is_continuous = detect_continuous_monitoring_task(task);
        let mode = if is_continuous {
            PlanMode::Continuous {
                watch_duration_secs: 3600,
                evaluation_interval_ms: 5000,
                opportunity_book_size: 50,
            }
        } else {
            PlanMode::OneShot {
                timeout_ms: 15_000,
                max_results: 10,
            }
        };

        let path_config = PathConfig::default();

        let routed_skills = route_skills(task);

        let mut routed_skills = routed_skills;

        if let Some(mem) = memory {
            if let Some(mapping) = mem.get_best_mapping(&task.task.goal) {
                if let Some(pos) = routed_skills
                    .iter()
                    .position(|s| s.skill_id.eq_ignore_ascii_case(&mapping.source))
                {
                    let boosted = routed_skills.remove(pos);
                    routed_skills.insert(0, boosted);
                }
            }
        }

        let search_tasks: Vec<PlannedSkillTask> = routed_skills
            .iter()
            .enumerate()
            .map(|(i, skill)| PlannedSkillTask {
                task_id: format!("search-{}-{}", skill.skill_id, i),
                skill_id: skill.skill_id.clone(),
                source_family: Some(skill.source_family),
                operation: PlannedOperation::Search,
                priority: (i + 1) as u32,
                timeout_ms: if is_continuous { 30_000 } else { 15_000 },
            })
            .collect();

        let mut stages = vec![PlanStage {
            stage_id: "stage-search".into(),
            name: "search candidate data skills".into(),
            strategy: ExecutionStrategy::Parallel,
            tasks: search_tasks,
        }];

        stages.push(PlanStage {
            stage_id: "stage-evaluate".into(),
            name: "evaluate and select candidate".into(),
            strategy: ExecutionStrategy::Sequential,
            tasks: vec![PlannedSkillTask {
                task_id: "evaluate-best-candidate".into(),
                skill_id: "guixu-evaluator".into(),
                source_family: None,
                operation: PlannedOperation::Evaluate,
                priority: 1,
                timeout_ms: if is_continuous { 60_000 } else { 10_000 },
            }],
        });

        if task.policy.allow_purchase {
            stages.push(PlanStage {
                stage_id: "stage-approval".into(),
                name: "approval gate for paid path".into(),
                strategy: ExecutionStrategy::Sequential,
                tasks: vec![PlannedSkillTask {
                    task_id: "approval-gate".into(),
                    skill_id: "guixu-approval".into(),
                    source_family: None,
                    operation: PlannedOperation::ApprovalGate,
                    priority: 1,
                    timeout_ms: 60_000,
                }],
            });
        }

        let mut rationale = plan_rationale(task, &routed_skills);
        if let Some(mem) = memory {
            if let Some(mapping) = mem.get_best_mapping(&task.task.goal) {
                rationale.push(format!(
                    "memory_boost: {} (score={:.1} from previous task)",
                    mapping.source, mapping.score
                ));
            }
        }

        if is_continuous {
            rationale.push("mode: continuous monitoring".into());
        }

        SkillExecutionPlan {
            job_id: task.job_id.clone(),
            mode,
            stages,
            stop_conditions: vec![
                StopCondition {
                    kind: StopConditionKind::EnoughCandidates,
                    threshold: Some(5),
                },
                StopCondition {
                    kind: StopConditionKind::CompatibleDatasetFound,
                    threshold: Some(1),
                },
            ],
            budget_policy: BudgetPolicy {
                max_budget_usd: task.task.budget.as_ref().map(|b| b.amount),
                free_first: !task.policy.allow_purchase,
                allow_purchase: task.policy.allow_purchase,
            },
            path_config,
            rationale,
            created_at: Utc::now(),
        }
    }
}

fn detect_continuous_monitoring_task(task: &DelegatedDataTask) -> bool {
    let goal_lower = task.task.goal.to_ascii_lowercase();

    let continuous_keywords = [
        "monitor",
        "watch",
        "track",
        "subscribe",
        "stream",
        "real-time",
        "realtime",
        "live",
        "mempool",
        "whale",
        "swap",
        "bridge",
        "mint",
        "governance",
    ];

    let has_continuous_keyword = continuous_keywords.iter().any(|kw| goal_lower.contains(kw));

    let has_subscribe_capability = task
        .policy
        .required_capabilities
        .contains(&SkillCapability::Subscribe);

    has_continuous_keyword || has_subscribe_capability
}

fn route_skills(task: &DelegatedDataTask) -> Vec<DataSkillProfile> {
    let registry = load_data_skill_profiles().unwrap_or_default();
    let mut skills = filter_skill_profiles(task, registry);

    if skills.is_empty() && !task.policy.allowed_skill_ids.is_empty() {
        skills = synthetic_skill_profiles(&task.policy.allowed_skill_ids);
    } else if skills.is_empty() {
        skills = synthetic_skill_profiles(&default_skill_ids());
    }

    skills.sort_by(|left, right| {
        skill_score(task, right)
            .cmp(&skill_score(task, left))
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    skills
}

pub fn route_skills_for_signals(
    task: &DelegatedDataTask,
    path: LatencyClass,
) -> Vec<EnrichedSkillProfile> {
    let skills = load_open_data_skills().unwrap_or_default();

    let subscribe_skills: Vec<EnrichedSkillProfile> = skills
        .into_iter()
        .filter(|skill| skill.capabilities.subscribe)
        .map(|skill| {
            let caps = &skill.capabilities;
            let capabilities = {
                let mut list = Vec::new();
                if caps.search {
                    list.push(SkillCapability::Search);
                }
                if caps.lookup {
                    list.push(SkillCapability::Lookup);
                }
                if caps.download {
                    list.push(SkillCapability::Download);
                }
                if caps.schema_probe {
                    list.push(SkillCapability::SchemaProbe);
                }
                if caps.sample_preview {
                    list.push(SkillCapability::SamplePreview);
                }
                if caps.license_lookup {
                    list.push(SkillCapability::LicenseLookup);
                }
                if caps.query {
                    list.push(SkillCapability::Query);
                }
                if caps.subscribe {
                    list.push(SkillCapability::Subscribe);
                }
                if caps.backfill {
                    list.push(SkillCapability::Backfill);
                }
                if caps.decode {
                    list.push(SkillCapability::Decode);
                }
                if caps.simulate {
                    list.push(SkillCapability::Simulate);
                }
                if caps.execute {
                    list.push(SkillCapability::Execute);
                }
                list
            };
            EnrichedSkillProfile {
                profile: DataSkillProfile {
                    skill_id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    source_family: infer_source_family_from_id(&skill.id),
                    capabilities,
                    labels: skill.tags.clone(),
                    routing_hints: if skill.routing_hints.is_empty() {
                        skill.tags.clone()
                    } else {
                        skill.routing_hints.clone()
                    },
                },
                latency_class: skill.governance.latency_class,
            }
        })
        .collect();

    let mut filtered: Vec<EnrichedSkillProfile> = if task.policy.required_capabilities.is_empty() {
        subscribe_skills
    } else {
        subscribe_skills
            .into_iter()
            .filter(|e| {
                task.policy
                    .required_capabilities
                    .iter()
                    .all(|cap| e.profile.capabilities.contains(cap))
            })
            .collect()
    };

    filtered.sort_by(|left, right| {
        let left_score = latency_match_score(path, &left.latency_class);
        let right_score = latency_match_score(path, &right.latency_class);
        right_score
            .cmp(&left_score)
            .then_with(|| left.profile.skill_id.cmp(&right.profile.skill_id))
    });

    filtered
}

fn infer_source_family_from_id(skill_id: &str) -> SourceFamily {
    match skill_id.to_ascii_lowercase().as_str() {
        "kaggle" | "huggingface" | "guixu_hub" | "guixu-hub" | "guixu.market" => {
            SourceFamily::Marketplace
        }
        "arxiv" | "dblp" | "semantic_scholar" | "datacite_commons" => SourceFamily::Academic,
        "ipfs" | "bittorrent" => SourceFamily::Decentralized,
        "postgresql" | "duckdb" | "spark" | "flink" | "presto" => SourceFamily::DbCatalog,
        "local_file" | "localfile" => SourceFamily::Local,
        "google_dataset_search"
        | "pan_search"
        | "open_data_skill"
        | "defillama"
        | "rwa_xyz"
        | "rwaxyz"
        | "thegraph" => SourceFamily::WebRegistry,
        _ => SourceFamily::Custom,
    }
}

fn latency_match_score(preferred: LatencyClass, actual: &Option<LatencyClass>) -> i64 {
    match actual {
        None => 0,
        Some(class) if class == &preferred => 100,
        Some(class) => {
            let distance = (preferred as i32 - *class as i32).abs();
            100 - (distance as i64 * 30)
        }
    }
}

fn filter_skill_profiles(
    task: &DelegatedDataTask,
    profiles: Vec<DataSkillProfile>,
) -> Vec<DataSkillProfile> {
    let allowed_ids: Vec<String> = task
        .policy
        .allowed_skill_ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect();
    let blocked_ids: Vec<String> = task
        .policy
        .blocked_skill_ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect();

    profiles
        .into_iter()
        .filter(|profile| {
            allowed_ids.is_empty()
                || allowed_ids
                    .iter()
                    .any(|candidate| candidate == &profile.skill_id.to_ascii_lowercase())
        })
        .filter(|profile| {
            !blocked_ids
                .iter()
                .any(|candidate| candidate == &profile.skill_id.to_ascii_lowercase())
        })
        .filter(|profile| {
            task.policy.allowed_source_families.is_empty()
                || task
                    .policy
                    .allowed_source_families
                    .contains(&profile.source_family)
        })
        .filter(|profile| {
            task.policy
                .required_capabilities
                .iter()
                .all(|capability| profile.capabilities.contains(capability))
        })
        .collect()
}

fn synthetic_skill_profiles(skill_ids: &[String]) -> Vec<DataSkillProfile> {
    skill_ids
        .iter()
        .map(|skill_id| DataSkillProfile {
            skill_id: skill_id.clone(),
            name: skill_id.clone(),
            description: "synthetic skill profile created from task policy".into(),
            source_family: SourceFamily::Custom,
            capabilities: vec![SkillCapability::Search],
            labels: vec![],
            routing_hints: vec![],
        })
        .collect()
}

fn default_skill_ids() -> Vec<String> {
    vec![
        "huggingface".into(),
        "kaggle".into(),
        "ipfs".into(),
        "guixu_hub".into(),
    ]
}

fn skill_score(task: &DelegatedDataTask, profile: &DataSkillProfile) -> i64 {
    let mut score = 0_i64;
    let tokens = profile_tokens(profile);

    if profile.capabilities.contains(&SkillCapability::Search) {
        score += 100;
    } else {
        score -= 1_000;
    }

    if !task.policy.allow_purchase {
        score += match profile.source_family {
            SourceFamily::Academic => 24,
            SourceFamily::Decentralized => 20,
            SourceFamily::WebRegistry => 16,
            SourceFamily::Local => 12,
            SourceFamily::DbCatalog => 10,
            SourceFamily::Marketplace => 6,
            SourceFamily::Custom => 8,
        };
    }

    if let Some(task_type) = task.task.task_type.as_deref() {
        score += keyword_score(&tokens, task_type_keywords(task_type), 8);
        score += source_family_bias(task_type, profile.source_family);
    }

    for modality in &task.task.required_modalities {
        score += keyword_score(&tokens, modality_keywords(*modality), 5);
    }

    score += keyword_score(
        &tokens,
        &task
            .task
            .required_columns
            .iter()
            .map(|column| column.as_str())
            .collect::<Vec<_>>(),
        2,
    );

    score
}

fn source_family_bias(task_type: &str, source_family: SourceFamily) -> i64 {
    match task_type {
        "classification" | "detection" | "segmentation" => match source_family {
            SourceFamily::Marketplace | SourceFamily::WebRegistry => 8,
            SourceFamily::Decentralized => 4,
            _ => 0,
        },
        "forecasting" | "ranking" => match source_family {
            SourceFamily::WebRegistry | SourceFamily::DbCatalog => 8,
            SourceFamily::Marketplace => 4,
            _ => 0,
        },
        "retrieval" | "summarization" | "evaluation" => match source_family {
            SourceFamily::Academic | SourceFamily::WebRegistry => 8,
            _ => 0,
        },
        _ => 0,
    }
}

fn profile_tokens(profile: &DataSkillProfile) -> Vec<String> {
    let mut tokens = Vec::new();
    tokens.push(profile.skill_id.to_ascii_lowercase());
    tokens.push(profile.name.to_ascii_lowercase());
    tokens.push(profile.description.to_ascii_lowercase());
    tokens.extend(
        profile
            .labels
            .iter()
            .map(|label| label.to_ascii_lowercase()),
    );
    tokens.extend(
        profile
            .routing_hints
            .iter()
            .map(|hint| hint.to_ascii_lowercase()),
    );
    tokens
}

fn keyword_score(tokens: &[String], keywords: &[&str], weight: i64) -> i64 {
    keywords
        .iter()
        .filter(|keyword| {
            let keyword = keyword.to_ascii_lowercase();
            tokens.iter().any(|token| token.contains(&keyword))
        })
        .count() as i64
        * weight
}

fn task_type_keywords(task_type: &str) -> &'static [&'static str] {
    match task_type {
        "classification" | "detection" | "segmentation" => {
            &["vision", "image", "annotation", "multimodal", "competition"]
        }
        "forecasting" | "ranking" => &[
            "finance",
            "market",
            "analytics",
            "tabular",
            "time_series",
            "timeseries",
            "database",
            "catalog",
            "rwa",
            "defi",
        ],
        "retrieval" | "summarization" | "evaluation" => &[
            "academic",
            "papers",
            "doi",
            "text",
            "nlp",
            "search",
            "preprints",
        ],
        _ => &[],
    }
}

fn modality_keywords(modality: DataType) -> &'static [&'static str] {
    match modality {
        DataType::Tabular => &["tabular", "analytics", "database", "catalog", "market"],
        DataType::Video => &["video", "vision", "multimodal"],
        DataType::Image => &["image", "vision", "multimodal"],
        DataType::Audio => &["audio", "speech"],
        DataType::Text => &["text", "nlp", "papers", "academic", "doi"],
    }
}

fn plan_rationale(task: &DelegatedDataTask, skills: &[DataSkillProfile]) -> Vec<String> {
    let mut reasons = vec![format!(
        "task_type={}",
        task.task
            .task_type
            .clone()
            .unwrap_or_else(|| "unknown".into())
    )];
    if !task.policy.allow_purchase {
        reasons.push("free-first policy enabled".into());
    }
    if let Some(budget) = &task.task.budget {
        reasons.push(format!("budget={} {}", budget.amount, budget.currency));
    }
    if !task.policy.allowed_skill_ids.is_empty() {
        reasons.push(format!(
            "allowed_skill_ids={}",
            task.policy.allowed_skill_ids.join(",")
        ));
    }
    if !task.policy.allowed_source_families.is_empty() {
        reasons.push(format!(
            "allowed_source_families={}",
            task.policy
                .allowed_source_families
                .iter()
                .map(|family| format!("{family:?}").to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    reasons.push(format!("skill_count={}", skills.len()));
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::agent::contracts::{
        Budget, DataTaskSpec, DelegatedDataTask, HostContext, HostKind, TaskPolicy,
        WorkspaceContext,
    };
    use data_core::agent::memory::AgentMemory;

    fn test_task() -> DelegatedDataTask {
        DelegatedDataTask {
            job_id: JobId::new(),
            host: HostContext {
                kind: HostKind::openclaw(),
                session_key: "agent:main:main".into(),
                run_id: None,
            },
            workspace: WorkspaceContext {
                id: "repo:guixu".into(),
                root_hint: None,
            },
            task: DataTaskSpec {
                goal: "find a chart QA dataset".into(),
                task_type: Some("evaluation".into()),
                required_modalities: vec![DataType::Text],
                required_columns: vec!["question".into(), "answer".into()],
                budget: Some(Budget::usd(10.0)),
            },
            policy: TaskPolicy {
                allow_purchase: false,
                allowed_skill_ids: vec!["datacite_commons".into(), "arxiv".into()],
                blocked_skill_ids: vec![],
                allowed_source_families: vec![],
                required_capabilities: vec![SkillCapability::Search],
                require_license_review: true,
            },
            desired_outputs: vec![],
            created_at: Utc::now(),
        }
    }

    fn continuous_monitor_task() -> DelegatedDataTask {
        DelegatedDataTask {
            job_id: JobId::new(),
            host: HostContext {
                kind: HostKind::openclaw(),
                session_key: "agent:main:main".into(),
                run_id: None,
            },
            workspace: WorkspaceContext {
                id: "repo:guixu".into(),
                root_hint: None,
            },
            task: DataTaskSpec {
                goal: "monitor mempool for large ETH transfers".into(),
                task_type: Some("monitoring".into()),
                required_modalities: vec![DataType::Text],
                required_columns: vec!["tx_hash".into(), "value".into()],
                budget: Some(Budget::usd(50.0)),
            },
            policy: TaskPolicy {
                allow_purchase: true,
                allowed_skill_ids: vec!["ethereum_mempool".into()],
                blocked_skill_ids: vec![],
                allowed_source_families: vec![],
                required_capabilities: vec![SkillCapability::Subscribe],
                require_license_review: false,
            },
            desired_outputs: vec![],
            created_at: Utc::now(),
        }
    }

    #[test]
    fn planner_builds_parallel_search_stage() {
        let task = test_task();
        let plan = Planner::build(&task, None);
        assert_eq!(
            plan.stages.first().unwrap().strategy,
            ExecutionStrategy::Parallel
        );
        assert!(plan
            .stages
            .first()
            .unwrap()
            .tasks
            .iter()
            .all(|task| !task.skill_id.is_empty()));
    }

    #[test]
    fn planner_boosts_skill_from_memory() {
        let task = test_task();

        let plan_no_mem = Planner::build(&task, None);
        let first_no_mem = &plan_no_mem.stages[0].tasks[0].skill_id;

        let mut memory = AgentMemory::default();
        memory.record_successful_mapping(
            "find a chart QA dataset",
            "arxiv:2301.00001",
            "arxiv",
            95.0,
        );
        let plan_with_mem = Planner::build(&task, Some(&memory));
        let first_with_mem = &plan_with_mem.stages[0].tasks[0].skill_id;

        assert_eq!(first_with_mem, "arxiv");
        assert!(plan_with_mem
            .rationale
            .iter()
            .any(|r| r.contains("memory_boost")));

        let _ = first_no_mem;
    }

    #[test]
    fn planner_oneshot_mode_by_default() {
        let task = test_task();
        let plan = Planner::build(&task, None);
        match plan.mode {
            PlanMode::OneShot {
                timeout_ms,
                max_results,
            } => {
                assert_eq!(timeout_ms, 15_000);
                assert_eq!(max_results, 10);
            }
            PlanMode::Continuous { .. } => {
                panic!("Expected OneShot mode for non-continuous task");
            }
        }
    }

    #[test]
    fn planner_continuous_mode_for_monitoring_task() {
        let task = continuous_monitor_task();
        let plan = Planner::build(&task, None);
        match plan.mode {
            PlanMode::OneShot { .. } => {
                panic!("Expected Continuous mode for monitoring task");
            }
            PlanMode::Continuous {
                watch_duration_secs,
                evaluation_interval_ms,
                opportunity_book_size,
            } => {
                assert_eq!(watch_duration_secs, 3600);
                assert_eq!(evaluation_interval_ms, 5000);
                assert_eq!(opportunity_book_size, 50);
            }
        }
        assert!(plan.rationale.iter().any(|r| r.contains("continuous")));
    }

    #[test]
    fn detect_continuous_monitoring_task_keywords() {
        let monitor_task = DelegatedDataTask {
            job_id: JobId::new(),
            host: HostContext {
                kind: HostKind::openclaw(),
                session_key: "agent:main:main".into(),
                run_id: None,
            },
            workspace: WorkspaceContext {
                id: "repo:guixu".into(),
                root_hint: None,
            },
            task: DataTaskSpec {
                goal: "watch whale wallets for large swaps".into(),
                task_type: None,
                required_modalities: vec![],
                required_columns: vec![],
                budget: None,
            },
            policy: TaskPolicy {
                allow_purchase: false,
                allowed_skill_ids: vec![],
                blocked_skill_ids: vec![],
                allowed_source_families: vec![],
                required_capabilities: vec![],
                require_license_review: false,
            },
            desired_outputs: vec![],
            created_at: Utc::now(),
        };
        assert!(detect_continuous_monitoring_task(&monitor_task));
    }

    #[test]
    fn detect_continuous_monitoring_task_by_subscribe_capability() {
        let task = DelegatedDataTask {
            job_id: JobId::new(),
            host: HostContext {
                kind: HostKind::openclaw(),
                session_key: "agent:main:main".into(),
                run_id: None,
            },
            workspace: WorkspaceContext {
                id: "repo:guixu".into(),
                root_hint: None,
            },
            task: DataTaskSpec {
                goal: "find datasets".into(),
                task_type: None,
                required_modalities: vec![],
                required_columns: vec![],
                budget: None,
            },
            policy: TaskPolicy {
                allow_purchase: false,
                allowed_skill_ids: vec![],
                blocked_skill_ids: vec![],
                allowed_source_families: vec![],
                required_capabilities: vec![SkillCapability::Subscribe],
                require_license_review: false,
            },
            desired_outputs: vec![],
            created_at: Utc::now(),
        };
        assert!(detect_continuous_monitoring_task(&task));
    }

    #[test]
    fn path_config_has_default_values() {
        let config = PathConfig::default();
        assert_eq!(config.hot_path.latency_budget_ms, 1000);
        assert!(config.hot_path.exclude_llm);
        assert_eq!(config.warm_path.latency_budget_ms, 30_000);
        assert!(config.warm_path.llm_explanation);
        assert!(config.warm_path.cross_reference);
        assert_eq!(config.cold_path.latency_budget_ms, 300_000);
        assert!(config.cold_path.historical_analysis);
        assert!(config.cold_path.backtest_enabled);
    }

    #[test]
    fn plan_mode_serialization() {
        let oneshot = PlanMode::OneShot {
            timeout_ms: 15000,
            max_results: 10,
        };
        let json = serde_json::to_string(&oneshot).unwrap();
        assert!(json.contains("one_shot"));
        let deserialized: PlanMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, oneshot);

        let continuous = PlanMode::Continuous {
            watch_duration_secs: 3600,
            evaluation_interval_ms: 5000,
            opportunity_book_size: 50,
        };
        let json = serde_json::to_string(&continuous).unwrap();
        assert!(json.contains("continuous"));
        let deserialized: PlanMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, continuous);
    }

    #[test]
    fn path_config_serialization() {
        let config = PathConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("hot_path"));
        assert!(json.contains("warm_path"));
        assert!(json.contains("cold_path"));
        let deserialized: PathConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.hot_path.latency_budget_ms,
            config.hot_path.latency_budget_ms
        );
    }

    #[test]
    fn latency_match_score_prefers_matching_class() {
        assert_eq!(
            latency_match_score(LatencyClass::Hot, &Some(LatencyClass::Hot)),
            100
        );
        assert_eq!(
            latency_match_score(LatencyClass::Warm, &Some(LatencyClass::Warm)),
            100
        );
        assert_eq!(
            latency_match_score(LatencyClass::Cold, &Some(LatencyClass::Cold)),
            100
        );
    }

    #[test]
    fn latency_match_score_penalizes_non_matching() {
        let hot_score = latency_match_score(LatencyClass::Hot, &Some(LatencyClass::Cold));
        let warm_score = latency_match_score(LatencyClass::Warm, &Some(LatencyClass::Hot));
        assert!(hot_score < 100);
        assert!(warm_score < 100);
    }

    #[test]
    fn latency_match_score_handles_none() {
        assert_eq!(latency_match_score(LatencyClass::Hot, &None), 0);
    }

    #[test]
    fn skill_execution_plan_includes_mode_and_path_config() {
        let task = test_task();
        let plan = Planner::build(&task, None);
        assert!(plan.path_config.hot_path.latency_budget_ms > 0);
        match plan.mode {
            PlanMode::OneShot { .. } => {}
            PlanMode::Continuous { .. } => panic!("Expected OneShot"),
        }
    }
}
