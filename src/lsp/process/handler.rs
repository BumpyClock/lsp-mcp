// ABOUTME: Process handler for managing LSP server subprocess communication
// ABOUTME: Provides lifecycle management, stdin/stdout communication, and health reporting

use super::health::ProcessHealth;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::watch;
use tokio::sync::Mutex;

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
