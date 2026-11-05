use notify::{PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_mini::{new_debouncer_opt, Config, DebounceEventResult, Debouncer};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub(crate) enum FileWatcher {
    Native {
        _guard: Debouncer<RecommendedWatcher>,
    },
    Polling {
        _guard: Debouncer<PollWatcher>,
    },
}

impl FileWatcher {
    pub(crate) fn new(
        root: &Path,
        interval: Duration,
        handler: impl Fn(DebounceEventResult) + Send + Sync + 'static,
    ) -> Result<Self, notify::Error> {
        std::fs::metadata(root)
            .map_err(|error| notify::Error::io(error).add_path(root.to_path_buf()))?;
        let handler = Arc::new(handler);
        match Self::watch::<RecommendedWatcher>(root, interval, handler.clone()) {
            Ok(guard) => Ok(Self::Native { _guard: guard }),
            Err(native_error) => {
                tracing::warn!(path = %root.display(), error = %native_error, "Native file watching unavailable; using polling");
                let guard =
                    Self::watch::<PollWatcher>(root, interval, handler).map_err(|poll_error| {
                        notify::Error::generic(&format!(
                            "Native watcher: {native_error}; polling watcher: {poll_error}"
                        ))
                        .add_path(root.to_path_buf())
                    })?;
                Ok(Self::Polling { _guard: guard })
            }
        }
    }

    fn watch<W: Watcher>(
        root: &Path,
        interval: Duration,
        handler: Arc<dyn Fn(DebounceEventResult) + Send + Sync>,
    ) -> Result<Debouncer<W>, notify::Error> {
        let config = Config::default()
            .with_timeout(interval)
            .with_notify_config(notify::Config::default().with_poll_interval(interval));
        let mut guard = new_debouncer_opt::<_, W>(config, move |events| handler(events))?;
        guard.watcher().watch(root, RecursiveMode::Recursive)?;
        Ok(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn polling_delivers_changes_until_the_guard_is_dropped() {
        let root = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let guard = FileWatcher::watch::<PollWatcher>(
            root.path(),
            Duration::from_millis(50),
            Arc::new(move |events| {
                for event in events.unwrap() {
                    let _ = tx.send(event.path);
                }
            }),
        )
        .unwrap();
        let path = root.path().join("watched.rs");
        std::fs::write(&path, "fn created() {}\n").unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while rx.recv().await.unwrap() != path {}
        })
        .await
        .expect("polling must report file creation");
        drop(guard);
        while rx.try_recv().is_ok() {}
        assert!(
            rx.recv().await.is_none(),
            "dropping the watcher must release its callback"
        );
    }

    #[test]
    fn invalid_workspace_returns_an_error_with_its_path() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        let error = FileWatcher::new(&missing, Duration::from_millis(50), |_| {})
            .err()
            .unwrap();
        assert!(error.paths.contains(&missing));
    }
}
