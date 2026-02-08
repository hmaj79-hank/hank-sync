//! QUIC client - sends files

use anyhow::Result;
use quinn::Endpoint;
use std::{io::Write, path::Path};
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::protocol::{Request, Response};
use crate::tls;

async fn connect(server: &str) -> Result<quinn::Connection> {
    let client_config = tls::client_config()?;
    
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(client_config);
    
    // Parse server address
    let addr = server.parse()?;
    
    // Connect (use "localhost" as server name for self-signed certs)
    let connection = endpoint.connect(addr, "localhost")?.await?;
    tracing::info!("🔗 Connected to {}", server);
    
    Ok(connection)
}

pub async fn send(server: &str, path: &Path, dest: Option<&str>) -> Result<()> {
    let connection = connect(server).await?;
    
    if path.is_file() {
        send_file(&connection, path, dest).await?;
    } else if path.is_dir() {
        send_dir(&connection, path, dest).await?;
    } else {
        anyhow::bail!("Path does not exist: {:?}", path);
    }
    
    connection.close(0u32.into(), b"done");
    Ok(())
}

async fn send_file(connection: &quinn::Connection, path: &Path, dest: Option<&str>) -> Result<()> {
    let filename = path.file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?
        .to_string_lossy();
    
    let remote_path = match dest {
        Some(d) => format!("{}/{}", d.trim_end_matches('/'), filename),
        None => filename.to_string(),
    };
    
    let metadata = fs::metadata(path).await?;
    let size = metadata.len();
    
    // Compute hash
    let mut file = fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize().to_hex().to_string();
    drop(file);
    
    tracing::info!("📤 Sending: {} → {} ({} bytes)", path.display(), remote_path, size);
    
    // Open stream
    let (mut send, mut recv) = connection.open_bi().await?;
    
    // Send request
    let request = Request::Put {
        path: remote_path,
        size,
        hash: Some(hash),
    };
    send_request(&mut send, &request).await?;
    
    // Wait for OK
    let response = recv_response(&mut recv).await?;
    if !matches!(response, Response::Ok) {
        anyhow::bail!("Server rejected: {:?}", response);
    }
    
    // Send file data
    let mut file = fs::File::open(path).await?;
    let mut sent = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    
    while sent < size {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        send.write_all(&buf[..n]).await?;
        sent += n as u64;
        
        // Progress
        let pct = (sent as f64 / size as f64 * 100.0) as u8;
        if sent % (1024 * 1024) == 0 || sent == size {
            tracing::debug!("Progress: {}%", pct);
        }
    }
    
    send.finish()?;
    
    // Wait for completion
    let response = recv_response(&mut recv).await?;
    match response {
        Response::Done { written } => {
            tracing::info!("✅ Done: {} bytes written", written);
        }
        _ => {
            tracing::warn!("Unexpected response: {:?}", response);
        }
    }
    
    Ok(())
}

async fn send_dir(connection: &quinn::Connection, path: &Path, dest: Option<&str>) -> Result<()> {
    let base = path.file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid directory name"))?
        .to_string_lossy();
    
    let base_dest = match dest {
        Some(d) => format!("{}/{}", d.trim_end_matches('/'), base),
        None => base.to_string(),
    };
    
    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(path)?;
            let remote = format!("{}/{}", base_dest, rel.to_string_lossy().replace('\\', "/"));
            
            send_file_with_path(connection, entry.path(), &remote).await?;
        }
    }
    
    Ok(())
}

async fn send_file_with_path(connection: &quinn::Connection, path: &Path, remote_path: &str) -> Result<()> {
    let metadata = fs::metadata(path).await?;
    let size = metadata.len();
    
    tracing::info!("📤 Sending: {} → {} ({} bytes)", path.display(), remote_path, size);
    
    // Open stream
    let (mut send, mut recv) = connection.open_bi().await?;
    
    // Send request
    let request = Request::Put {
        path: remote_path.to_string(),
        size,
        hash: None, // Skip hash for directories (faster)
    };
    send_request(&mut send, &request).await?;
    
    // Wait for OK
    let response = recv_response(&mut recv).await?;
    if !matches!(response, Response::Ok) {
        anyhow::bail!("Server rejected: {:?}", response);
    }
    
    // Send file data
    let mut file = fs::File::open(path).await?;
    let mut buf = vec![0u8; 64 * 1024];
    
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        send.write_all(&buf[..n]).await?;
    }
    
    send.finish()?;
    
    // Wait for completion
    let _ = recv_response(&mut recv).await?;
    
    Ok(())
}

pub async fn list(server: &str, path: &str) -> Result<()> {
    let connection = connect(server).await?;
    
    let (mut send, mut recv) = connection.open_bi().await?;
    
    send_request(&mut send, &Request::List { path: path.to_string() }).await?;
    
    let response = recv_response(&mut recv).await?;
    
    match response {
        Response::List { entries } => {
            println!("📁 Contents of {}:", path);
            for entry in entries {
                let type_indicator = if entry.is_dir { "📁" } else { "📄" };
                let size = if entry.is_dir {
                    String::new()
                } else {
                    format!(" ({} bytes)", entry.size)
                };
                println!("  {} {}{}", type_indicator, entry.name, size);
            }
        }
        _ => {
            tracing::error!("Unexpected response: {:?}", response);
        }
    }
    
    connection.close(0u32.into(), b"done");
    Ok(())
}

pub async fn status(server: &str) -> Result<()> {
    let connection = connect(server).await?;
    
    let (mut send, mut recv) = connection.open_bi().await?;
    
    send_request(&mut send, &Request::Status).await?;
    
    let response = recv_response(&mut recv).await?;
    
    match response {
        Response::Status { root, total_size, file_count } => {
            println!("📊 Server Status:");
            println!("  Root: {}", root);
            println!("  Files: {}", file_count);
            println!("  Total size: {} MB", total_size / 1024 / 1024);
        }
        _ => {
            tracing::error!("Unexpected response: {:?}", response);
        }
    }
    
    connection.close(0u32.into(), b"done");
    Ok(())
}

pub async fn view(server: &str, path: &str) -> Result<()> {
    let connection = connect(server).await?;

    let (mut send, mut recv) = connection.open_bi().await?;
    send_request(&mut send, &Request::Get { path: path.to_string() }).await?;

    let response = recv_response(&mut recv).await?;
    match response {
        Response::File { size } => {
            let mut remaining = size as usize;
            let mut buf = vec![0u8; 64 * 1024];
            let mut out = std::io::stdout();
            while remaining > 0 {
                let to_read = std::cmp::min(remaining, buf.len());
                let n = match recv.read(&mut buf[..to_read]).await? {
                    Some(n) => n,
                    None => break,
                };
                if n == 0 { break; }
                out.write_all(&buf[..n])?;
                remaining -= n;
            }
        }
        Response::Error { message } => {
            anyhow::bail!("Server error: {}", message);
        }
        other => {
            anyhow::bail!("Unexpected response: {:?}", other);
        }
    }

    connection.close(0u32.into(), b"done");
    Ok(())
}

async fn send_request(send: &mut quinn::SendStream, request: &Request) -> Result<()> {
    let json = serde_json::to_vec(request)?;
    let len = (json.len() as u32).to_be_bytes();
    send.write_all(&len).await?;
    send.write_all(&json).await?;
    Ok(())
}

async fn recv_response(recv: &mut quinn::RecvStream) -> Result<Response> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    
    Ok(serde_json::from_slice(&buf)?)
}
