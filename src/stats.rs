// ABOUTME: Tool usage statistics tracking and persistence.
// ABOUTME: Stores call counts per tool in both local and global JSON files.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Individual tool statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolStat {
    pub total_calls: u64,
    pub success_count: u64,
    pub failure_count: u64,
}

/// File format for tool-stats.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatsFile {
    pub version: u32,
    pub last_updated: DateTime<Utc>,
    pub tools: HashMap<String, ToolStat>,
}

/// Statistics store managing both local and global tool usage data
pub struct StatsStore {
    local_path: PathBuf,
    global_path: PathBuf,
    local_cache: Arc<Mutex<ToolStatsFile>>,
    global_cache: Arc<Mutex<ToolStatsFile>>,
}

impl StatsStore {
    /// Create a new stats store for the given workspace root
    ///
    /// Loads existing stats from disk if available, otherwise creates default empty stats.
    /// Never fails - silently uses defaults if files cannot be read.
    pub async fn new(workspace_root: &Path) -> Self {
        let global_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lsp-mcp")
            .join("tool-stats.json");

        Self::new_with_global_path(workspace_root, &global_path).await
    }

    async fn new_with_global_path(workspace_root: &Path, global_path: &Path) -> Self {
        let local_path = workspace_root.join(".lsp-mcp").join("tool-stats.json");

        let local_stats = Self::load_stats(&local_path).await;
        let global_stats = Self::load_stats(global_path).await;

        Self {
            local_path,
            global_path: global_path.to_path_buf(),
            local_cache: Arc::new(Mutex::new(local_stats)),
            global_cache: Arc::new(Mutex::new(global_stats)),
        }
    }

    /// Record a tool call, updating both local and global stats
    ///
    /// Updates in-memory caches immediately and spawns background tasks to persist to disk.
    /// Never fails - errors during persistence are silently ignored.
    pub async fn record_call(&self, tool_name: &str, success: bool) {
        self.update_cache(&self.local_cache, tool_name, success);
        self.update_cache(&self.global_cache, tool_name, success);

        self.persist_async(&self.local_path, &self.local_cache);
        self.persist_async(&self.global_path, &self.global_cache);
    }

