use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Structured query profile produced before discovery.
///
/// This is the stable intermediate representation between
/// natural-language input and downstream discovery logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryProfile {
    pub raw_query: String,
    pub task_type: Option<String>,
    #[serde(default)]
    pub task_description: Option<String>,
    pub target_entity: Option<String>,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub data_standard: DataStandard,
    #[serde(default)]
    pub user_profile: UserProfile,
}

impl Default for QueryProfile {
    fn default() -> Self {
        Self {
            raw_query: String::new(),
            task_type: None,
            task_description: None,
            target_entity: None,
            keywords: Vec::new(),
            data_standard: DataStandard::default(),
            user_profile: UserProfile::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataStandard {
    pub sample_unit: String,
    #[serde(
        default = "default_budget",
        deserialize_with = "deserialize_budget_or_default"
    )]
    pub budget: String,
    #[serde(
        default = "default_max_latency_secs",
        deserialize_with = "deserialize_f64_or_default"
    )]
    pub max_latency_secs: f64,
    #[serde(default, deserialize_with = "deserialize_u64_or_default")]
    pub min_dataset_size_bytes: u64,
    #[serde(default, deserialize_with = "deserialize_u64_or_default")]
    pub max_dataset_size_bytes: u64,
    #[serde(default)]
    pub canonical_columns: Vec<String>,
    #[serde(default)]
    pub extra_columns: Vec<String>,
}

