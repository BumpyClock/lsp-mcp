use super::detection::detect_languages_in_workspace;
use super::Manager;
use crate::api_types::{LspStatus, SupportedLanguages};
use crate::config::{LanguageSelection, LspMcpConfig};
use crate::lsp::registry::LanguageMetadata;
use std::time::Duration;

#[test]
fn detects_registry_languages_once_in_stable_order_and_respects_exclusions() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.path().join("other.rs"), "fn other() {}\n").unwrap();
    std::fs::write(root.path().join("main.py"), "print('hello')\n").unwrap();
    std::fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
    for (directory, filename) in [
        ("ignored", "file.go"),
        ("target", "file.rb"),
        ("node_modules", "file.ts"),
    ] {
        std::fs::create_dir(root.path().join(directory)).unwrap();
        std::fs::write(root.path().join(directory).join(filename), "").unwrap();
    }
    assert_eq!(
        detect_languages_in_workspace(root.path().to_str().unwrap()),
        vec![SupportedLanguages::Python, SupportedLanguages::Rust]
    );
}

#[tokio::test]
async fn settings_only_config_detects_languages_and_preserves_missing_binary_override() {
    let root = tempfile::tempdir().unwrap();
    let missing = root
        .path()
        .join("missing-rust-server")
        .to_string_lossy()
        .into_owned();
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        root.path().join(".lsp-mcp.json"),
        serde_json::to_vec(&serde_json::json!({
            "binaries": {"rust": missing}, "output": {"mode":"verbose"}
        }))
        .unwrap(),
    )
    .unwrap();
    let config = LspMcpConfig::load_project(root.path()).unwrap().unwrap();
    assert_eq!(config.languages, LanguageSelection::Auto);
    let manager = Manager::new(root.path().to_str().unwrap()).await.unwrap();
    let manager = std::sync::Arc::new(manager);
    manager
        .start_langservers_async(root.path().to_str().unwrap(), Some(config))
        .await;
    assert!(
        manager
            .wait_for_pending_clients(Duration::from_secs(5))
            .await
    );
    let health = manager.language_health().await;
    assert_eq!(
        health[&SupportedLanguages::Rust].status,
        LspStatus::MissingExecutable
    );
    assert_eq!(
        health[&SupportedLanguages::Rust].command.as_deref(),
        Some(missing.as_str())
    );
    assert!(health[&SupportedLanguages::Rust]
        .reason
        .as_ref()
        .unwrap()
        .contains("binaries.rust"));
    assert_eq!(
        health[&SupportedLanguages::Ruby].status,
        LspStatus::NotDetected
    );
    assert_eq!(health.len(), LanguageMetadata::all().count());
    assert!(manager.pending_languages().await.is_empty());
    assert!(manager.get_client(SupportedLanguages::Rust).await.is_none());
    let error = manager
        .ensure_client(SupportedLanguages::Rust)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("missing-rust-server"), "{error}");
}

#[tokio::test]
async fn explicit_selection_and_empty_list_disable_unselected_file_requests() {
    for selection in [vec![], vec![SupportedLanguages::Python]] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
        let mut config = LspMcpConfig {
            languages: LanguageSelection::Explicit(selection),
            ..Default::default()
        };
        config.binaries.insert(
            "python".into(),
            root.path()
                .join("missing-python")
                .to_string_lossy()
                .into_owned(),
        );
        let manager = Manager::new(root.path().to_str().unwrap()).await.unwrap();
        let manager = std::sync::Arc::new(manager);
        manager
            .start_langservers_async(root.path().to_str().unwrap(), Some(config))
            .await;
        assert!(
            manager
                .wait_for_pending_clients(Duration::from_secs(5))
                .await
        );
        assert_eq!(
            manager.language_health().await[&SupportedLanguages::Rust].status,
            LspStatus::Disabled
        );
        let error = manager
            .hover("main.rs", lsp_types::Position::new(0, 3))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.to_lowercase().contains("disabled"), "{error}");
        assert!(manager.get_client(SupportedLanguages::Rust).await.is_none());
    }
}

#[tokio::test]
async fn no_config_health_does_not_probe_or_start_any_server() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::new(root.path().to_str().unwrap()).await.unwrap();
    let manager = std::sync::Arc::new(manager);
    manager
        .start_langservers_async(root.path().to_str().unwrap(), None)
        .await;
    assert!(
        manager
            .wait_for_pending_clients(Duration::from_secs(5))
            .await
    );
    let first = manager.language_health().await;
    assert!(first
        .values()
        .all(|health| health.status == LspStatus::NotDetected));
    assert_eq!(first, manager.language_health().await);
    assert!(
        manager
            .wait_for_pending_clients(Duration::from_millis(10))
            .await
    );
    assert!(manager.pending_languages().await.is_empty());
}

#[test]
fn registry_resolves_absolute_and_workspace_relative_executable_overrides() {
    let executable = std::env::current_exe().unwrap();
    let metadata = LanguageMetadata::get(SupportedLanguages::Rust).unwrap();
    assert_eq!(
        SupportedLanguages::TypeScriptJavaScript.to_string(),
        "typescript"
    );
    let directory = executable.parent().unwrap();
    assert_eq!(
        metadata
            .resolve_binary(executable.to_str(), directory)
            .unwrap(),
        executable
    );
    let relative = std::path::Path::new(".").join(executable.file_name().unwrap());
    assert_eq!(
        metadata
            .resolve_binary(relative.to_str(), directory)
            .unwrap(),
        directory.join(relative)
    );
}
