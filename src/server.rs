//! QUIC server - receives files

use anyhow::Result;
use quinn::Endpoint;
use std::net::SocketAddr;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::audit::{AuditEntry, AuditEvent, AuditLogger};
use crate::protocol::{Request, Response};
use crate::tls;

pub async fn run(bind: &str, root: &Path, audit_log: &Path) -> Result<()> {
    // Ensure root directory exists
    fs::create_dir_all(root).await?;
    
    // Setup audit logger
    let logger = AuditLogger::new(audit_log.to_path_buf()).await?;
    logger.log(AuditEntry::new(AuditEvent::ServerStart)
        .with_message(format!("bind={} root={}", bind, root.display()))).await;
    
    // Setup TLS
    let (cert, key) = tls::generate_self_signed()?;
    let server_config = tls::server_config(cert, key)?;
    
    // Bind endpoint
    let endpoint = Endpoint::server(server_config, bind.parse()?)?;
    tracing::info!("🚀 Server listening on {}", bind);
    tracing::info!("📁 Root: {:?}", root);
    tracing::info!("📋 Audit log: {:?}", audit_log);
    
    // Accept connections
    while let Some(incoming) = endpoint.accept().await {
        let root = root.to_path_buf();
        let audit_tx = logger.sender();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(incoming, &root, audit_tx).await {
                tracing::error!("Connection error: {}", e);
            }
        });
    }
    
    Ok(())
}

async fn handle_connection(
    incoming: quinn::Incoming,
    root: &Path,
    audit_tx: mpsc::Sender<AuditEntry>,
) -> Result<()> {
    let connection = incoming.await?;
    let remote = connection.remote_address();
    tracing::info!("📥 Connection from {}", remote);
    
    // Log connection
    let _ = audit_tx.send(AuditEntry::new(AuditEvent::Connect)
        .with_remote(remote)).await;
    
    loop {
        // Accept bidirectional stream
        let stream = match connection.accept_bi().await {
            Ok(s) => s,
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                tracing::info!("Connection closed by peer");
                let _ = audit_tx.send(AuditEntry::new(AuditEvent::Disconnect)
                    .with_remote(remote)).await;
                break;
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                let _ = audit_tx.send(AuditEntry::new(AuditEvent::Error)
                    .with_remote(remote)
                    .with_success(false)
                    .with_message(e.to_string())).await;
                break;
            }
        };
        
        let (send, recv) = stream;
        let root = root.to_path_buf();
        let tx = audit_tx.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_stream(send, recv, &root, remote, tx).await {
                tracing::error!("Stream error: {}", e);
            }
        });
    }
    
    Ok(())
}

async fn handle_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    root: &Path,
    remote: SocketAddr,
    audit_tx: mpsc::Sender<AuditEntry>,
) -> Result<()> {
    // Read request header (length-prefixed JSON)
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    
    let mut req_buf = vec![0u8; len];
    recv.read_exact(&mut req_buf).await?;
    
    let request: Request = serde_json::from_slice(&req_buf)?;
    tracing::debug!("Request: {:?}", request);
    
    match request {
        Request::Put { path, size, hash } => {
            let result = handle_put(&mut send, &mut recv, root, &path, size, hash.as_deref()).await;
            let success = result.is_ok();
            let _ = audit_tx.send(AuditEntry::new(AuditEvent::FileReceived)
                .with_remote(remote)
                .with_path(&path)
                .with_size(size)
                .with_success(success)
                .with_message(if success { "OK".to_string() } else { format!("{:?}", result) })).await;
            result?;
        }
        Request::List { path, recursive, long } => {
            let _ = audit_tx.send(AuditEntry::new(AuditEvent::ListRequest)
                .with_remote(remote)
                .with_path(&path)).await;
            handle_list(&mut send, root, &path, recursive, long).await?;
        }
        Request::Status => {
            let _ = audit_tx.send(AuditEntry::new(AuditEvent::StatusRequest)
                .with_remote(remote)).await;
            handle_status(&mut send, root).await?;
        }
        Request::Get { path } => {
            let _ = audit_tx.send(AuditEntry::new(AuditEvent::FileRequest)
                .with_remote(remote)
                .with_path(&path)).await;
            handle_get(&mut send, root, &path).await?;
        }
    }
    
    Ok(())
}