impl Default for DataStandard {
    fn default() -> Self {
        Self {
            sample_unit: String::new(),
            budget: default_budget(),
            max_latency_secs: default_max_latency_secs(),
            min_dataset_size_bytes: 0,
            max_dataset_size_bytes: 0,
            canonical_columns: default_canonical_columns(),
            extra_columns: default_extra_columns(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfile {
    pub cpu: CpuProfile,
    #[serde(default)]
    pub gpus: Vec<GpuProfile>,
}

/// CPU details exposed to the intent parser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuProfile {
    pub architecture: String,
    pub logical_cores: usize,
    pub model: Option<String>,
}

/// GPU details exposed to the intent parser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuProfile {
    pub vendor: Option<String>,
    pub model: String,
}

/// Produces a structured profile from a raw user query.
#[async_trait::async_trait]
pub trait QueryProfiler: Send + Sync {
    async fn profile(&self, query: &str) -> Result<QueryProfile>;
}

/// Parses natural language queries into structured intents.
/// Uses DeepSeek with a local rule-based fallback when unavailable.
#[derive(Debug, Clone)]
pub struct IntentParser {
    client: Client,
    config: IntentParserConfig,
}

const MEMORY_SEARCH_LIMIT: usize = 6;
const DEFAULT_MAX_LATENCY_SECS: f64 = 30.0;
const CONTEXT_WINDOW_BYTES: usize = 48;
const DEFAULT_BUDGET: &str = "0 USD";

#[derive(Debug, Clone)]
pub struct IntentParserConfig {
    pub api_key: Option<String>,
    pub api_base: String,
    pub model: String,
    pub timeout: Duration,
    pub proxy_url: String,
}

impl Default for IntentParserConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl IntentParserConfig {
    pub fn from_env() -> Self {
        Self {
            api_key: load_setting_env_value("DEEPSEEK_API_KEY"),
            api_base: "https://api.deepseek.com".to_string(),
            model: std::env::var("DEEPSEEK_MODEL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "deepseek-chat".to_string()),
            timeout: Duration::from_secs(15),
            proxy_url: std::env::var("GUIXU_DEEPSEEK_PROXY_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/search/deepseek".into()),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct IntentContext {
    user_profile: UserProfile,
    bandwidth_bytes_per_sec: u64,
    related_memories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct QueryTransferConstraints {
    max_latency_secs: f64,
    min_dataset_size_bytes: u64,
    max_dataset_size_bytes: u64,
}

#[derive(Debug, Clone)]
struct NumericUnitMatch {
    value: f64,
    unit: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LlmScalarValue {
    Number(f64),
    Text(String),
}

impl Default for IntentParser {
    fn default() -> Self {
        Self::new(IntentParserConfig::default())
    }
}

impl IntentParser {
    pub fn new(config: IntentParserConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.timeout)
            .user_agent("guixu/0.1")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, config }
    }

    /// Produce a structured query profile from raw user input.
    pub async fn profile(&self, query: &str) -> Result<QueryProfile> {
        let has_key = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .is_some();

        let query_owned = query.to_string();
        let intent_context =
            tokio::task::spawn_blocking(move || collect_intent_context(&query_owned, true))
                .await
                .unwrap_or_else(|_| IntentContext::default());

        if has_key {
            eprintln!("calling DeepSeek API for intent parsing");
            let profile = self
                .profile_with_deepseek(
                    query,
                    &intent_context.user_profile,
                    intent_context.bandwidth_bytes_per_sec,
                    &intent_context.related_memories,
                )
                .await?;
            eprintln!(
                "intent profile:\n{}",
                serde_json::to_string_pretty(&profile)?
            );
            Ok(profile)
        } else {
            eprintln!("no DEEPSEEK_API_KEY, using guixu.org proxy for intent parsing");
            self.profile_via_proxy(
                query,
                &intent_context.user_profile,
                intent_context.bandwidth_bytes_per_sec,
                &intent_context.related_memories,
            )
            .await
        }
    }

    /// Parse a natural language query into structured intent.
    pub async fn parse(&self, query: &str) -> Result<QueryProfile> {
        self.profile(query).await
    }

    async fn profile_with_deepseek(
        &self,
        query: &str,
        user_profile: &UserProfile,
        bandwidth_bytes_per_sec: u64,
        related_memories: &[String],
    ) -> Result<QueryProfile> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing DEEPSEEK_API_KEY"))?;
        let endpoint = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let request = self.build_deepseek_request(query, user_profile, related_memories)?;

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await
            .context("send DeepSeek chat completion request")?
            .error_for_status()
            .context("DeepSeek chat completion returned error status")?
            .json::<DeepSeekChatResponse>()
            .await
            .context("parse DeepSeek chat completion response")?;
        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| anyhow!("DeepSeek returned an empty intent payload"))?;
        self.profile_from_deepseek_content(query, user_profile, bandwidth_bytes_per_sec, &content)
    }

    #[cfg(test)]
    pub(crate) fn build_deepseek_request_json(
        &self,
        query: &str,
        user_profile: &UserProfile,
        related_memories: &[String],
    ) -> Result<serde_json::Value> {
        let request = self.build_deepseek_request(query, user_profile, related_memories)?;
        Ok(serde_json::to_value(request)?)
    }

    pub(crate) fn profile_from_deepseek_content(
        &self,
        query: &str,
        user_profile: &UserProfile,
        bandwidth_bytes_per_sec: u64,
        content: &str,
    ) -> Result<QueryProfile> {
        let llm_profile = serde_json::from_str::<LlmIntentProfile>(content)
            .with_context(|| format!("parse DeepSeek intent JSON: {content}"))?;
        let top_level_max_latency_secs =
            normalize_latency_secs(llm_profile.max_latency_secs.clone());
        let mut data_standard = normalize_data_standard(llm_profile.data_standard);
        let max_latency_secs = top_level_max_latency_secs.or(Some(data_standard.max_latency_secs));
        let transfer_constraints =
            derive_query_transfer_constraints(query, max_latency_secs, bandwidth_bytes_per_sec);
        if llm_profile.budget.is_some() {
            data_standard.budget = normalize_budget(llm_profile.budget);
        }
        data_standard.max_latency_secs = transfer_constraints.max_latency_secs;
        data_standard.min_dataset_size_bytes = transfer_constraints.min_dataset_size_bytes;
        data_standard.max_dataset_size_bytes = transfer_constraints.max_dataset_size_bytes;

        Ok(QueryProfile {
            raw_query: query.to_string(),
            task_type: normalize_optional(llm_profile.task_type),
            task_description: normalize_optional(llm_profile.task_description)
                .or_else(|| Some(fallback_task_description(query))),
            target_entity: normalize_optional(llm_profile.target_entity),
            keywords: normalize_keywords(llm_profile.keywords),
            data_standard,
            user_profile: user_profile.clone(),
        })
    }

    fn build_deepseek_request(
        &self,
        query: &str,
        user_profile: &UserProfile,
        related_memories: &[String],
    ) -> Result<DeepSeekChatRequest> {
        let prompt = build_user_prompt(query, user_profile, related_memories)?;
        Ok(DeepSeekChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                DeepSeekMessage {
                    role: "system",
                    content: INTENT_SYSTEM_PROMPT.to_string(),
                },
                DeepSeekMessage {
                    role: "user",
                    content: prompt,
                },
            ],
            response_format: DeepSeekResponseFormat {
                kind: "json_object".to_string(),
            },
            temperature: 0.0,
            max_tokens: 256,
            stream: false,
        })
    }

    async fn profile_via_proxy(
        &self,
        query: &str,
        user_profile: &UserProfile,
        bandwidth_bytes_per_sec: u64,
        related_memories: &[String],
    ) -> Result<QueryProfile> {
        let request = self.build_deepseek_request(query, user_profile, related_memories)?;
        let response = tokio::time::timeout(
            self.config.timeout,
            self.client
                .post(&self.config.proxy_url)
                .json(&request)
                .send(),
        )
        .await
        .map_err(|_| anyhow!("deepseek proxy: timeout"))?
        .context("send DeepSeek proxy request")?
        .error_for_status()
        .context("DeepSeek proxy returned error status")?
        .json::<DeepSeekChatResponse>()
        .await
        .context("parse DeepSeek proxy response")?;

        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| anyhow!("DeepSeek proxy returned an empty intent payload"))?;
        self.profile_from_deepseek_content(query, user_profile, bandwidth_bytes_per_sec, &content)
    }
}

#[async_trait::async_trait]
impl QueryProfiler for IntentParser {
    async fn profile(&self, query: &str) -> Result<QueryProfile> {
        IntentParser::profile(self, query).await
    }
}

const INTENT_SYSTEM_PROMPT: &str = r#"You are an intent parser for dataset search.
Return valid json only, with no markdown and no extra commentary.

Use this exact json schema:
{
  "task_type": "string or null",
  "task_description": "string or null",
  "target_entity": "string or null",
  "keywords": ["lowercase keyword"],
  "data_standard": {
    "sample_unit": "string",
    "budget": "string or null",
    "max_latency_secs": "number or null",
    "min_dataset_size_bytes": "integer",
    "max_dataset_size_bytes": "integer",
    "canonical_columns": ["string"],
    "extra_columns": ["string"]
  }
}

Rules:
- First infer the user's likely task description from the natural-language query and any relevant user memories.
- task_type must be a concise task label such as classification, forecasting, detection, ranking, retrieval, segmentation, generation, summarization, or evaluation.
- task_description must be a detailed natural-language description of what the user is trying to accomplish with the data.
- data_standard.budget must preserve the amount together with its explicit unit or currency, for example "$20", "20 USD", "100 RMB", or "0.05 ETH".
- If the query gives a bare budget number without a currency, normalize it as USD, for example "20 USD".
- If the query does not explicitly mention a budget, data_standard.budget must be "0 USD".
- Do not infer a budget from unrelated numbers such as years, row counts, image resolutions, hardware specs, or model names.
- data_standard.max_latency_secs must be the maximum acceptable waiting time for getting the dataset only when the query explicitly states one.
- Convert data_standard.max_latency_secs to seconds as a plain number. If the query does not specify such a limit, use null.
- Do not infer data_standard.max_latency_secs from time spans that describe the dataset contents themselves.
- target_entity must be the main subject or object of the requested dataset, kept short, and should prefer the generic publicly searchable category over a private instance name when the memories make that category clear.
- If the query names a private entity such as a pet or person, resolve it to the relevant dataset-search target when possible, for example a named cat should produce target_entity "cat" rather than the pet's name.
- keywords must be extracted only from the original query text or from relevant user memories that are clearly linked to the query by named-entity matches, do not invent keywords
- keywords must answer ONE question: "What terms would I type into Kaggle / HuggingFace search bar to find the dataset I need?"
- Only include terms that describe the CONTENT of the desired dataset (objects, domains, sensor types, data modalities).
- EXCLUDE all of the following — they never help find datasets:
  • task/model words: classifier, classification, detection, segmentation, model, network, train, predict
  • generic media words: image, photo, picture, video, frame (unless the query is specifically about a dataset OF photos/videos as subject matter)
  • action/intent words: write, build, create, check, detect, identify, recognize
- After resolving private entities to public categories via memories, the private name (e.g. "caesar") must NOT appear in keywords.
- Maximum 5 keywords. Fewer is better. If one keyword suffices, use one.
- When in doubt about whether to include a keyword, leave it out.
- data_standard carries dataset schema preferences plus query-side transfer constraints.
- data_standard.sample_unit is the only hard dataset constraint used for search filtering before scoring.
- data_standard.sample_unit should use broad units such as image, video, text, tabular, or audio.
- If the user is asking for an image classifier or image detector, sample_unit should normally be "image".
- data_standard.min_dataset_size_bytes and data_standard.max_dataset_size_bytes should be 0 in the LLM output; the application will compute them.
- data_standard.canonical_columns must always include exactly these fields: sample_id, label.
- data_standard.extra_columns must always include exactly these fields: timestamp.
- canonical_columns and extra_columns must be string arrays, not objects.
- If you do not have enough information for data_standard, leave sample_unit as an empty string.
Examples:
  Query: "write an image classifier that checks whether my cat is in the photo taken by my house monitor"
  Good keywords: ["cat"]
  Bad keywords: ["image", "classifier", "cat", "photo", "house", "monitor"]
  Budget: 0