    fn load_stats(path: &Path) -> impl std::future::Future<Output = ToolStatsFile> + Send {
        let path = path.to_path_buf();
        async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match serde_json::from_str::<ToolStatsFile>(&content) {
                    Ok(stats) => stats,
                    Err(_) => Self::default_stats(),
                },
                Err(_) => Self::default_stats(),
            }
        }
    }

    fn default_stats() -> ToolStatsFile {
        ToolStatsFile {
            version: 1,
            last_updated: Utc::now(),
            tools: HashMap::new(),
        }
    }

    fn update_cache(&self, cache: &Arc<Mutex<ToolStatsFile>>, tool_name: &str, success: bool) {
        let mut stats = cache.lock();
        stats.last_updated = Utc::now();

        let tool_stat = stats.tools.entry(tool_name.to_string()).or_default();
        tool_stat.total_calls += 1;
        if success {
            tool_stat.success_count += 1;
        } else {
            tool_stat.failure_count += 1;
        }
    }

    fn persist_async(&self, path: &PathBuf, cache: &Arc<Mutex<ToolStatsFile>>) {
        let path = path.clone();
        let cache = Arc::clone(cache);

        tokio::spawn(async move {
            let stats = {
                let locked = cache.lock();
                locked.clone()
            };

            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            if let Ok(json) = serde_json::to_string_pretty(&stats) {
                let _ = tokio::fs::write(&path, json).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    #[test]
    fn tool_stat_serializes_to_json() {
        let stat = ToolStat {
            total_calls: 150,
            success_count: 145,
            failure_count: 5,
        };

        let json = serde_json::to_string(&stat).unwrap();
        let parsed: ToolStat = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed.total_calls, 150,
            "Total calls should match after serialization"
        );
        assert_eq!(
            parsed.success_count, 145,
            "Success count should match after serialization"
        );
        assert_eq!(
            parsed.failure_count, 5,
            "Failure count should match after serialization"
        );
    }

    #[test]
    fn tool_stat_default_has_zero_counts() {
        let stat = ToolStat::default();

        assert_eq!(stat.total_calls, 0, "Default total calls should be zero");
        assert_eq!(
            stat.success_count, 0,
            "Default success count should be zero"
        );
        assert_eq!(
            stat.failure_count, 0,
            "Default failure count should be zero"
        );
    }

    #[test]
    fn tool_stats_file_serializes_with_version_and_timestamp() {
        let mut tools = HashMap::new();
        tools.insert(
            "hover".to_string(),
            ToolStat {
                total_calls: 150,
                success_count: 145,
                failure_count: 5,
            },
        );

        let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let stats_file = ToolStatsFile {
            version: 1,
            last_updated: timestamp,
            tools,
        };

        let json = serde_json::to_string(&stats_file).unwrap();
        let parsed: ToolStatsFile = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed.version, 1,
            "Version should be preserved in serialization"
        );
        assert_eq!(
            parsed.last_updated, timestamp,
            "Timestamp should be preserved in serialization"
        );
        assert_eq!(parsed.tools.len(), 1, "Tools map should have one entry");

        let hover_stat = parsed.tools.get("hover").unwrap();
        assert_eq!(hover_stat.total_calls, 150, "Tool stat should be preserved");
    }

    #[test]
    fn tool_stats_file_deserializes_from_expected_json_format() {
        let json = r#"{
            "version": 1,
            "last_updated": "2024-01-15T10:30:00Z",
            "tools": {
                "hover": {
                    "total_calls": 150,
                    "success_count": 145,
                    "failure_count": 5
                }
            }
        }"#;

        let parsed: ToolStatsFile = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.version, 1, "Version should match expected format");
        assert_eq!(parsed.tools.len(), 1, "Should have one tool entry");

        let hover_stat = parsed.tools.get("hover").unwrap();
        assert_eq!(hover_stat.total_calls, 150, "Total calls should match JSON");
        assert_eq!(
            hover_stat.success_count, 145,
            "Success count should match JSON"
        );
        assert_eq!(
            hover_stat.failure_count, 5,
            "Failure count should match JSON"
        );
    }

    #[tokio::test]
    async fn stats_store_creates_with_default_empty_stats() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;

        let local_cache = store.local_cache.lock();
        assert_eq!(local_cache.version, 1, "Local cache should have version 1");
        assert_eq!(
            local_cache.tools.len(),
            0,
            "Local cache should start with no tool stats"
        );

        let global_cache = store.global_cache.lock();
        assert_eq!(
            global_cache.version, 1,
            "Global cache should have version 1"
        );
        assert_eq!(
            global_cache.tools.len(),
            0,
            "Global cache should start with no tool stats"
        );
    }

    #[tokio::test]
    async fn stats_store_loads_existing_local_stats() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let stats_dir = workspace_root.join(".lsp-mcp");
        tokio::fs::create_dir_all(&stats_dir).await.unwrap();

        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let existing_stats = ToolStatsFile {
            version: 1,
            last_updated: Utc::now(),
            tools: {
                let mut map = HashMap::new();
                map.insert(
                    "hover".to_string(),
                    ToolStat {
                        total_calls: 42,
                        success_count: 40,
                        failure_count: 2,
                    },
                );
                map
            },
        };

        let stats_path = stats_dir.join("tool-stats.json");
        let json = serde_json::to_string_pretty(&existing_stats).unwrap();
        tokio::fs::write(&stats_path, json).await.unwrap();

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;

        let local_cache = store.local_cache.lock();
        let hover_stat = local_cache.tools.get("hover").unwrap();
        assert_eq!(
            hover_stat.total_calls, 42,
            "Should load existing local stats"
        );
        assert_eq!(
            hover_stat.success_count, 40,
            "Should load existing success count"
        );
        assert_eq!(
            hover_stat.failure_count, 2,
            "Should load existing failure count"
        );
    }

    #[tokio::test]
    async fn stats_store_record_call_increments_success_count() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;

        store.record_call("hover", true).await;

        let local_cache = store.local_cache.lock();
        let hover_stat = local_cache.tools.get("hover").unwrap();
        assert_eq!(
            hover_stat.total_calls, 1,
            "Total calls should be incremented"
        );
        assert_eq!(
            hover_stat.success_count, 1,
            "Success count should be incremented"
        );
        assert_eq!(
            hover_stat.failure_count, 0,
            "Failure count should remain zero"
        );
    }

    #[tokio::test]
    async fn stats_store_record_call_increments_failure_count() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;

        store.record_call("hover", false).await;

        let local_cache = store.local_cache.lock();
        let hover_stat = local_cache.tools.get("hover").unwrap();
        assert_eq!(
            hover_stat.total_calls, 1,
            "Total calls should be incremented"
        );
        assert_eq!(
            hover_stat.success_count, 0,
            "Success count should remain zero"
        );
        assert_eq!(
            hover_stat.failure_count, 1,
            "Failure count should be incremented"
        );
    }

    #[tokio::test]
    async fn stats_store_record_call_updates_both_local_and_global() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;

        store.record_call("hover", true).await;

        let local_cache = store.local_cache.lock();
        let local_hover = local_cache.tools.get("hover").unwrap();
        assert_eq!(local_hover.total_calls, 1, "Local cache should be updated");

        drop(local_cache);

        let global_cache = store.global_cache.lock();
        let global_hover = global_cache.tools.get("hover").unwrap();
        assert_eq!(
            global_hover.total_calls, 1,
            "Global cache should be updated"
        );
    }

    #[tokio::test]
    async fn stats_store_persists_to_local_file() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;
        store.record_call("hover", true).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats_path = workspace_root.join(".lsp-mcp").join("tool-stats.json");
        let json_content = tokio::fs::read_to_string(&stats_path).await.unwrap();
        let persisted: ToolStatsFile = serde_json::from_str(&json_content).unwrap();

        let hover_stat = persisted.tools.get("hover").unwrap();
        assert_eq!(
            hover_stat.total_calls, 1,
            "Stats should be persisted to local file"
        );
    }

    #[tokio::test]
    async fn stats_store_persists_to_global_file() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let global_temp = TempDir::new().unwrap();
        let global_path = global_temp.path().join("tool-stats.json");

        let store = StatsStore::new_with_global_path(workspace_root, &global_path).await;
        store.record_call("hover", true).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let global_cache = store.global_cache.lock();
        let global_hover = global_cache.tools.get("hover").unwrap();
        assert_eq!(
            global_hover.total_calls, 1,
            "Global cache should have stats after record_call"
        );
    }
}