async fn handle_put(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    root: &Path,
    path: &str,
    size: u64,
    _hash: Option<&str>,
) -> Result<()> {
    // Sanitize path (no ..)
    let clean_path = path.trim_start_matches('/').replace("..", "");
    let dest = root.join(&clean_path);
    
    // Create parent directories
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }
    
    tracing::info!("📝 Receiving: {} ({} bytes)", clean_path, size);
    
    // Send OK to start transfer
    send_response(send, Response::Ok).await?;
    
    // Receive file data
    let mut file = fs::File::create(&dest).await?;
    let mut received = 0u64;
    let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks
    
    while received < size {
        let to_read = std::cmp::min(buf.len() as u64, size - received) as usize;
        let n = recv.read(&mut buf[..to_read]).await?.unwrap_or(0);
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).await?;
        received += n as u64;
    }
    
    file.flush().await?;
    drop(file);
    
    tracing::info!("✅ Written: {} ({} bytes)", clean_path, received);
    
    // Send completion
    send_response(send, Response::Done { written: received }).await?;
    
    Ok(())
}

async fn handle_list(
    send: &mut quinn::SendStream,
    root: &Path,
    path: &str,
    recursive: bool,
    long: bool,
) -> Result<()> {
    let clean_path = path.trim_start_matches('/').replace("..", "");
    let dir = if clean_path.is_empty() {
        root.to_path_buf()
    } else {
        root.join(&clean_path)
    };
    
    let mut entries = Vec::new();
    
    if dir.is_dir() {
        if recursive {
            for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
                if entry.path() == dir { continue; }
                let metadata = entry.metadata().ok();
                let rel = entry.path().strip_prefix(&dir).unwrap_or(entry.path());
                let name = rel.to_string_lossy().replace('\\', "/");
                let (is_dir, size, modified) = match metadata {
                    Some(m) => {
                        let modified = if long {
                            m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
                        } else { None };
                        (m.is_dir(), m.len(), modified)
                    }
                    None => (entry.file_type().is_dir(), 0, None),
                };
                entries.push(crate::protocol::FileEntry { name, is_dir, size, modified });
            }
        } else {
            let mut read_dir = fs::read_dir(&dir).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let meta = entry.metadata().await?;
                let name = entry.file_name().to_string_lossy().to_string();
                let modified = if long {
                    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
                } else { None };
                entries.push(crate::protocol::FileEntry {
                    name,
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                    modified,
                });
            }
        }
    }
    
    send_response(send, Response::List { entries }).await?;
    
    Ok(())
}

async fn handle_status(
    send: &mut quinn::SendStream,
    root: &Path,
) -> Result<()> {
    // Calculate disk usage
    let mut total_size = 0u64;
    let mut file_count = 0u64;
    
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            file_count += 1;
        }
    }
    
    send_response(send, Response::Status {
        root: root.to_string_lossy().to_string(),
        total_size,
        file_count,
    }).await?;
    
    Ok(())
}

async fn handle_get(
    send: &mut quinn::SendStream,
    root: &Path,
    path: &str,
) -> Result<()> {
    let clean_path = path.trim_start_matches('/').replace("..", "");
    let file_path = root.join(&clean_path);

    let metadata = fs::metadata(&file_path).await?;
    if !metadata.is_file() {
        send_response(send, Response::Error { message: "Not a file".into() }).await?;
        return Ok(());
    }

    let size = metadata.len();
    send_response(send, Response::File { size }).await?;

    let mut file = fs::File::open(&file_path).await?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut sent = 0u64;
    while sent < size {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        send.write_all(&buf[..n]).await?;
        sent += n as u64;
    }

    Ok(())
}

async fn send_response(send: &mut quinn::SendStream, response: Response) -> Result<()> {
    let json = serde_json::to_vec(&response)?;
    let len = (json.len() as u32).to_be_bytes();
    send.write_all(&len).await?;
    send.write_all(&json).await?;
    Ok(())
}
