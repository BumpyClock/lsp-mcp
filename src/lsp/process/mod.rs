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

    #[tokio::test]
    async fn cleanup_write_survives_temporary_backpressure() {
        let child = Command::new("python3")
            .args([
                "-c",
                r#"
import sys, time
sys.stdout.write('Content-Length: 2\r\n\r\n{}')
sys.stdout.flush()
time.sleep(0.5)
length = int(sys.stdin.buffer.readline().split(b':')[1])
sys.stdin.buffer.readline()
sys.stdin.buffer.read(length)
sys.stdout.write('Content-Length: 2\r\n\r\n{}')
sys.stdout.flush()
time.sleep(60)
"#,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let handler = ProcessHandler::new(child).await.unwrap();
        timeout(Duration::from_secs(5), async {
            handler.receive().await.unwrap();
            let body = "x".repeat(8 * 1024 * 1024);
            let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
            let sent = handler
                .send_with_lock_timeout(&frame, Duration::from_millis(25))
                .await;
            assert!(
                sent.is_ok(),
                "optional cleanup must not interrupt an owned frame"
            );
            assert_eq!(handler.receive().await.unwrap(), "{}");
            assert!(handler.is_alive().await);
            handler.kill().await.unwrap();
        })
        .await
        .expect("cleanup transport scenario must complete");
    }

    #[tokio::test]
    async fn cleanup_wait_for_busy_writer_is_bounded() {
        let mut handler = ProcessHandler::new(spawn_test_process().await)
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            let stdin = handler.stdin.lock().await;
            let frame = "Content-Length: 2\r\n\r\n{}";
            let result = timeout(
                Duration::from_secs(1),
                handler.send_with_lock_timeout(frame, Duration::from_millis(25)),
            )
            .await
            .unwrap();
            assert!(result.is_err());
            assert!(handler.is_alive().await);
            drop(stdin);
            handler.send(frame).await.unwrap();
            assert_eq!(handler.receive().await.unwrap(), "{}");
            handler.kill().await.unwrap();
        })
        .await
        .expect("cleanup transport scenario must complete");
    }

    #[tokio::test]
    async fn interrupted_frame_transmission_invalidates_the_connection() {
        let child = Command::new("python3")
            .args(["-c", "import time; time.sleep(60)"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut handler = ProcessHandler::new(child).await.unwrap();
        let body = "x".repeat(8 * 1024 * 1024);
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        assert!(timeout(Duration::from_millis(25), handler.send(&frame))
            .await
            .is_err());
        assert!(
            !matches!(*handler.health_receiver().borrow(), ProcessHealth::Healthy),
            "a partial frame must not leave the connection reusable"
        );
        assert!(handler.send("Content-Length: 2\r\n\r\n{}").await.is_err());
        handler.kill().await.unwrap();
    }
}
