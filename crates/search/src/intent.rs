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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataStandard {
    pub sample_unit: String,
    #[serde(default)]
    pub metadata_fields: Vec<MetadataField>,
    #[serde(default)]
    pub canonical_columns: Vec<String>,
    #[serde(default)]
    pub extra_columns: Vec<String>,
}

impl Default for DataStandard {
    fn default() -> Self {
        Self {
            sample_unit: String::new(),
            metadata_fields: default_metadata_fields(),
            canonical_columns: default_canonical_columns(),
            extra_columns: default_extra_columns(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataField {
    pub name: String,
    #[serde(default)]
    pub value: String,
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

#[derive(Debug, Clone)]
pub struct IntentParserConfig {
    pub api_key: Option<String>,
    pub api_base: String,
    pub model: String,
    pub timeout: Duration,
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
        }
    }
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
        if self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Err(anyhow!("missing DEEPSEEK_API_KEY"));
        }

        let query_owned = query.to_string();
        let (user_profile, related_memories) =
            tokio::task::spawn_blocking(move || collect_intent_context(&query_owned, true))
                .await
                .unwrap_or_else(|_| (UserProfile::default(), Vec::new()));

        eprintln!("calling DeepSeek API for intent parsing");
        let profile = self
            .profile_with_deepseek(query, &user_profile, &related_memories)
            .await?;
        eprintln!(
            "intent profile:\n{}",
            serde_json::to_string_pretty(&profile)?
        );
        Ok(profile)
    }

    /// Parse a natural language query into structured intent.
    pub async fn parse(&self, query: &str) -> Result<QueryProfile> {
        self.profile(query).await
    }

    async fn profile_with_deepseek(
        &self,
        query: &str,
        user_profile: &UserProfile,
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
        self.profile_from_deepseek_content(query, user_profile, &content)
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
        content: &str,
    ) -> Result<QueryProfile> {
        let llm_profile = serde_json::from_str::<LlmIntentProfile>(content)
            .with_context(|| format!("parse DeepSeek intent JSON: {content}"))?;

        Ok(QueryProfile {
            raw_query: query.to_string(),
            task_type: normalize_optional(llm_profile.task_type),
            task_description: normalize_optional(llm_profile.task_description)
                .or_else(|| Some(fallback_task_description(query))),
            target_entity: normalize_optional(llm_profile.target_entity),
            keywords: normalize_keywords(llm_profile.keywords),
            data_standard: normalize_data_standard(llm_profile.data_standard),
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
    "metadata_fields": [{"name": "string", "value": "string"}],
    "canonical_columns": ["string"],
    "extra_columns": ["string"]
  }
}

Rules:
- First infer the user's likely task description from the natural-language query and any relevant user memories.
- task_type must be a concise task label such as classification, forecasting, detection, ranking, retrieval, segmentation, generation, summarization, or evaluation.
- task_description must be a detailed natural-language description of what the user is trying to accomplish with the data.
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
- data_standard carries hard dataset constraints that will be used to filter search results before scoring.
- data_standard.sample_unit must encode the required per-sample modality using broad units such as image, video, text, tabular, or audio.
- If the user is asking for an image classifier or image detector, sample_unit should normally be "image".
- data_standard.metadata_fields must always include exactly these fields: min_sample_num, resolution.
- data_standard.metadata_fields.resolution is the minimum acceptable per-sample resolution or clarity requirement and should be a concrete floor such as "720p", "1080p", or "1024x1024" when the user implies the samples must be clear, sharp, or not blurry.
- If the user requests that images should not be blurry or should be high quality, prefer a concrete resolution floor instead of leaving resolution empty.
- data_standard.canonical_columns must always include exactly these fields: sample_id, label.
- data_standard.extra_columns must always include exactly these fields: timestamp.
- metadata_fields values may be empty strings when the query does not provide the information.
- canonical_columns and extra_columns must be string arrays, not objects.
- If you do not have enough information for a hard constraint, leave that specific field empty rather than inventing it.
Examples:
  Query: "write an image classifier that checks whether my cat is in the photo taken by my house monitor"
  Good keywords: ["cat"]
  Bad keywords: ["image", "classifier", "cat", "photo", "house", "monitor"]

  Query: "build a model to detect lung nodules in chest CT scans"
  Good keywords: ["lung nodule", "chest ct"]
  Bad keywords: ["detect", "model", "build", "scan"]
- If a field is unknown, use null for scalars and [] for keywords.
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
    let metadata_values = data_standard
        .metadata_fields
        .into_iter()
        .map(|field| {
            (
                field.name.trim().to_lowercase(),
                field.value.trim().to_string(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    DataStandard {
        sample_unit: data_standard.sample_unit.trim().to_string(),
        metadata_fields: vec![
            MetadataField {
                name: "min_sample_num".to_string(),
                value: metadata_values
                    .get("min_sample_num")
                    .cloned()
                    .unwrap_or_default(),
            },
            MetadataField {
                name: "resolution".to_string(),
                value: metadata_values
                    .get("resolution")
                    .cloned()
                    .unwrap_or_default(),
            },
        ],
        canonical_columns: default_canonical_columns(),
        extra_columns: default_extra_columns(),
    }
}

fn default_metadata_fields() -> Vec<MetadataField> {
    vec![
        MetadataField {
            name: "min_sample_num".to_string(),
            value: String::new(),
        },
        MetadataField {
            name: "resolution".to_string(),
            value: String::new(),
        },
    ]
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

fn load_setting_env_value(key: &str) -> Option<String> {
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

fn collect_intent_context(query: &str, include_memories: bool) -> (UserProfile, Vec<String>) {
    let user_profile = collect_user_profile();
    let related_memories = if include_memories {
        retrieve_related_memories(query, MEMORY_SEARCH_LIMIT)
    } else {
        Vec::new()
    };
    (user_profile, related_memories)
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
            let candidate = ancestor.join("local").join("setting.env");
            if candidate.is_file() {
                return Some(candidate);
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
