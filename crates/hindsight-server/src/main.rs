//! Hindsight server - collects and visualizes distributed traces.

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
        /// Port to listen on
        #[arg(short, long, default_value = "9090")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, host } => {
            println!("🔍 Hindsight server starting on http://{}:{}", host, port);
            println!("   Web UI: http://{}:{}", host, port);
            println!("   API: http://{}:{}/v1/traces", host, port);

            // TODO: Start server
            println!("\n⚠️  Server implementation coming soon!");
            Ok(())
        }
    }
}
