// ABOUTME: Configuration types for semantic code search.
// ABOUTME: Defines embedder, vector store, indexing, and search settings.

use serde::Deserialize;

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
