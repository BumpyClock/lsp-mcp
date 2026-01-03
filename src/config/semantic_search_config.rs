// ABOUTME: Configuration types for semantic code search.
// ABOUTME: Defines embedder, vector store, indexing, and search settings.

use serde::{Deserialize, Deserializer};

/// Configuration for the embedding provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum EmbedderConfig {
    /// OpenAI-compatible embedding API
    #[serde(rename = "openai")]
    OpenAI {
        /// Model name (default: text-embedding-3-small)
        #[serde(default = "default_openai_model")]
        model: String,
        /// API base URL (default: https://api.openai.com/v1)
        #[serde(default = "default_openai_base_url")]
        base_url: String,
        /// Direct API key (optional, prefer api_key_env)
        #[serde(default)]
        api_key: Option<String>,
        /// Environment variable name containing API key
        #[serde(default = "default_openai_api_key_env")]
        api_key_env: String,
        /// Embedding dimension (default: 1536)
        #[serde(default = "default_openai_dimension")]
        dimension: usize,
    },
    /// Local FastEmbed model
    #[serde(rename = "fastembed")]
    FastEmbed {
        /// Model name (default: BAAI/bge-small-en-v1.5)
        #[serde(default = "default_fastembed_model")]
        model: String,
        /// Embedding dimension (default: 384)
        #[serde(default = "default_fastembed_dimension")]
        dimension: usize,
        /// Cache directory for downloaded models (default: ~/.lsp-mcp/.fastembed-cache)
        #[serde(default = "default_fastembed_cache_dir")]
        cache_dir: String,
    },
}

/// Optional embedder configuration for config file merging.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub(crate) enum EmbedderConfigFile {
    #[serde(rename = "openai")]
    OpenAI {
        model: Option<String>,
        base_url: Option<String>,
        api_key: Option<String>,
        api_key_env: Option<String>,
        dimension: Option<usize>,
    },
    #[serde(rename = "fastembed")]
    FastEmbed {
        model: Option<String>,
        dimension: Option<usize>,
        cache_dir: Option<String>,
    },
}

impl EmbedderConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        match (self, project) {
            (
                EmbedderConfigFile::OpenAI {
                    model,
                    base_url,
                    api_key,
                    api_key_env,
                    dimension,
                },
                EmbedderConfigFile::OpenAI {
                    model: project_model,
                    base_url: project_base_url,
                    api_key: project_api_key,
                    api_key_env: project_api_key_env,
                    dimension: project_dimension,
                },
            ) => EmbedderConfigFile::OpenAI {
                model: project_model.or(model),
                base_url: project_base_url.or(base_url),
                api_key: project_api_key.or(api_key),
                api_key_env: project_api_key_env.or(api_key_env),
                dimension: project_dimension.or(dimension),
            },
            (
                EmbedderConfigFile::FastEmbed {
                    model,
                    dimension,
                    cache_dir,
                },
                EmbedderConfigFile::FastEmbed {
                    model: project_model,
                    dimension: project_dimension,
                    cache_dir: project_cache_dir,
                },
            ) => EmbedderConfigFile::FastEmbed {
                model: project_model.or(model),
                dimension: project_dimension.or(dimension),
                cache_dir: project_cache_dir.or(cache_dir),
            },
            (_, project) => project,
        }
    }

    pub(crate) fn resolve(self) -> EmbedderConfig {
        match self {
            EmbedderConfigFile::OpenAI {
                model,
                base_url,
                api_key,
                api_key_env,
                dimension,
            } => EmbedderConfig::OpenAI {
                model: model.unwrap_or_else(default_openai_model),
                base_url: base_url.unwrap_or_else(default_openai_base_url),
                api_key,
                api_key_env: api_key_env.unwrap_or_else(default_openai_api_key_env),
                dimension: dimension.unwrap_or_else(default_openai_dimension),
            },
            EmbedderConfigFile::FastEmbed {
                model,
                dimension,
                cache_dir,
            } => EmbedderConfig::FastEmbed {
                model: model.unwrap_or_else(default_fastembed_model),
                dimension: dimension.unwrap_or_else(default_fastembed_dimension),
                cache_dir: cache_dir.unwrap_or_else(default_fastembed_cache_dir),
            },
        }
    }
}