  Query: "find a cat dataset under $20 for classification"
  Good keywords: ["cat"]
  data_standard.budget: "$20"

  Query: "build a model to detect lung nodules in chest CT scans"
  Good keywords: ["lung nodule", "chest ct"]
  Bad keywords: ["detect", "model", "build", "scan"]
- Query: "find a dataset I can download within 45 seconds"
  data_standard.max_latency_secs: 45
- If a scalar field other than data_standard.budget is unknown, use null. data_standard.budget must always be present as a string.
- The provided hardware profile and user memories are context only; do not copy them verbatim into the json output."#;

#[derive(Debug, Serialize)]
struct DeepSeekChatRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    response_format: DeepSeekResponseFormat,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct DeepSeekMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct DeepSeekResponseFormat {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChatResponse {
    choices: Vec<DeepSeekChoice>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoice {
    message: DeepSeekChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmIntentProfile {
    task_type: Option<String>,
    task_description: Option<String>,
    budget: Option<LlmScalarValue>,
    max_latency_secs: Option<LlmScalarValue>,
    target_entity: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    data_standard: DataStandard,
}

fn build_user_prompt(
    query: &str,
    user_profile: &UserProfile,
    related_memories: &[String],
) -> Result<String> {
    let hardware_json = serde_json::to_string_pretty(user_profile)?;
    let memories_section = format_memory_context(related_memories);
    Ok(format!(
        "Extract dataset search intent as json.\n\
         Query:\n{query}\n\n\
         Relevant user memories:\n{memories_section}\n\n\
         Hardware profile:\n{hardware_json}\n"
    ))
}

fn default_max_latency_secs() -> f64 {
    DEFAULT_MAX_LATENCY_SECS
}

fn default_budget() -> String {
    DEFAULT_BUDGET.to_string()
}

fn deserialize_budget_or_default<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_budget))
}

fn deserialize_f64_or_default<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or_else(default_max_latency_secs))
}

fn deserialize_u64_or_default<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<u64>::deserialize(deserializer)?.unwrap_or_default())
}

