use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod server;
mod client;
mod protocol;
mod tls;
mod config;
mod audit;

#[derive(Parser)]
#[command(name = "hank-sync")]
#[command(about = "Minimal QUIC-based file sync", long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start server to receive files
    Server {
        /// Root directory for received files
        #[arg(short, long)]
        root: PathBuf,
        
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:4433")]
        bind: String,
        
        /// Audit log file path
        #[arg(short, long)]
        audit_log: Option<PathBuf>,
    },
    
    /// Send file(s) to server
    Send {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
        
        /// File or directory to send
        path: PathBuf,
        
        /// Destination path on server (relative to root)
        #[arg(short, long)]
        dest: Option<String>,
    },
    
    /// List files on server
    List {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
        
        /// Path to list
        #[arg(default_value = "/")]
        path: String,
    },
    
    /// Get server status
    Status {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
    },
    
    /// Generate default config
    Init {
        /// Config directory
        #[arg(short, long)]
        config_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Setup logging
    tracing_subscriber::fmt()
        .with_max_level(if cli.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO })
        .init();
    
    match cli.command {
        Commands::Server { root, bind, audit_log } => {
            tracing::info!("Starting server on {}", bind);
            tracing::info!("Root directory: {:?}", root);
            let log_path = audit_log.unwrap_or_else(|| root.join("audit.jsonl"));
            tracing::info!("Audit log: {:?}", log_path);
            server::run(&bind, &root, &log_path).await?;
        }
        Commands::Send { server, path, dest } => {
            let server = config::resolve_server(server)?;
            tracing::info!("Sending {:?} to {}", path, server);
            client::send(&server, &path, dest.as_deref()).await?;
        }
        Commands::List { server, path } => {
            let server = config::resolve_server(server)?;
            tracing::info!("Listing {} on {}", path, server);
            client::list(&server, &path).await?;
        }
        Commands::Status { server } => {
            let server = config::resolve_server(server)?;
            client::status(&server).await?;
        }
        Commands::Init { config_dir } => {
            config::init(config_dir.as_deref())?;
        }
    }
    
    Ok(())
}
