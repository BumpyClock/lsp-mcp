// ABOUTME: Workspace documents module for file caching and language pattern constants
// ABOUTME: Re-exports handler, constants, and range utilities for workspace file management

mod constants;
mod handler;
mod range;

pub use constants::*;
pub use handler::{DidOpenConfiguration, WorkspaceDocuments, WorkspaceDocumentsHandler};

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;
    use notify_debouncer_mini::DebouncedEventKind;
    use std::{error::Error, fs, time::Duration};
    use tempfile::tempdir;
    use tokio::sync::broadcast::{channel, Receiver, Sender};
    use notify_debouncer_mini::DebouncedEvent;

    fn create_test_watcher_channels() -> (Sender<DebouncedEvent>, Receiver<DebouncedEvent>) {
        channel(100)
    }

    #[tokio::test]
    async fn test_read_text_document() -> Result<(), Box<dyn Error + Send + Sync>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, world!\nThis is a test.")?;
        let (_, rx) = create_test_watcher_channels();
        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.txt".to_string()],
            vec![],
            rx,
            DidOpenConfiguration::None,
        );

        let content = handler.read_text_document(&file_path, None).await?;
        assert_eq!(content, "Hello, world!\nThis is a test.");

        let range = Range {
            start: lsp_types::Position {
                line: 0,
                character: 7,
            },
            end: lsp_types::Position {
                line: 0,
                character: 12,
            },
        };
        let extracted = handler.read_text_document(&file_path, Some(range)).await?;
        assert_eq!(extracted, "world");

        Ok(())
    }

    #[tokio::test]
    async fn test_list_files() -> Result<(), Box<dyn Error + Send + Sync>> {
        let dir = tempdir()?;
        fs::write(dir.path().join("file1.rs"), "fn main() {}")?;
        fs::write(dir.path().join("file2.txt"), "Hello")?;
        let (tx, rx) = create_test_watcher_channels();

        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.rs".to_string()],
            vec!["file2.txt".to_string()],
            rx,
            DidOpenConfiguration::None,
        );

        let files = handler.list_files().await;
        assert_eq!(files.len(), 1);
        assert!(files.contains(&dir.path().join("file1.rs")));

        fs::write(dir.path().join("file3.rs"), "fn main() {}")?;
        tx.send(DebouncedEvent {
            path: dir.path().join("file3.rs"),
            kind: DebouncedEventKind::Any,
        })?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let files = handler.list_files().await;
        assert_eq!(files.len(), 2);
        assert!(files.contains(&dir.path().join("file1.rs")));
        assert!(files.contains(&dir.path().join("file3.rs")));

        Ok(())
    }

    #[tokio::test]
    async fn test_read_text_document_out_of_bounds() -> Result<(), Box<dyn Error + Send + Sync>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_out_of_bounds.txt");
        fs::write(&file_path, "Line 1\nLine 2")?;
        let (_, rx) = create_test_watcher_channels();
        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.txt".to_string()],
            vec![],
            rx,
            DidOpenConfiguration::None,
        );

        let range = Range {
            start: lsp_types::Position {
                line: 5,
                character: 0,
            },
            end: lsp_types::Position {
                line: 6,
                character: 10,
            },
        };
        let extracted = handler.read_text_document(&file_path, Some(range)).await?;
        assert_eq!(extracted, "Line 2");

        Ok(())
    }

    #[tokio::test]
    async fn test_read_text_document_invalid_characters() -> Result<(), Box<dyn Error + Send + Sync>>
    {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_invalid_chars.txt");
        fs::write(&file_path, "Short line")?;

        let (_, rx) = create_test_watcher_channels();
        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.txt".to_string()],
            vec![],
            rx,
            DidOpenConfiguration::None,
        );

        let range = Range {
            start: lsp_types::Position {
                line: 0,
                character: 100,
            },
            end: lsp_types::Position {
                line: 0,
                character: 200,
            },
        };
        let extracted = handler.read_text_document(&file_path, Some(range)).await?;
        assert_eq!(extracted, "");

        Ok(())
    }

    #[tokio::test]
    async fn test_read_text_document_empty_file() -> Result<(), Box<dyn Error + Send + Sync>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("empty.txt");
        fs::write(&file_path, "")?;

        let (_, rx) = create_test_watcher_channels();
        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.txt".to_string()],
            vec![],
            rx,
            DidOpenConfiguration::None,
        );

        let content = handler.read_text_document(&file_path, None).await?;
        assert_eq!(content, "");

        let range = Range {
            start: lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: lsp_types::Position {
                line: 0,
                character: 10,
            },
        };
        let extracted = handler.read_text_document(&file_path, Some(range)).await?;
        assert_eq!(extracted, "");

        Ok(())
    }

    #[tokio::test]
    async fn test_list_files_no_matching_files() -> Result<(), Box<dyn Error + Send + Sync>> {
        let dir = tempdir()?;
        fs::write(dir.path().join("file1.rs"), "fn main() {}")?;
        let (_, rx) = create_test_watcher_channels();
        let handler = WorkspaceDocumentsHandler::new(
            dir.path(),
            vec!["*.txt".to_string()],
            vec!["*.md".to_string()],
            rx,
            DidOpenConfiguration::None,
        );

        let files = handler.list_files().await;
        assert!(files.is_empty());

        Ok(())
    }
}