fn derive_query_transfer_constraints(
    query: &str,
    llm_max_latency_secs: Option<f64>,
    bandwidth_bytes_per_sec: u64,
) -> QueryTransferConstraints {
    let max_latency_secs = llm_max_latency_secs
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_else(default_max_latency_secs);
    let min_dataset_size_bytes = extract_min_dataset_size_bytes(query).unwrap_or(0);
    let max_dataset_size_bytes =
        compute_max_dataset_size_bytes(max_latency_secs, bandwidth_bytes_per_sec);

    QueryTransferConstraints {
        max_latency_secs,
        min_dataset_size_bytes,
        max_dataset_size_bytes,
    }
}

fn extract_min_dataset_size_bytes(query: &str) -> Option<u64> {
    extract_numeric_unit_matches(query)
        .into_iter()
        .filter_map(|capture| {
            let size_bytes = parse_size_bytes(&capture.unit, capture.value)?;
            if has_minimum_size_context(query, capture.start, capture.end, &capture.unit) {
                Some(size_bytes)
            } else {
                None
            }
        })
        .max()
}

fn compute_max_dataset_size_bytes(max_latency_secs: f64, bandwidth_bytes_per_sec: u64) -> u64 {
    if !max_latency_secs.is_finite() || max_latency_secs <= 0.0 || bandwidth_bytes_per_sec == 0 {
        return 0;
    }

    let max_bytes = max_latency_secs * bandwidth_bytes_per_sec as f64;
    if max_bytes >= u64::MAX as f64 {
        u64::MAX
    } else {
        max_bytes.floor() as u64
    }
}

fn normalize_budget(value: Option<LlmScalarValue>) -> String {
    match value {
        Some(LlmScalarValue::Text(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                default_budget()
            } else {
                trimmed.to_string()
            }
        }
        Some(LlmScalarValue::Number(number)) => format_numeric_budget(number),
        None => default_budget(),
    }
}

fn normalize_latency_secs(value: Option<LlmScalarValue>) -> Option<f64> {
    match value {
        Some(LlmScalarValue::Number(number)) if number.is_finite() => Some(number),
        Some(LlmScalarValue::Text(text)) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn format_numeric_budget(value: f64) -> String {
    if !value.is_finite() {
        return default_budget();
    }

    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0} USD")
    } else {
        format!("{value} USD")
    }
}

