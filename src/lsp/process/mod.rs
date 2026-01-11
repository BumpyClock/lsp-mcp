// ABOUTME: Process module for LSP subprocess management and health monitoring
// ABOUTME: Re-exports ProcessHandler, Process trait, and ProcessHealth state

mod handler;
mod health;

pub use handler::{Process, ProcessHandler};
pub use health::ProcessHealth;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;
    use tokio::time::timeout;

    async fn spawn_test_process() -> tokio::process::Child {
        Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cat command should be available")
    }

    #[tokio::test]
    async fn process_handler_reports_healthy_status_initially() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        let health = handler.health_receiver().borrow().clone();

        assert_eq!(
            health,
            ProcessHealth::Healthy,
            "expected Healthy status initially, got {:?}",
            health
        );
    }

    #[tokio::test]
    async fn process_handler_is_alive_returns_true_for_running_process() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        let alive = handler.is_alive().await;

        assert!(
            alive,
            "expected is_alive to return true for running process"
        );
    }

    #[tokio::test]
    async fn process_handler_kill_terminates_process() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        handler.kill().await.expect("kill should succeed");
        let alive = handler.is_alive().await;

        assert!(!alive, "expected is_alive to return false after kill");
    }

    #[tokio::test]
    async fn process_handler_report_unhealthy_updates_health_channel() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        handler.report_unhealthy("test failure reason".to_string());
        let health = handler.health_receiver().borrow().clone();

        match health {
            ProcessHealth::Unhealthy(reason) => {
                assert_eq!(
                    reason, "test failure reason",
                    "expected reason 'test failure reason', got '{}'",
                    reason
                );
            }
            other => panic!(
                "expected Unhealthy status after report_unhealthy, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn process_handler_report_dead_updates_health_channel() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        handler.report_dead();
        let health = handler.health_receiver().borrow().clone();

        assert_eq!(
            health,
            ProcessHealth::Dead,
            "expected Dead status after report_dead, got {:?}",
            health
        );
    }

    #[tokio::test]
    async fn process_handler_health_receiver_receives_updates() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");
        let mut receiver = handler.health_receiver();

        handler.report_unhealthy("connection lost".to_string());
        let changed = timeout(Duration::from_millis(100), receiver.changed()).await;

        assert!(
            changed.is_ok(),
            "expected health receiver to receive update within timeout"
        );
        let health = receiver.borrow().clone();
        match health {
            ProcessHealth::Unhealthy(_) => {}
            other => panic!("expected Unhealthy, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn process_handler_is_alive_returns_false_after_process_exits() {
        let child = Command::new("true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("true command should be available");

        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        tokio::time::sleep(Duration::from_millis(100)).await;
        let alive = handler.is_alive().await;

        assert!(
            !alive,
            "expected is_alive to return false after process exits"
        );
    }
}
