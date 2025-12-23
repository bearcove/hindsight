use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hindsight")]
#[command(about = "Distributed tracing made simple", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the trace collection server
    Serve {
        /// Port for HTTP + WebSocket (web UI and browser clients)
        #[arg(short = 'w', long, default_value = "1990")]
        http_port: u16,

        /// Port for Rapace TCP transport (native clients)
        #[arg(short = 't', long, default_value = "1991")]
        tcp_port: u16,

        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// TTL for traces in seconds
        #[arg(long, default_value = "3600")]
        ttl: u64,

        /// Load seed data on startup for UI development
        #[arg(long)]
        seed: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with INFO level by default if RUST_LOG is not set
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            http_port,
            tcp_port,
            host,
            ttl,
            seed,
        } => {
            eprintln!("🔍 Hindsight server starting...");
            eprintln!("   HTTP/WebSocket: http://{}:{}", if host == "0.0.0.0" { "localhost" } else { &host }, http_port);
            eprintln!("   Rapace TCP: {}:{}", host, tcp_port);
            if seed {
                eprintln!("   Seed data: enabled");
            }
            hindsight_server::run_server(host, http_port, tcp_port, ttl, seed).await
        }
    }
}