fn extract_numeric_unit_matches(query: &str) -> Vec<NumericUnitMatch> {
    let mut matches = Vec::new();
    let mut chars = query.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if !ch.is_ascii_digit() {
            continue;
        }

        let mut number = String::from(ch);
        let mut number_end = start + ch.len_utf8();
        let mut seen_dot = false;

        while let Some(&(idx, next)) = chars.peek() {
            if next.is_ascii_digit() {
                number.push(next);
                number_end = idx + next.len_utf8();
                chars.next();
                continue;
            }
            if next == ',' {
                number_end = idx + next.len_utf8();
                chars.next();
                continue;
            }
            if next == '.' && !seen_dot {
                seen_dot = true;
                number.push(next);
                number_end = idx + next.len_utf8();
                chars.next();
                continue;
            }
            break;
        }

        while let Some(&(idx, next)) = chars.peek() {
            if next.is_whitespace() || next == '-' {
                number_end = idx + next.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let mut unit = String::new();
        let mut unit_end = number_end;
        while let Some(&(idx, next)) = chars.peek() {
            if is_unit_char(next) {
                unit.push(next);
                unit_end = idx + next.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        if unit.is_empty() {
            continue;
        }

        let Ok(value) = number.replace(',', "").parse::<f64>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }

        matches.push(NumericUnitMatch {
            value,
            unit: unit.to_lowercase(),
            start,
            end: unit_end,
        });
    }

    matches
}

fn parse_size_bytes(unit: &str, value: f64) -> Option<u64> {
    let normalized = strip_size_suffixes(unit.trim().to_lowercase());
    let multiplier =
        if normalized == "b" || normalized.contains("byte") || normalized.contains("字节") {
            1_f64
        } else if normalized == "kib" || normalized == "ki" || normalized.contains("kibibyte") {
            1024_f64
        } else if normalized == "kb" || normalized == "k" || normalized.contains("kilobyte") {
            1_000_f64
        } else if normalized == "mib" || normalized == "mi" || normalized.contains("mebibyte") {
            1024_f64.powi(2)
        } else if normalized == "mb"
            || normalized == "m"
            || normalized.contains("megabyte")
            || normalized.contains("兆字节")
        {
            1_000_f64.powi(2)
        } else if normalized == "gib" || normalized == "gi" || normalized.contains("gibibyte") {
            1024_f64.powi(3)
        } else if normalized == "gb"
            || normalized == "g"
            || normalized.contains("gigabyte")
            || normalized.contains("吉字节")
        {
            1_000_f64.powi(3)
        } else if normalized == "tib" || normalized == "ti" || normalized.contains("tebibyte") {
            1024_f64.powi(4)
        } else if normalized == "tb"
            || normalized == "t"
            || normalized.contains("terabyte")
            || normalized.contains("太字节")
        {
            1_000_f64.powi(4)
        } else {
            return None;
        };

    let size_bytes = value * multiplier;
    if !size_bytes.is_finite() || size_bytes <= 0.0 {
        return None;
    }
    if size_bytes >= u64::MAX as f64 {
        Some(u64::MAX)
    } else {
        Some(size_bytes.floor() as u64)
    }
}

fn has_minimum_size_context(query: &str, start: usize, end: usize, unit: &str) -> bool {
    let before = context_slice_before(query, start).to_lowercase();
    let after = context_slice_after(query, end).to_lowercase();
    let around = format!("{before} {after}");
    let unit = unit.to_lowercase();

    contains_any(
        &around,
        &[
            "at least",
            "minimum",
            "min ",
            "min:",
            ">=",
            "not less than",
            "no smaller than",
            "over",
            "greater than",
            "至少",
            "不少于",
            "最小",
            "下限",
            "不低于",
            "大于",
            "超过",
            "以上",
        ],
    ) || contains_any(&unit, &["以上"])
}

fn strip_size_suffixes(mut unit: String) -> String {
    for suffix in ["以上", "左右", "约", "的"] {
        while unit.ends_with(suffix) {
            unit.truncate(unit.len() - suffix.len());
        }
    }
    unit
}

fn context_slice_before(query: &str, start: usize) -> &str {
    let window_start =
        clamp_to_char_boundary_left(query, start.saturating_sub(CONTEXT_WINDOW_BYTES));
    &query[window_start..start]
}

fn context_slice_after(query: &str, end: usize) -> &str {
    let window_end = clamp_to_char_boundary_right(
        query,
        end.saturating_add(CONTEXT_WINDOW_BYTES).min(query.len()),
    );
    &query[end..window_end]
}

fn clamp_to_char_boundary_left(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn clamp_to_char_boundary_right(value: &str, mut index: usize) -> usize {
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn is_unit_char(ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || matches!(
            ch,
            '秒' | '钟'
                | '分'
                | '小'
                | '时'
                | '毫'
                | '字'
                | '节'
                | '兆'
                | '吉'
                | '太'
                | '千'
                | '内'
                | '之'
                | '以'
                | '下'
                | '上'
                | '约'
                | '的'
        )
        || ch == '/'
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_keywords(keywords: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for keyword in keywords {
        let keyword = normalize_keyword(keyword);
        if keyword.len() <= 1 {
            continue;
        }
        if seen.insert(keyword.clone()) {
            normalized.push(keyword);
        }
    }
    normalized
}

fn normalize_data_standard(data_standard: DataStandard) -> DataStandard {
    DataStandard {
        sample_unit: data_standard.sample_unit.trim().to_string(),
        budget: data_standard.budget.trim().to_string(),
        max_latency_secs: data_standard.max_latency_secs,
        min_dataset_size_bytes: data_standard.min_dataset_size_bytes,
        max_dataset_size_bytes: data_standard.max_dataset_size_bytes,
        canonical_columns: default_canonical_columns(),
        extra_columns: default_extra_columns(),
    }
}

fn default_canonical_columns() -> Vec<String> {
    vec!["sample_id".to_string(), "label".to_string()]
}

fn default_extra_columns() -> Vec<String> {
    vec!["timestamp".to_string()]
}

fn normalize_keyword(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
        .to_lowercase()
}

fn fallback_task_description(query: &str) -> String {
    query.trim().to_string()
}

pub(crate) fn load_setting_env_value(key: &str) -> Option<String> {
    let path = resolve_setting_env_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    contents.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn collect_intent_context(query: &str, include_memories: bool) -> IntentContext {
    let user_profile = collect_user_profile();
    let related_memories = if include_memories {
        retrieve_related_memories(query, MEMORY_SEARCH_LIMIT)
    } else {
        Vec::new()
    };

    IntentContext {
        user_profile,
        bandwidth_bytes_per_sec: collect_local_bandwidth_bytes_per_sec(),
        related_memories,
    }
}

fn retrieve_related_memories(query: &str, limit: usize) -> Vec<String> {
    let entries = load_memory_entries();
    retrieve_related_memories_from_entries(query, &entries, limit)
}

fn load_memory_entries() -> Vec<String> {
    let Some(path) = resolve_memory_path() else {
        return Vec::new();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- "))
        .map(|line| line.trim_start_matches("- ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn resolve_memory_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("USER_MEMORY_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    for base in [
        std::env::current_dir().ok(),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(path) = find_memory_path_from_base(&base) {
            return Some(path);
        }
    }

    None
}

fn resolve_setting_env_path() -> Option<PathBuf> {
    for base in [
        std::env::current_dir().ok(),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
    ]
    .into_iter()
    .flatten()
    {
        for ancestor in base.ancestors() {
            for file_name in ["settings.env", "setting.env"] {
                let candidate = ancestor.join("local").join(file_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn find_memory_path_from_base(base: &Path) -> Option<PathBuf> {
    for ancestor in base.ancestors() {
        let candidate = ancestor.join("files").join("MEMORY.md");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn retrieve_related_memories_from_entries(
    query: &str,
    entries: &[String],
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }

    let named_entities = extract_named_entities(query);
    let entity_matchers: Vec<(String, HashSet<String>)> = named_entities
        .iter()
        .map(|entity| {
            (
                entity.to_lowercase(),
                tokenize_for_search(entity)
                    .into_iter()
                    .collect::<HashSet<_>>(),
            )
        })
        .filter(|(_, tokens)| !tokens.is_empty())
        .collect();
    let keyword_terms: HashSet<String> = extract_salient_terms(query).into_iter().collect();

    let mut entity_ranked: Vec<(usize, usize, String)> = Vec::new();
    let mut keyword_ranked: Vec<(usize, usize, String)> = Vec::new();

    for entry in entries.iter() {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let entry_lower = entry.to_lowercase();
        let entry_tokens: HashSet<String> = tokenize_for_search(entry).into_iter().collect();

        let mut entity_score = 0usize;
        for (entity_phrase, entity_tokens) in &entity_matchers {
            if entity_phrase.len() > 1 && entry_lower.contains(entity_phrase) {
                entity_score += 20 + entity_tokens.len() * 3;
                continue;
            }

            if entity_tokens.len() >= 2
                && entity_tokens
                    .iter()
                    .all(|token| entry_tokens.contains(token))
            {
                entity_score += 12 + entity_tokens.len() * 2;
                continue;
            }

            if entity_tokens.len() == 1
                && entity_tokens
                    .iter()
                    .next()
                    .is_some_and(|token| entry_tokens.contains(token))
            {
                entity_score += 8;
            }
        }

        let keyword_score = keyword_terms
            .iter()
            .filter(|keyword| entry_tokens.contains(*keyword))
            .count()
            * 2;

        if entity_score > 0 {
            entity_ranked.push((entity_score + keyword_score, entry.len(), entry.to_string()));
        } else if keyword_score > 0 {
            keyword_ranked.push((keyword_score, entry.len(), entry.to_string()));
        }
    }

    let mut ranked = if !entity_matchers.is_empty() && !entity_ranked.is_empty() {
        entity_ranked
    } else {
        keyword_ranked
    };

    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked.truncate(limit);
    ranked.into_iter().map(|(_, _, entry)| entry).collect()
}

fn extract_named_entities(query: &str) -> Vec<String> {
    let mut entities = Vec::new();

    for quote in ['"', '\''] {
        let mut segments = query.split(quote);
        segments.next();
        for segment in segments.step_by(2) {
            let candidate = segment.trim();
            if candidate.len() > 1 {
                entities.push(candidate.to_string());
            }
        }
    }

    let surface_tokens = tokenize_surface(query);
    let mut current = Vec::new();

    for (index, token) in surface_tokens.iter().enumerate() {
        if looks_like_named_entity_token(token, index, &surface_tokens) {
            if current.is_empty() {
                if let Some(prefix) = maybe_entity_prefix(index, &surface_tokens) {
                    current.push(prefix.to_string());
                }
            }
            current.push(token.clone());
            continue;
        }
        if !current.is_empty() {
            entities.push(current.join(" "));
            current.clear();
        }
    }
    if !current.is_empty() {
        entities.push(current.join(" "));
    }

    dedupe_strings(entities)
}

fn extract_salient_terms(query: &str) -> Vec<String> {
    dedupe_strings(
        tokenize_for_search(query)
            .into_iter()
            .filter(|token| token.len() > 1)
            .filter(|token| !is_stop_word(token))
            .collect(),
    )
}

fn format_memory_context(related_memories: &[String]) -> String {
    if related_memories.is_empty() {
        return "- None".to_string();
    }

    related_memories
        .iter()
        .map(|memory| format!("- {memory}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tokenize_surface(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_alphanumeric() || ch == '-' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn tokenize_for_search(value: &str) -> Vec<String> {
    tokenize_surface(value)
        .into_iter()
        .map(normalize_keyword)
        .filter(|token| !token.is_empty())
        .collect()
}

fn looks_like_named_entity_token(token: &str, index: usize, surface_tokens: &[String]) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if token.chars().any(|ch| ch.is_ascii_digit()) {
        return true;
    }
    if !first.is_uppercase() {
        return false;
    }

    let next_is_entity_like = surface_tokens
        .get(index + 1)
        .and_then(|next| next.chars().next())
        .is_some_and(|next| next.is_uppercase());

    if index == 0 && !next_is_entity_like {
        return false;
    }

    true
}

fn maybe_entity_prefix(index: usize, surface_tokens: &[String]) -> Option<&str> {
    let previous = surface_tokens.get(index.checked_sub(1)?)?;
    let normalized = normalize_keyword(previous);
    if normalized.len() <= 2 || is_stop_word(&normalized) {
        return None;
    }
    if previous.chars().next().is_some_and(|ch| ch.is_uppercase()) {
        return None;
    }
    Some(previous.as_str())
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "as"
            | "at"
            | "by"
            | "for"
            | "from"
            | "how"
            | "i"
            | "if"
            | "in"
            | "into"
            | "is"
            | "it"
            | "me"
            | "my"
            | "of"
            | "on"
            | "or"
            | "please"
            | "the"
            | "to"
            | "use"
            | "want"
            | "with"
    )
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let owned = trimmed.to_string();
        if seen.insert(owned.to_lowercase()) {
            deduped.push(owned);
        }
    }
    deduped
}

#[cfg(test)]
pub(crate) fn retrieve_related_memories_for_test(
    query: &str,
    entries: &[&str],
    limit: usize,
) -> Vec<String> {
    let entries = entries
        .iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>();
    retrieve_related_memories_from_entries(query, &entries, limit)
}

fn collect_user_profile() -> UserProfile {
    UserProfile {
        cpu: collect_cpu_profile(),
        gpus: collect_gpu_profiles(),
    }
}

fn collect_local_bandwidth_bytes_per_sec() -> u64 {
    detect_local_bandwidth_bytes_per_sec()
        .or_else(load_bandwidth_override_bytes_per_sec)
        .unwrap_or(0)
}

fn detect_local_bandwidth_bytes_per_sec() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        return detect_linux_bandwidth_bytes_per_sec();
    }

    #[cfg(target_os = "macos")]
    {
        return detect_macos_bandwidth_bytes_per_sec();
    }

    #[cfg(target_os = "windows")]
    {
        return detect_windows_bandwidth_bytes_per_sec();
    }

    #[allow(unreachable_code)]
    None
}

fn load_bandwidth_override_bytes_per_sec() -> Option<u64> {
    if let Ok(value) = std::env::var("GUIXU_DEFAULT_BANDWIDTH_BYTES_PER_SEC") {
        let parsed = value.trim().parse::<u64>().ok()?;
        if parsed > 0 {
            return Some(parsed);
        }
    }

    let mbps = std::env::var("GUIXU_DEFAULT_BANDWIDTH_MBPS")
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())?;
    if !mbps.is_finite() || mbps <= 0.0 {
        return None;
    }

    let bytes_per_sec = (mbps * 1_000_000.0) / 8.0;
    if bytes_per_sec >= u64::MAX as f64 {
        Some(u64::MAX)
    } else {
        Some(bytes_per_sec.floor() as u64)
    }
}

fn collect_cpu_profile() -> CpuProfile {
    CpuProfile {
        architecture: std::env::consts::ARCH.to_string(),
        logical_cores: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(0),
        model: detect_cpu_model(),
    }
}

fn collect_gpu_profiles() -> Vec<GpuProfile> {
    let mut gpus = Vec::new();

    if let Some(nvidia_gpus) = detect_nvidia_gpus() {
        gpus.extend(nvidia_gpus);
    }

    if gpus.is_empty() {
        #[cfg(target_os = "linux")]
        {
            gpus.extend(detect_linux_gpus());
        }
        #[cfg(target_os = "macos")]
        {
            gpus.extend(detect_macos_gpus());
        }
        #[cfg(target_os = "windows")]
        {
            gpus.extend(detect_windows_gpus());
        }
    }

    let mut seen = HashSet::new();
    gpus.retain(|gpu| seen.insert((gpu.vendor.clone(), gpu.model.clone())));
    gpus
}

fn detect_cpu_model() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if let Some((_, value)) = line.split_once(':') {
                    if line.starts_with("model name") {
                        let model = value.trim();
                        if !model.is_empty() {
                            return Some(model.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(output) = run_command("sysctl", &["-n", "machdep.cpu.brand_string"]) {
            let model = output.trim();
            if !model.is_empty() {
                return Some(model.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(output) = run_command("wmic", &["cpu", "get", "Name"]) {
            for line in output.lines().skip(1) {
                let model = line.trim();
                if !model.is_empty() {
                    return Some(model.to_string());
                }
            }
        }
    }

    None
}

fn detect_nvidia_gpus() -> Option<Vec<GpuProfile>> {
    let output = run_command(
        "nvidia-smi",
        &["--query-gpu=gpu_name", "--format=csv,noheader"],
    )?;
    let gpus: Vec<GpuProfile> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|model| GpuProfile {
            vendor: Some("NVIDIA".to_string()),
            model: model.to_string(),
        })
        .collect();
    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_bandwidth_bytes_per_sec() -> Option<u64> {
    let interface =
        detect_linux_default_interface().or_else(detect_linux_default_interface_via_ip_route)?;
    read_linux_interface_speed_bytes_per_sec(&interface)
}

#[cfg(target_os = "linux")]
fn detect_linux_default_interface() -> Option<String> {
    let routes = std::fs::read_to_string("/proc/net/route").ok()?;
    for line in routes.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        if fields[1] == "00000000" && fields[0] != "lo" {
            return Some(fields[0].to_string());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_linux_default_interface_via_ip_route() -> Option<String> {
    let output = run_command("ip", &["route", "show", "default"])?;
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let Some(index) = fields.iter().position(|field| *field == "dev") {
            if let Some(interface) = fields.get(index + 1) {
                return Some((*interface).to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn read_linux_interface_speed_bytes_per_sec(interface: &str) -> Option<u64> {
    let speed_path = PathBuf::from("/sys/class/net")
        .join(interface)
        .join("speed");
    let speed_mbps = std::fs::read_to_string(speed_path)
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    if speed_mbps <= 0 {
        return None;
    }

    Some((speed_mbps as u64).saturating_mul(1_000_000) / 8)
}

#[cfg(target_os = "macos")]
fn detect_macos_bandwidth_bytes_per_sec() -> Option<u64> {
    let interface = detect_macos_default_interface()?;
    let output = run_command("system_profiler", &["SPNetworkDataType"])?;
    parse_macos_link_speed_bytes_per_sec(&output, &interface)
}

#[cfg(target_os = "macos")]
fn detect_macos_default_interface() -> Option<String> {
    let output = run_command("route", &["-n", "get", "default"])?;
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix("interface:")?.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

#[cfg(target_os = "macos")]
fn parse_macos_link_speed_bytes_per_sec(output: &str, interface: &str) -> Option<u64> {
    let mut in_matching_block = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(device) = trimmed.strip_prefix("BSD Device Name:") {
            in_matching_block = device.trim() == interface;
            continue;
        }
        if in_matching_block {
            if let Some(speed) = trimmed.strip_prefix("Link Speed:") {
                return parse_link_speed_bytes_per_sec(speed.trim());
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn detect_windows_bandwidth_bytes_per_sec() -> Option<u64> {
    let output = run_command(
        "wmic",
        &["nic", "where", "NetEnabled=true", "get", "Speed", "/value"],
    )?;
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Speed="))
        .filter_map(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .max()
        .map(|bits_per_sec| bits_per_sec / 8)
}

#[cfg(target_os = "macos")]
fn parse_link_speed_bytes_per_sec(value: &str) -> Option<u64> {
    let normalized = value.trim().to_lowercase();
    let capture = extract_numeric_unit_matches(&normalized)
        .into_iter()
        .next()?;
    let multiplier = if capture.unit.starts_with("tbit") || capture.unit.starts_with("tbps") {
        1_000_000_000_000_f64
    } else if capture.unit.starts_with("gbit") || capture.unit.starts_with("gbps") {
        1_000_000_000_f64
    } else if capture.unit.starts_with("mbit") || capture.unit.starts_with("mbps") {
        1_000_000_f64
    } else if capture.unit.starts_with("kbit") || capture.unit.starts_with("kbps") {
        1_000_f64
    } else {
        return None;
    };
    let bits_per_sec = capture.value * multiplier;
    if !bits_per_sec.is_finite() || bits_per_sec <= 0.0 {
        return None;
    }

    Some((bits_per_sec / 8.0).floor() as u64)
}

#[cfg(target_os = "linux")]
fn detect_linux_gpus() -> Vec<GpuProfile> {
    let Some(output) = run_command("lspci", &[]) else {
        return Vec::new();
    };
    output
        .lines()
        .filter(|line| {
            let line_lower = line.to_lowercase();
            line_lower.contains("vga compatible controller")
                || line_lower.contains("3d controller")
                || line_lower.contains("display controller")
        })
        .filter_map(|line| {
            let (_, model) = line.split_once(": ")?;
            let model = model.trim();
            if model.is_empty() {
                return None;
            }
            Some(GpuProfile {
                vendor: infer_gpu_vendor(model),
                model: model.to_string(),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn detect_macos_gpus() -> Vec<GpuProfile> {
    let Some(output) = run_command("system_profiler", &["SPDisplaysDataType"]) else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let model = trimmed.strip_prefix("Chipset Model:")?.trim();
            if model.is_empty() {
                return None;
            }
            Some(GpuProfile {
                vendor: infer_gpu_vendor(model),
                model: model.to_string(),
            })
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn detect_windows_gpus() -> Vec<GpuProfile> {
    let Some(output) = run_command("wmic", &["path", "win32_VideoController", "get", "Name"])
    else {
        return Vec::new();
    };
    output
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|model| GpuProfile {
            vendor: infer_gpu_vendor(model),
            model: model.to_string(),
        })
        .collect()
}

fn infer_gpu_vendor(model: &str) -> Option<String> {
    let model_lower = model.to_lowercase();
    if model_lower.contains("nvidia")
        || model_lower.contains("geforce")
        || model_lower.contains("quadro")
    {
        Some("NVIDIA".to_string())
    } else if model_lower.contains("amd") || model_lower.contains("radeon") {
        Some("AMD".to_string())
    } else if model_lower.contains("intel") {
        Some("Intel".to_string())
    } else if model_lower.contains("apple") {
        Some("Apple".to_string())
    } else if model_lower.contains("qualcomm") || model_lower.contains("adreno") {
        Some("Qualcomm".to_string())
    } else {
        None
    }
}

fn run_command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|stdout| stdout.trim().to_string())
        .filter(|stdout| !stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        compute_max_dataset_size_bytes, derive_query_transfer_constraints,
        extract_min_dataset_size_bytes, format_numeric_budget, normalize_budget,
        normalize_latency_secs, LlmScalarValue,
    };

    #[test]
    fn normalize_budget_preserves_units_from_llm() {
        let budget = normalize_budget(Some(LlmScalarValue::Text("$20".into())));
        assert_eq!(budget, "$20");
    }

    #[test]
    fn normalize_latency_secs_accepts_llm_number() {
        let latency = normalize_latency_secs(Some(LlmScalarValue::Number(45.0)));
        assert_eq!(latency, Some(45.0));
    }

    #[test]
    fn extract_min_dataset_size_bytes_parses_minimum_constraint() {
        let size = extract_min_dataset_size_bytes("需要至少 2GB 的数据集");
        assert_eq!(size, Some(2_000_000_000));
    }

    #[test]
    fn derive_query_transfer_constraints_defaults_latency_and_computes_max_size() {
        let constraints = derive_query_transfer_constraints("find a cat dataset", None, 8_000_000);
        assert_eq!(constraints.max_latency_secs, 30.0);
        assert_eq!(constraints.min_dataset_size_bytes, 0);
        assert_eq!(constraints.max_dataset_size_bytes, 240_000_000);
        assert_eq!(
            compute_max_dataset_size_bytes(constraints.max_latency_secs, 8_000_000),
            240_000_000
        );
    }

    #[test]
    fn format_numeric_budget_adds_default_unit() {
        assert_eq!(format_numeric_budget(20.0), "20 USD");
    }
}
