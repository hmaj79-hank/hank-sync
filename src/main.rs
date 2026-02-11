use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod server;
mod client;
mod protocol;
mod tls;
mod config;
mod audit;
mod state;

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
    
    /// Put (upload) file(s) to server
    Put {
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
        
        /// Path to list (defaults to current cwd)
        path: Option<String>,
    },

    /// Long list (ls -al)
    Listl {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
        
        /// Path to list (defaults to current cwd)
        path: Option<String>,
    },

    /// Recursive list (ls -R)
    Listr {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
        
        /// Path to list (defaults to current cwd)
        path: Option<String>,
    },

    /// Go up one directory (and list)
    Up {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,
    },

    /// Go down: back to previous dir, or into <dir> (and list)
    Down {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,

        /// Directory to enter
        dir: Option<String>,
    },

    /// View (dump) a file from server
    View {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,

        /// File path to view
        path: String,
    },
    
    /// Get (download) a file from server
    Get {
        /// Server address (overrides config)
        #[arg(short, long)]
        server: Option<String>,

        /// File path on server
        path: String,

        /// Destination path on client (file or directory)
        #[arg(short, long)]
        dest: Option<PathBuf>,
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
        Commands::Put { server, path, dest } => {
            let server = config::resolve_server(server)?;
            tracing::info!("Putting {:?} to {}", path, server);
            client::put(&server, &path, dest.as_deref()).await?;
        }
        Commands::List { server, path } => {
            let server = config::resolve_server(server)?;
            let mut state = state::load().unwrap_or_default();
            let list_path = match path {
                Some(p) => state::normalize(&p),
                None => state::normalize(&state.cwd),
            };
            state.prev = state.cwd.clone();
            state.cwd = list_path.clone();
            let _ = state::save(&state);
            tracing::info!("Listing {} on {}", list_path, server);
            client::list(&server, &list_path).await?;
        }
        Commands::Listl { server, path } => {
            let server = config::resolve_server(server)?;
            let mut state = state::load().unwrap_or_default();
            let list_path = match path {
                Some(p) => state::normalize(&p),
                None => state::normalize(&state.cwd),
            };
            state.prev = state.cwd.clone();
            state.cwd = list_path.clone();
            let _ = state::save(&state);
            tracing::info!("Listing (long) {} on {}", list_path, server);
            client::list_long(&server, &list_path).await?;
        }
        Commands::Listr { server, path } => {
            let server = config::resolve_server(server)?;
            let mut state = state::load().unwrap_or_default();
            let list_path = match path {
                Some(p) => state::normalize(&p),
                None => state::normalize(&state.cwd),
            };
            state.prev = state.cwd.clone();
            state.cwd = list_path.clone();
            let _ = state::save(&state);
            tracing::info!("Listing (recursive) {} on {}", list_path, server);
            client::list_recursive(&server, &list_path).await?;
        }
        Commands::Up { server } => {
            let server = config::resolve_server(server)?;
            let mut state = state::load().unwrap_or_default();
            let parent = std::path::Path::new(&state.cwd)
                .parent()
                .unwrap_or(std::path::Path::new("/"))
                .to_string_lossy()
                .to_string();
            state.prev = state.cwd.clone();
            state.cwd = state::normalize(&parent);
            let _ = state::save(&state);
            client::list(&server, &state.cwd).await?;
        }
        Commands::Down { server, dir } => {
            let server = config::resolve_server(server)?;
            let mut state = state::load().unwrap_or_default();
            if let Some(d) = dir {
                let next = state::join(&state.cwd, &d);
                state.prev = state.cwd.clone();
                state.cwd = next;
            } else {
                std::mem::swap(&mut state.cwd, &mut state.prev);
                state.cwd = state::normalize(&state.cwd);
            }
            let _ = state::save(&state);
            client::list(&server, &state.cwd).await?;
        }
        Commands::Status { server } => {
            let server = config::resolve_server(server)?;
            client::status(&server).await?;
        }
        Commands::View { server, path } => {
            let server = config::resolve_server(server)?;
            client::view(&server, &path).await?;
        }
        Commands::Get { server, path, dest } => {
            let server = config::resolve_server(server)?;
            client::get(&server, &path, dest.as_deref()).await?;
        }
        Commands::Init { config_dir } => {
            config::init(config_dir.as_deref())?;
        }
    }
    
    Ok(())
}