fn default_openai_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_openai_dimension() -> usize {
    1536
}

fn default_fastembed_model() -> String {
    "BAAI/bge-small-en-v1.5".to_string()
}

fn default_fastembed_dimension() -> usize {
    384
}

fn default_fastembed_cache_dir() -> String {
    if let Ok(cache_dir) = std::env::var("FASTEMBED_CACHE_DIR") {
        return cache_dir;
    }

    dirs::home_dir()
        .map(|home| home.join(".lsp-mcp").join(".fastembed-cache"))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".fastembed_cache".to_string())
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        EmbedderConfig::FastEmbed {
            model: default_fastembed_model(),
            dimension: default_fastembed_dimension(),
            cache_dir: default_fastembed_cache_dir(),
        }
    }
}

impl EmbedderConfig {
    /// Get the embedding dimension for this provider.
    pub fn dimension(&self) -> usize {
        match self {
            EmbedderConfig::OpenAI { dimension, .. } => *dimension,
            EmbedderConfig::FastEmbed { dimension, .. } => *dimension,
        }
    }
}

/// Configuration for the vector store.
#[derive(Debug, Clone, Deserialize)]
pub struct VectorStoreConfig {
    /// Storage path relative to workspace (default: .lsp-mcp/semanticSearch)
    #[serde(default = "default_vector_store_path")]
    pub path: String,
}

/// Optional vector store configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct VectorStoreConfigFile {
    pub path: Option<String>,
}

impl VectorStoreConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        VectorStoreConfigFile {
            path: project.path.or(self.path),
        }
    }

    pub(crate) fn resolve(self) -> VectorStoreConfig {
        VectorStoreConfig {
            path: self.path.unwrap_or_else(default_vector_store_path),
        }
    }
}

fn default_vector_store_path() -> String {
    ".lsp-mcp/semanticSearch".to_string()
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        VectorStoreConfig {
            path: default_vector_store_path(),
        }
    }
}

/// Configuration for excluding files and directories during indexing.
#[derive(Debug, Clone, Deserialize)]
pub struct ExcludeConfig {
    /// File patterns to exclude (glob patterns)
    #[serde(default)]
    pub files: Vec<String>,
    /// Directory patterns to exclude (glob patterns)
    #[serde(default = "default_exclude_directories")]
    pub directories: Vec<String>,
}

/// Optional exclude configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ExcludeConfigFile {
    pub files: Option<Vec<String>>,
    pub directories: Option<Vec<String>>,
}

impl ExcludeConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        ExcludeConfigFile {
            files: project.files.or(self.files),
            directories: project.directories.or(self.directories),
        }
    }

    pub(crate) fn resolve(self) -> ExcludeConfig {
        ExcludeConfig {
            files: self.files.unwrap_or_default(),
            directories: self.directories.unwrap_or_else(default_exclude_directories),
        }
    }
}

fn default_include_patterns() -> Vec<String> {
    vec![
        "**/*.rs".to_string(),
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "**/*.py".to_string(),
        "**/*.go".to_string(),
        "**/*.java".to_string(),
        "**/*.c".to_string(),
        "**/*.cpp".to_string(),
        "**/*.h".to_string(),
        "**/*.cs".to_string(),
        "**/*.rb".to_string(),
        "**/*.php".to_string(),
        "**/*.md".to_string(),
    ]
}

