// ABOUTME: Reconnection infrastructure for LSP clients
// ABOUTME: Provides SpawnConfig, DocumentTracker, and ReconnectController for auto-reconnection

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Configuration for spawning an LSP process
#[derive(Clone, Debug)]
pub struct SpawnConfig {
    /// Path to the LSP server binary
    pub binary: String,
    /// Arguments to pass to the binary
    pub args: Vec<String>,
    /// Working directory for the process
    pub working_dir: PathBuf,
}

impl SpawnConfig {
    pub fn new(binary: impl Into<String>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            args: Vec::new(),
            working_dir: working_dir.into(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

/// Tracks opened documents for re-opening after reconnection
#[derive(Clone, Debug, Default)]
pub struct DocumentTracker {
    /// Maps file paths to their last known version
    documents: Arc<Mutex<HashMap<String, i32>>>,
}

impl DocumentTracker {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Track a document that was opened
    pub async fn track_document(&self, file_path: String, version: i32) {
        let mut docs = self.documents.lock().await;
        docs.insert(file_path, version);
    }

    /// Remove a document from tracking (e.g., when closed)
    pub async fn untrack_document(&self, file_path: &str) {
        let mut docs = self.documents.lock().await;
        docs.remove(file_path);
    }

    /// Get all tracked documents
    pub async fn get_tracked_documents(&self) -> Vec<(String, i32)> {
        let docs = self.documents.lock().await;
        docs.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }

    /// Clear all tracked documents (e.g., after reconnection)
    pub async fn clear(&self) {
        let mut docs = self.documents.lock().await;
        docs.clear();
    }
}

/// Controls reconnection with exponential backoff
#[derive(Clone, Debug)]
pub struct ReconnectController {
    /// Maximum number of reconnection attempts
    pub max_attempts: u32,
    /// Base delay between attempts
    pub base_delay: Duration,
    /// Maximum delay between attempts
    pub max_delay: Duration,
    /// Current attempt counter
    attempts: Arc<Mutex<u32>>,
}

impl ReconnectController {
    pub fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            base_delay,
            max_delay,
            attempts: Arc::new(Mutex::new(0)),
        }
    }

    /// Calculate the delay for the next reconnection attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms = self.base_delay.as_millis() as u64 * 2u64.pow(attempt);
        let delay = Duration::from_millis(delay_ms);
        std::cmp::min(delay, self.max_delay)
    }

    /// Check if more reconnection attempts are allowed
    pub async fn can_attempt(&self) -> bool {
        let attempts = self.attempts.lock().await;
        *attempts < self.max_attempts
    }

    /// Increment the attempt counter and return the current attempt number
    pub async fn record_attempt(&self) -> u32 {
        let mut attempts = self.attempts.lock().await;
        *attempts += 1;
        *attempts
    }

    /// Reset the attempt counter (call after successful reconnection)
    pub async fn reset(&self) {
        let mut attempts = self.attempts.lock().await;
        *attempts = 0;
    }

