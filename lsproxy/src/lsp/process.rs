// ABOUTME: Process handler for managing LSP server subprocess communication
// ABOUTME: Provides health monitoring, lifecycle management, and stdin/stdout communication

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::watch;
use tokio::sync::Mutex;

/// Represents the current health state of the LSP process
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessHealth {
    Healthy,
    Unhealthy(String),
    Dead,
}

#[async_trait::async_trait]
pub trait Process: Send + Sync {
    async fn send(&mut self, data: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn receive(&self) -> Result<String, Box<dyn Error + Send + Sync>>;
}

#[derive(Clone)]
pub struct ProcessHandler {
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    child: Arc<Mutex<Option<Child>>>,
    health_tx: Arc<watch::Sender<ProcessHealth>>,
    health_rx: watch::Receiver<ProcessHealth>,
    is_dead: Arc<AtomicBool>,
}

impl ProcessHandler {
    pub async fn new(mut child: Child) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open stdout")?;
        let (health_tx, health_rx) = watch::channel(ProcessHealth::Healthy);
        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            child: Arc::new(Mutex::new(Some(child))),
            health_tx: Arc::new(health_tx),
            health_rx,
            is_dead: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Check if the child process is still running
    pub async fn is_alive(&self) -> bool {
        if self.is_dead.load(Ordering::SeqCst) {
            return false;
        }
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    self.is_dead.store(true, Ordering::SeqCst);
                    false
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Kill the child process gracefully
    pub async fn kill(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            child.kill().await?;
            self.is_dead.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Get a receiver to monitor health status
    pub fn health_receiver(&self) -> watch::Receiver<ProcessHealth> {
        self.health_rx.clone()
    }

    /// Report that the process is unhealthy (called by response listener on error)
    pub fn report_unhealthy(&self, reason: String) {
        self.is_dead.store(true, Ordering::SeqCst);
        let _ = self.health_tx.send(ProcessHealth::Unhealthy(reason));
    }

    /// Report that the process is dead
    pub fn report_dead(&self) {
        self.is_dead.store(true, Ordering::SeqCst);
        let _ = self.health_tx.send(ProcessHealth::Dead);
    }
}

#[async_trait::async_trait]
impl Process for ProcessHandler {
    async fn send(&mut self, data: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(data.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn receive(&self) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut content_length: Option<usize> = None;
        let mut buffer = Vec::new();

        loop {
            let mut stdout = self.stdout.lock().await;
            let n = stdout.read_until(b'\n', &mut buffer).await?;
            if n == 0 {
                return Err("LSP process terminated (EOF)".into());
            }

            let line = String::from_utf8_lossy(&buffer[buffer.len() - n..]);
            if line.trim().is_empty() && content_length.is_some() {
                let length =
                    content_length.ok_or("Missing Content-Length header in LSP message")?;
                let mut content = vec![0; length];
                stdout.read_exact(&mut content).await?;
                return Ok(String::from_utf8(content)?);
            } else if line.starts_with("Content-Length: ") {
                content_length = Some(line.trim_start_matches("Content-Length: ").trim().parse()?);
            }
            buffer.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;
    use tokio::time::timeout;

    async fn spawn_test_process() -> Child {
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

        assert!(alive, "expected is_alive to return true for running process");
    }

    #[tokio::test]
    async fn process_handler_kill_terminates_process() {
        let child = spawn_test_process().await;
        let handler = ProcessHandler::new(child)
            .await
            .expect("ProcessHandler should be created");

        handler.kill().await.expect("kill should succeed");
        let alive = handler.is_alive().await;

        assert!(
            !alive,
            "expected is_alive to return false after kill"
        );
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