fn default_exclude_directories() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/__pycache__/**".to_string(),
        "**/venv/**".to_string(),
        "**/.venv/**".to_string(),
    ]
}

fn default_max_file_size_mb() -> f64 {
    1.0
}

fn default_min_chunk_chars() -> usize {
    50
}

fn default_max_chunk_chars() -> usize {
    2000
}

fn default_max_function_chunk_chars() -> usize {
    5000
}

fn default_chunk_overlap_chars() -> usize {
    200
}

fn default_batch_size() -> usize {
    60
}

fn default_respect_gitignore() -> bool {
    true
}

impl Default for ExcludeConfig {
    fn default() -> Self {
        ExcludeConfig {
            files: Vec::new(),
            directories: default_exclude_directories(),
        }
    }
}

fn default_min_score() -> f32 {
    // Higher threshold (0.4) improves precision over 0.25
    // Users can override via config if broader recall is needed
    0.4
}

fn default_max_results() -> usize {
    5
}

fn default_context_lines() -> Option<u32> {
    Some(15)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OptionalContextLines {
    Missing,
    Null,
    Value(u32),
}

impl Default for OptionalContextLines {
    fn default() -> Self {
        OptionalContextLines::Missing
    }
}

impl OptionalContextLines {
    fn merge(self, project: Self) -> Self {
        match project {
            OptionalContextLines::Missing => self,
            _ => project,
        }
    }

    fn resolve(self) -> Option<u32> {
        match self {
            OptionalContextLines::Missing => default_context_lines(),
            OptionalContextLines::Null => None,
            OptionalContextLines::Value(value) => Some(value),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalContextLines {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<u32>::deserialize(deserializer)?;
        Ok(match value {
            Some(value) => OptionalContextLines::Value(value),
            None => OptionalContextLines::Null,
        })
    }
}

/// Configuration for optional LLM enrichment during indexing.
#[derive(Debug, Clone, Deserialize)]
pub struct EnrichmentConfig {
    /// Enable LLM enrichment (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Model name (default: gpt-4o-mini)
    #[serde(default = "default_enrichment_model")]
    pub model: String,
    /// Batch size for LLM requests (default: 16)
    #[serde(default = "default_enrichment_batch_size")]
    pub batch_size: usize,
    /// Maximum number of concurrent LLM requests (default: 2)
    #[serde(default = "default_enrichment_max_concurrency")]
    pub max_concurrency: usize,
    /// Maximum summary length in characters (default: 280)
    #[serde(default = "default_enrichment_summary_max_chars")]
    pub summary_max_chars: usize,
    /// Maximum number of tags per item (default: 6)
    #[serde(default = "default_enrichment_max_tags")]
    pub max_tags: usize,
    /// Request timeout in milliseconds (default: 8000)
    #[serde(default = "default_enrichment_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_enrichment_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_enrichment_batch_size() -> usize {
    16
}

fn default_enrichment_max_concurrency() -> usize {
    2
}

fn default_enrichment_summary_max_chars() -> usize {
    280
}

fn default_enrichment_max_tags() -> usize {
    6
}

fn default_enrichment_timeout_ms() -> u64 {
    8000
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_enrichment_model(),
            batch_size: default_enrichment_batch_size(),
            max_concurrency: default_enrichment_max_concurrency(),
            summary_max_chars: default_enrichment_summary_max_chars(),
            max_tags: default_enrichment_max_tags(),
            timeout_ms: default_enrichment_timeout_ms(),
        }
    }
}

/// Optional enrichment configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct EnrichmentConfigFile {
    pub enabled: Option<bool>,
    pub model: Option<String>,
    pub batch_size: Option<usize>,
    pub max_concurrency: Option<usize>,
    pub summary_max_chars: Option<usize>,
    pub max_tags: Option<usize>,
    pub timeout_ms: Option<u64>,
}

impl EnrichmentConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        EnrichmentConfigFile {
            enabled: project.enabled.or(self.enabled),
            model: project.model.or(self.model),
            batch_size: project.batch_size.or(self.batch_size),
            max_concurrency: project.max_concurrency.or(self.max_concurrency),
            summary_max_chars: project.summary_max_chars.or(self.summary_max_chars),
            max_tags: project.max_tags.or(self.max_tags),
            timeout_ms: project.timeout_ms.or(self.timeout_ms),
        }
    }

    pub(crate) fn resolve(self) -> EnrichmentConfig {
        EnrichmentConfig {
            enabled: self.enabled.unwrap_or_default(),
            model: self.model.unwrap_or_else(default_enrichment_model),
            batch_size: self.batch_size.unwrap_or_else(default_enrichment_batch_size),
            max_concurrency: self
                .max_concurrency
                .unwrap_or_else(default_enrichment_max_concurrency),
            summary_max_chars: self
                .summary_max_chars
                .unwrap_or_else(default_enrichment_summary_max_chars),
            max_tags: self.max_tags.unwrap_or_else(default_enrichment_max_tags),
            timeout_ms: self.timeout_ms.unwrap_or_else(default_enrichment_timeout_ms),
        }
    }
}

/// Semantic search feature configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticSearchConfig {
    /// Enable semantic search (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Embedding provider configuration
    #[serde(default)]
    pub embedder: EmbedderConfig,
    /// Vector store configuration
    #[serde(default)]
    pub vector_store: VectorStoreConfig,
    /// File patterns to include (glob patterns)
    #[serde(default = "default_include_patterns")]
    pub include: Vec<String>,
    /// File and directory patterns to exclude (glob patterns)
    #[serde(default)]
    pub exclude: ExcludeConfig,
    /// Maximum file size to index in MB (default: 1)
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: f64,
    /// Minimum chunk size in characters (default: 50)
    #[serde(default = "default_min_chunk_chars")]
    pub min_chunk_chars: usize,
    /// Maximum chunk size in characters (default: 2000)
    #[serde(default = "default_max_chunk_chars")]
    pub max_chunk_chars: usize,
    /// Maximum chunk size for functions in characters (default: 5000)
    #[serde(default = "default_max_function_chunk_chars")]
    pub max_function_chunk_chars: usize,
    /// Overlap size between chunks in characters (default: 200)
    #[serde(default = "default_chunk_overlap_chars")]
    pub chunk_overlap_chars: usize,
    /// Batch size for embedding requests (default: 60)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Whether to respect .gitignore files when indexing (default: true)
    #[serde(default = "default_respect_gitignore")]
    pub respect_gitignore: bool,
    /// Minimum similarity score threshold (0.0 to 1.0, default: 0.4)
    #[serde(default = "default_min_score")]
    pub min_score: f32,
    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Default context lines per result chunk (default: 15, None = full chunk)
    #[serde(default = "default_context_lines")]
    pub default_context_lines: Option<u32>,
    /// Optional LLM enrichment configuration
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
}

/// Optional semantic search configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct SemanticSearchConfigFile {
    pub enabled: Option<bool>,
    pub embedder: Option<EmbedderConfigFile>,
    pub vector_store: Option<VectorStoreConfigFile>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<ExcludeConfigFile>,
    pub max_file_size_mb: Option<f64>,
    pub min_chunk_chars: Option<usize>,
    pub max_chunk_chars: Option<usize>,
    pub max_function_chunk_chars: Option<usize>,
    pub chunk_overlap_chars: Option<usize>,
    pub batch_size: Option<usize>,
    pub respect_gitignore: Option<bool>,
    pub min_score: Option<f32>,
    pub max_results: Option<usize>,
    #[serde(default)]
    pub default_context_lines: OptionalContextLines,
    pub enrichment: Option<EnrichmentConfigFile>,
}

impl SemanticSearchConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        let embedder = match (self.embedder, project.embedder) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_embedder)) => Some(project_embedder),
            (Some(global), Some(project_embedder)) => Some(global.merge(project_embedder)),
        };
        let vector_store = match (self.vector_store, project.vector_store) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_vector_store)) => Some(project_vector_store),
            (Some(global), Some(project_vector_store)) => Some(global.merge(project_vector_store)),
        };
        let exclude = match (self.exclude, project.exclude) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_exclude)) => Some(project_exclude),
            (Some(global), Some(project_exclude)) => Some(global.merge(project_exclude)),
        };
        let enrichment = match (self.enrichment, project.enrichment) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_enrichment)) => Some(project_enrichment),
            (Some(global), Some(project_enrichment)) => {
                Some(global.merge(project_enrichment))
            }
        };

        SemanticSearchConfigFile {
            enabled: project.enabled.or(self.enabled),
            embedder,
            vector_store,
            include: project.include.or(self.include),
            exclude,
            max_file_size_mb: project.max_file_size_mb.or(self.max_file_size_mb),
            min_chunk_chars: project.min_chunk_chars.or(self.min_chunk_chars),
            max_chunk_chars: project.max_chunk_chars.or(self.max_chunk_chars),
            max_function_chunk_chars: project
                .max_function_chunk_chars
                .or(self.max_function_chunk_chars),
            chunk_overlap_chars: project.chunk_overlap_chars.or(self.chunk_overlap_chars),
            batch_size: project.batch_size.or(self.batch_size),
            respect_gitignore: project.respect_gitignore.or(self.respect_gitignore),
            min_score: project.min_score.or(self.min_score),
            max_results: project.max_results.or(self.max_results),
            default_context_lines: self.default_context_lines.merge(project.default_context_lines),
            enrichment,
        }
    }

    pub(crate) fn resolve(self) -> SemanticSearchConfig {
        SemanticSearchConfig {
            enabled: self.enabled.unwrap_or_default(),
            embedder: self
                .embedder
                .map(EmbedderConfigFile::resolve)
                .unwrap_or_default(),
            vector_store: self
                .vector_store
                .map(VectorStoreConfigFile::resolve)
                .unwrap_or_default(),
            include: self.include.unwrap_or_else(default_include_patterns),
            exclude: self
                .exclude
                .map(ExcludeConfigFile::resolve)
                .unwrap_or_default(),
            max_file_size_mb: self.max_file_size_mb.unwrap_or_else(default_max_file_size_mb),
            min_chunk_chars: self.min_chunk_chars.unwrap_or_else(default_min_chunk_chars),
            max_chunk_chars: self.max_chunk_chars.unwrap_or_else(default_max_chunk_chars),
            max_function_chunk_chars: self
                .max_function_chunk_chars
                .unwrap_or_else(default_max_function_chunk_chars),
            chunk_overlap_chars: self
                .chunk_overlap_chars
                .unwrap_or_else(default_chunk_overlap_chars),
            batch_size: self.batch_size.unwrap_or_else(default_batch_size),
            respect_gitignore: self
                .respect_gitignore
                .unwrap_or_else(default_respect_gitignore),
            min_score: self.min_score.unwrap_or_else(default_min_score),
            max_results: self.max_results.unwrap_or_else(default_max_results),
            default_context_lines: self.default_context_lines.resolve(),
            enrichment: self
                .enrichment
                .map(EnrichmentConfigFile::resolve)
                .unwrap_or_default(),
        }
    }
}

impl SemanticSearchConfig {
    /// Create a default config with semantic search enabled.
    pub fn default_enabled() -> Self {
        let mut config = Self::default();
        config.enabled = true;
        config
    }

    /// Check if the configuration is valid for starting semantic search.
    pub fn is_valid(&self) -> Result<(), String> {
        if !self.enabled {
            return Err("Semantic search is disabled".to_string());
        }

        // Validate OpenAI config requires API key
        if let EmbedderConfig::OpenAI {
            api_key,
            api_key_env,
            ..
        } = &self.embedder
        {
            let api_key_present = api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
            if !api_key_present && std::env::var(api_key_env).is_err() {
                return Err(format!(
                    "OpenAI API key missing; set api_key or environment variable: {}",
                    api_key_env
                ));
            }
        }

        Ok(())
    }

    /// Get the storage path for the vector index.
    pub fn storage_path(&self) -> &str {
        &self.vector_store.path
    }

    /// Expand exclude patterns to include directory matches.
    pub fn expanded_exclude_patterns(&self) -> Vec<String> {
        let mut patterns = Vec::new();
        patterns.extend(self.exclude.files.clone());
        patterns.extend(expand_directory_patterns(&self.exclude.directories));
        patterns
    }
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        SemanticSearchConfig {
            enabled: false,
            embedder: EmbedderConfig::default(),
            vector_store: VectorStoreConfig::default(),
            include: default_include_patterns(),
            exclude: ExcludeConfig::default(),
            max_file_size_mb: default_max_file_size_mb(),
            min_chunk_chars: default_min_chunk_chars(),
            max_chunk_chars: default_max_chunk_chars(),
            max_function_chunk_chars: default_max_function_chunk_chars(),
            chunk_overlap_chars: default_chunk_overlap_chars(),
            batch_size: default_batch_size(),
            respect_gitignore: default_respect_gitignore(),
            min_score: default_min_score(),
            max_results: default_max_results(),
            default_context_lines: default_context_lines(),
            enrichment: EnrichmentConfig::default(),
        }
    }
}

fn expand_directory_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains('*') {
            let normalized = trimmed.trim_end_matches('/');
            expanded.push(trimmed.to_string());
            if !normalized.ends_with("/**") {
                expanded.push(format!("{}/**", normalized));
            }
        } else {
            let normalized = trimmed.trim_matches('/');
            if !normalized.is_empty() {
                expanded.push(format!("**/{}/**", normalized));
            }
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn random_env_var_name() -> String {
        format!("LSP_MCP_TEST_{}", Uuid::new_v4().to_string().replace('-', "_"))
    }

    fn random_api_key() -> String {
        format!("sk-test-{}", Uuid::new_v4())
    }

    fn openai_config(api_key: Option<String>, api_key_env: String) -> SemanticSearchConfig {
        SemanticSearchConfig {
            enabled: true,
            embedder: EmbedderConfig::OpenAI {
                model: default_openai_model(),
                base_url: default_openai_base_url(),
                api_key,
                api_key_env,
                dimension: default_openai_dimension(),
            },
            vector_store: VectorStoreConfig::default(),
            include: default_include_patterns(),
            exclude: ExcludeConfig::default(),
            max_file_size_mb: default_max_file_size_mb(),
            min_chunk_chars: default_min_chunk_chars(),
            max_chunk_chars: default_max_chunk_chars(),
            max_function_chunk_chars: default_max_function_chunk_chars(),
            chunk_overlap_chars: default_chunk_overlap_chars(),
            batch_size: default_batch_size(),
            respect_gitignore: default_respect_gitignore(),
            min_score: default_min_score(),
            max_results: default_max_results(),
            default_context_lines: default_context_lines(),
            enrichment: EnrichmentConfig::default(),
        }
    }

    #[test]
    fn openai_config_is_valid_when_inline_api_key_is_present() {
        let env_name = random_env_var_name();
        let config = openai_config(Some(random_api_key()), env_name);
        assert!(
            config.is_valid().is_ok(),
            "Inline API key did not validate"
        );
    }

    #[test]
    fn openai_config_is_valid_when_env_var_is_present() {
        let env_name = random_env_var_name();
        std::env::set_var(&env_name, random_api_key());
        let config = openai_config(None, env_name);
        assert!(
            config.is_valid().is_ok(),
            "Environment API key did not validate"
        );
    }

    #[test]
    fn openai_config_is_invalid_when_no_api_key_is_provided() {
        let env_name = random_env_var_name();
        let config = openai_config(None, env_name);
        assert!(
            config.is_valid().is_err(),
            "Missing API key should fail validation"
        );
    }
}