    /// Get the current attempt count
    pub async fn current_attempts(&self) -> u32 {
        let attempts = self.attempts.lock().await;
        *attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SpawnConfig tests
    #[test]
    fn spawn_config_new_creates_config_with_binary_and_working_dir() {
        let config = SpawnConfig::new("rust-analyzer", "/home/user/project");

        assert_eq!(config.binary, "rust-analyzer");
        assert_eq!(config.working_dir, PathBuf::from("/home/user/project"));
        assert!(config.args.is_empty());
    }

    #[test]
    fn spawn_config_with_args_adds_arguments() {
        let config = SpawnConfig::new("rust-analyzer", "/home/user/project")
            .with_args(vec!["--log-file".to_string(), "/tmp/ra.log".to_string()]);

        assert_eq!(config.args.len(), 2);
        assert_eq!(config.args[0], "--log-file");
        assert_eq!(config.args[1], "/tmp/ra.log");
    }

    // DocumentTracker tests
    #[tokio::test]
    async fn document_tracker_track_document_stores_file_path_and_version() {
        let tracker = DocumentTracker::new();

        tracker
            .track_document("/path/to/file.rs".to_string(), 1)
            .await;
        let docs = tracker.get_tracked_documents().await;

        assert_eq!(docs.len(), 1);
        assert!(docs
            .iter()
            .any(|(path, version)| path == "/path/to/file.rs" && *version == 1));
    }

    #[tokio::test]
    async fn document_tracker_untrack_document_removes_file() {
        let tracker = DocumentTracker::new();
        tracker
            .track_document("/path/to/file.rs".to_string(), 1)
            .await;

        tracker.untrack_document("/path/to/file.rs").await;
        let docs = tracker.get_tracked_documents().await;

        assert!(docs.is_empty());
    }

    #[tokio::test]
    async fn document_tracker_clear_removes_all_documents() {
        let tracker = DocumentTracker::new();
        tracker
            .track_document("/path/to/file1.rs".to_string(), 1)
            .await;
        tracker
            .track_document("/path/to/file2.rs".to_string(), 2)
            .await;

        tracker.clear().await;
        let docs = tracker.get_tracked_documents().await;

        assert!(docs.is_empty());
    }

    #[tokio::test]
    async fn document_tracker_tracks_multiple_documents() {
        let tracker = DocumentTracker::new();

        tracker.track_document("/file1.rs".to_string(), 1).await;
        tracker.track_document("/file2.rs".to_string(), 2).await;
        tracker.track_document("/file3.rs".to_string(), 3).await;
        let docs = tracker.get_tracked_documents().await;

        assert_eq!(docs.len(), 3);
    }

    // ReconnectController tests
    #[tokio::test]
    async fn reconnect_controller_can_attempt_returns_true_when_under_max() {
        let controller =
            ReconnectController::new(3, Duration::from_secs(1), Duration::from_secs(30));

        let can_attempt = controller.can_attempt().await;

        assert!(can_attempt);
    }

    #[tokio::test]
    async fn reconnect_controller_can_attempt_returns_false_after_max_attempts() {
        let controller =
            ReconnectController::new(2, Duration::from_secs(1), Duration::from_secs(30));
        controller.record_attempt().await;
        controller.record_attempt().await;

        let can_attempt = controller.can_attempt().await;

        assert!(!can_attempt);
    }

    #[tokio::test]
    async fn reconnect_controller_record_attempt_increments_counter() {
        let controller =
            ReconnectController::new(3, Duration::from_secs(1), Duration::from_secs(30));

        let attempt1 = controller.record_attempt().await;
        let attempt2 = controller.record_attempt().await;

        assert_eq!(attempt1, 1);
        assert_eq!(attempt2, 2);
    }

    #[tokio::test]
    async fn reconnect_controller_reset_clears_attempt_counter() {
        let controller =
            ReconnectController::new(3, Duration::from_secs(1), Duration::from_secs(30));
        controller.record_attempt().await;
        controller.record_attempt().await;

        controller.reset().await;
        let attempts = controller.current_attempts().await;

        assert_eq!(attempts, 0);
    }

    #[test]
    fn reconnect_controller_calculate_delay_uses_exponential_backoff() {
        let controller =
            ReconnectController::new(3, Duration::from_secs(1), Duration::from_secs(30));

        let delay0 = controller.calculate_delay(0);
        let delay1 = controller.calculate_delay(1);
        let delay2 = controller.calculate_delay(2);

        assert_eq!(delay0, Duration::from_secs(1));
        assert_eq!(delay1, Duration::from_secs(2));
        assert_eq!(delay2, Duration::from_secs(4));
    }

    #[test]
    fn reconnect_controller_calculate_delay_caps_at_max_delay() {
        let controller =
            ReconnectController::new(10, Duration::from_secs(1), Duration::from_secs(10));

        let delay5 = controller.calculate_delay(5); // Would be 32s without cap

        assert_eq!(delay5, Duration::from_secs(10));
    }
}
