//! Hindsight TUI - terminal interface for viewing distributed traces.

use clap::Parser;

#[derive(Parser)]
#[command(name = "hindsight-tui")]
#[command(about = "Terminal UI for Hindsight distributed tracing", long_about = None)]
struct Cli {
    /// Hindsight server address
    #[arg(short, long, default_value = "http://localhost:9090")]
    connect: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    println!("🔍 Hindsight TUI");
    println!("   Connecting to: {}", cli.connect);

    // TODO: Start TUI
    println!("\n⚠️  TUI implementation coming soon!");
    Ok(())
}
