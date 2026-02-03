//! Audit trail logging

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Local>,
    pub event: AuditEvent,
    pub remote: Option<String>,
    pub path: Option<String>,
    pub size: Option<u64>,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    ServerStart,
    ServerStop,
    Connect,
    Disconnect,
    FileReceived,
    FileRejected,
    ListRequest,
    StatusRequest,
    Error,
}

impl AuditEntry {
    pub fn new(event: AuditEvent) -> Self {
        Self {
            timestamp: Local::now(),
            event,
            remote: None,
            path: None,
            size: None,
            success: true,
            message: None,
        }
    }

    pub fn with_remote(mut self, addr: SocketAddr) -> Self {
        self.remote = Some(addr.to_string());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// Audit logger that writes to a JSONL file
pub struct AuditLogger {
    tx: mpsc::Sender<AuditEntry>,
}

impl AuditLogger {
    /// Start the audit logger with the given log file path
    pub async fn new(log_path: PathBuf) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel::<AuditEntry>(100);

        // Spawn background writer task
        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                if let Err(e) = write_entry(&log_path, &entry).await {
                    eprintln!("Failed to write audit log: {}", e);
                }
            }
        });

        Ok(Self { tx })
    }

    /// Log an audit entry
    pub async fn log(&self, entry: AuditEntry) {
        let _ = self.tx.send(entry).await;
    }

    /// Get a clone of the sender for sharing across tasks
    pub fn sender(&self) -> mpsc::Sender<AuditEntry> {
        self.tx.clone()
    }
}

async fn write_entry(path: &Path, entry: &AuditEntry) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    let json = serde_json::to_string(entry)?;
    file.write_all(json.as_bytes()).await?;
    file.write_all(b"\n").await?;

    Ok(())
}

/// Format for human-readable log output
impl std::fmt::Display for AuditEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ts = self.timestamp.format("%Y-%m-%d %H:%M:%S");
        let status = if self.success { "✓" } else { "✗" };
        let remote = self.remote.as_deref().unwrap_or("-");
        let path = self.path.as_deref().unwrap_or("-");
        
        write!(f, "[{}] {} {:?} from {} path={}", ts, status, self.event, remote, path)?;
        
        if let Some(size) = self.size {
            write!(f, " size={}", size)?;
        }
        if let Some(ref msg) = self.message {
            write!(f, " ({})", msg)?;
        }
        
        Ok(())
    }
}
