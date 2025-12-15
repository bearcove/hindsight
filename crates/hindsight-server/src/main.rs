mod storage;
mod service_impl;

use axum::{response::Html, routing::get, Router};
use clap::{Parser, Subcommand};
use hindsight_protocol::*;
use rapace::RpcSession;
use std::sync::Arc;
use std::time::Duration;

use crate::service_impl::HindsightServiceImpl;
use crate::storage::TraceStore;

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
        /// Port for Rapace TCP transport (for native clients)
        #[arg(short, long, default_value = "9090")]
        port: u16,

        /// Port for WebSocket transport (for browser clients)
        #[arg(short, long, default_value = "9091")]
        ws_port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// TTL for traces in seconds
        #[arg(long, default_value = "3600")]
        ttl: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, ws_port, host, ttl } => {
            serve(host, port, ws_port, ttl).await
        }
    }
}

async fn serve(host: String, port: u16, ws_port: u16, ttl_secs: u64) -> anyhow::Result<()> {
    tracing::info!("🔍 Hindsight server starting");

    let store = TraceStore::new(Duration::from_secs(ttl_secs));
    let service = Arc::new(HindsightServiceImpl::new(store));

    // Spawn TCP server (for native clients: TUI, apps, Rapace cells)
    let service_tcp = service.clone();
    let host_tcp = host.clone();
    tokio::spawn(async move {
        if let Err(e) = serve_tcp(&host_tcp, port, service_tcp).await {
            tracing::error!("TCP server error: {}", e);
        }
    });

    // Spawn WebSocket server (for browser WASM clients)
    let service_ws = service.clone();
    let host_ws = host.clone();
    tokio::spawn(async move {
        if let Err(e) = serve_websocket(&host_ws, ws_port, service_ws).await {
            tracing::error!("WebSocket server error: {}", e);
        }
    });

    // Serve minimal HTTP server (only for serving the WASM UI)
    serve_http(&host, ws_port).await?;

    Ok(())
}

/// Serve Rapace RPC over TCP (for native clients)
async fn serve_tcp(
    host: &str,
    port: u16,
    service: Arc<HindsightServiceImpl>,
) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("📡 Rapace TCP server listening on {}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        tracing::info!("New TCP connection from {}", peer_addr);

        let service = service.clone();
        tokio::spawn(async move {
            let transport = Arc::new(rapace::transport::StreamTransport::new(stream));

            // IMPORTANT: No tracer attached! (Prevents infinite loop)
            let session = Arc::new(RpcSession::new(transport));

            // Create dispatcher function
            session.set_dispatcher(move |_channel_id, method_id, payload| {
                let service_impl = service.as_ref().clone();
                Box::pin(async move {
                    let server = HindsightServiceServer::new(service_impl);
                    server.dispatch(method_id, &payload).await
                })
            });

            if let Err(e) = session.run().await {
                tracing::error!("TCP session error: {}", e);
            }
        });
    }
}

/// Serve Rapace RPC over WebSocket (for browser WASM clients)
async fn serve_websocket(
    host: &str,
    port: u16,
    service: Arc<HindsightServiceImpl>,
) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🌐 WebSocket server listening on ws://{}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;

        let service = service.clone();
        tokio::spawn(async move {
            // Accept WebSocket upgrade
            let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    tracing::error!("WebSocket upgrade failed: {}", e);
                    return;
                }
            };

            tracing::info!("New WebSocket connection from {}", peer_addr);

            let transport = Arc::new(rapace::transport::WebSocketTransport::new(ws_stream));

            // IMPORTANT: No tracer attached! (Prevents infinite loop)
            let session = Arc::new(RpcSession::new(transport));

            // Create dispatcher function
            session.set_dispatcher(move |_channel_id, method_id, payload| {
                let service_impl = service.as_ref().clone();
                Box::pin(async move {
                    let server = HindsightServiceServer::new(service_impl);
                    server.dispatch(method_id, &payload).await
                })
            });

            if let Err(e) = session.run().await {
                tracing::error!("WebSocket session error: {}", e);
            }
        });
    }
}

/// Serve static HTML (only for loading the WASM UI)
async fn serve_http(host: &str, ws_port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(move || async move {
            let html = include_str!("ui/index.html")
                .replace("{{WS_PORT}}", &ws_port.to_string());
            Html(html)
        }));

    let addr = format!("{}:8080", host);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🌍 Web UI available at http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
