mod seed_data;
mod service_impl;
mod storage;

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use hindsight_protocol::*;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use rapace::RpcSession;
use std::sync::Arc;
use std::time::Duration;
use tower::Service;

use crate::service_impl::HindsightServiceImpl;
use crate::storage::TraceStore;

pub async fn run_server(
    host: impl Into<String>,
    http_port: u16,
    tcp_port: u16,
    ttl_secs: u64,
    seed: bool,
) -> anyhow::Result<()> {
    let host = host.into();
    tracing::info!("🔍 Hindsight server starting");

    let store = TraceStore::new(Duration::from_secs(ttl_secs));

    // Load seed data if requested
    if seed {
        tracing::info!("Loading seed data for UI development...");
        seed_data::load_seed_data(&store);
        tracing::info!("Seed data loaded successfully");
    }

    let service = Arc::new(HindsightServiceImpl::new(store));

    // Spawn raw TCP server on port 1991 (for clients that want to skip HTTP handshake)
    let service_tcp = service.clone();
    let host_tcp = host.clone();
    tokio::spawn(async move {
        if let Err(e) = serve_tcp(&host_tcp, tcp_port, service_tcp).await {
            tracing::error!("TCP server error: {}", e);
        }
    });

    // Serve unified HTTP server on port 1990
    // Handles: HTTP GET, WebSocket upgrade, Rapace upgrade
    serve_http_unified(&host, http_port, service).await?;

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
            let transport = rapace::Transport::stream(stream);

            // IMPORTANT: No tracer attached! (Prevents infinite loop)
            let session = Arc::new(RpcSession::new(transport));

            // Create dispatcher function
            session.set_dispatcher(move |frame| {
                let service_impl = service.as_ref().clone();
                Box::pin(async move {
                    let server = HindsightServiceServer::new(service_impl);
                    server
                        .dispatch(frame.desc.method_id, frame.payload_bytes())
                        .await
                })
            });

            if let Err(e) = session.run().await {
                tracing::error!("TCP session error: {}", e);
            }
        });
    }
}

/// Unified HTTP server on port 1990
/// Handles: HTTP GET /, WebSocket upgrade, Rapace upgrade
async fn serve_http_unified(
    host: &str,
    port: u16,
    service: Arc<HindsightServiceImpl>,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route(
            "/",
            get({
                let service = service.clone();
                move |headers: HeaderMap, req: Request| handle_root(headers, req, service.clone())
            }),
        )
        .route("/pkg/*file", get(serve_pkg_file))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .with_state(service.clone());

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🌐 Unified server listening on {}", addr);
    tracing::info!("  - HTTP GET / → Web UI");
    tracing::info!("  - WebSocket upgrade → WebSocket Rapace (for WASM clients)");
    tracing::info!("  - HTTP Upgrade: rapace → Rapace over HTTP upgrade");
    tracing::info!("  - Raw binary → Direct Rapace TCP (for native clients)");

    // Handle connections manually to intercept WebSocket at TCP level
    loop {
        let (tcp_stream, peer_addr) = listener.accept().await?;
        let service = service.clone();
        let app = app.clone();

        tokio::spawn(async move {
            // Peek at the first bytes to detect connection type
            let mut peek_buf = [0u8; 1024];
            match tcp_stream.peek(&mut peek_buf).await {
                Ok(n) if n > 0 => {
                    let peek_str = String::from_utf8_lossy(&peek_buf[..n]);

                    if peek_str.contains("Upgrade: websocket") {
                        tracing::info!(
                            "Detected WebSocket upgrade from {}, handling with tokio-tungstenite",
                            peer_addr
                        );
                        handle_websocket_tcp(tcp_stream, service).await;
                    } else if peek_str.starts_with("GET ")
                        || peek_str.starts_with("POST ")
                        || peek_str.starts_with("PUT ")
                        || peek_str.starts_with("DELETE ")
                        || peek_str.starts_with("HEAD ")
                        || peek_str.starts_with("OPTIONS ")
                    {
                        // HTTP request - handle with axum
                        tracing::info!("Detected HTTP request from {}", peer_addr);
                        let tower_service = app.into_service();
                        let hyper_service = hyper::service::service_fn(
                            move |request: hyper::Request<hyper::body::Incoming>| {
                                tower_service.clone().call(request)
                            },
                        );

                        if let Err(e) = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(tcp_stream), hyper_service)
                            .await
                        {
                            tracing::error!("HTTP connection error: {}", e);
                        }
                    } else {
                        // Raw binary Rapace protocol (no HTTP)
                        tracing::info!("Detected raw Rapace binary connection from {}", peer_addr);
                        handle_rapace_tcp(tcp_stream, service).await;
                    }
                }
                _ => {
                    tracing::warn!("Failed to peek TCP stream from {}", peer_addr);
                }
            }
        });
    }
}

/// Handle requests to "/" - detect upgrade type or serve HTML
async fn handle_root(
    headers: HeaderMap,
    req: Request,
    service: Arc<HindsightServiceImpl>,
) -> Response {
    // Check for Upgrade header
    let upgrade = headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    match upgrade.as_deref() {
        Some("websocket") => {
            // WebSocket is now intercepted at TCP level before reaching here
            // This should never happen, but return error just in case
            tracing::error!(
                "WebSocket upgrade reached handle_root - should be intercepted at TCP level"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Some("rapace") => {
            // Rapace upgrade - manual handling
            handle_rapace_upgrade(req, service).await.into_response()
        }
        _ => {
            // Normal HTTP - serve trace viewer UI
            let html = include_str!("ui/app.html");
            Html(html).into_response()
        }
    }
}

/// Handle raw binary Rapace TCP connection (no HTTP)
async fn handle_rapace_tcp(tcp_stream: tokio::net::TcpStream, service: Arc<HindsightServiceImpl>) {
    tracing::info!("Handling raw Rapace binary connection");

    let transport = rapace::Transport::stream(tcp_stream);
    let session = Arc::new(RpcSession::new(transport));

    session.set_dispatcher(move |frame| {
        let service_impl = service.as_ref().clone();
        Box::pin(async move {
            let server = HindsightServiceServer::new(service_impl);
            server
                .dispatch(frame.desc.method_id, frame.payload_bytes())
                .await
        })
    });

    if let Err(e) = session.run().await {
        tracing::error!("Raw Rapace session error: {}", e);
    }

    tracing::info!("Raw Rapace connection closed");
}

/// Handle WebSocket at TCP level using tokio-tungstenite
async fn handle_websocket_tcp(
    tcp_stream: tokio::net::TcpStream,
    service: Arc<HindsightServiceImpl>,
) {
    tracing::info!("Accepting WebSocket connection with tokio-tungstenite");

    // Let tokio-tungstenite handle the entire WebSocket handshake (including HTTP headers)
    match tokio_tungstenite::accept_async(tcp_stream).await {
        Ok(ws_stream) => {
            tracing::info!("WebSocket handshake complete, starting Rapace session");

            // Create WebSocket transport using rapace's Transport enum
            let transport = rapace::Transport::websocket(ws_stream);
            let server = HindsightServiceServer::new(service.as_ref().clone());

            if let Err(e) = server.serve(transport).await {
                tracing::error!("WebSocket Rapace session error: {:?}", e);
            }

            tracing::info!("WebSocket Rapace connection closed");
        }
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {:?}", e);
        }
    }
}

/// Handle Rapace HTTP upgrade (for native clients)
async fn handle_rapace_upgrade(mut req: Request, service: Arc<HindsightServiceImpl>) -> Response {
    // Extract the upgrade future from the request
    let upgrade = hyper::upgrade::on(&mut req);

    // Spawn task to handle the upgraded connection
    tokio::spawn(async move {
        match upgrade.await {
            Ok(upgraded) => {
                tracing::info!("Rapace HTTP upgrade successful");
                handle_rapace_connection(upgraded, service).await;
            }
            Err(e) => {
                tracing::error!("Rapace upgrade failed: {}", e);
            }
        }
    });

    // Return 101 Switching Protocols response
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header("Upgrade", "rapace")
        .header("Connection", "Upgrade")
        .body(axum::body::Body::empty())
        .unwrap()
        .into_response()
}

/// Handle upgraded Rapace connection
async fn handle_rapace_connection(upgraded: Upgraded, service: Arc<HindsightServiceImpl>) {
    tracing::info!("Handling Rapace connection over HTTP upgrade");

    // Upgraded is not Sync, but Rapace requires Sync for Transport.
    // Solution: Use DuplexStream as a bridge (it's Sync)
    let (mut client_stream, server_stream) = tokio::io::duplex(8192);

    // Spawn a task to bridge the Upgraded connection to DuplexStream
    tokio::spawn(async move {
        let mut upgraded = TokioIo::new(upgraded);
        if let Err(e) = tokio::io::copy_bidirectional(&mut upgraded, &mut client_stream).await {
            tracing::error!("HTTP upgrade bridge error: {}", e);
        }
    });

    // Use the Sync-safe DuplexStream with Transport
    let transport = rapace::Transport::stream(server_stream);

    // IMPORTANT: No tracer attached! (Prevents infinite loop)
    let session = Arc::new(RpcSession::new(transport));

    // Create dispatcher function
    session.set_dispatcher(move |frame| {
        let service_impl = service.as_ref().clone();
        Box::pin(async move {
            let server = HindsightServiceServer::new(service_impl);
            server
                .dispatch(frame.desc.method_id, frame.payload_bytes())
                .await
        })
    });

    if let Err(e) = session.run().await {
        tracing::error!("Rapace HTTP upgrade session error: {}", e);
    }
}

/// Serve WASM package files
async fn serve_pkg_file(
    axum::extract::Path(file): axum::extract::Path<String>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    // Map file extensions to content types
    let content_type = if file.ends_with(".wasm") {
        "application/wasm"
    } else if file.ends_with(".js") {
        "application/javascript"
    } else if file.ends_with(".json") {
        "application/json"
    } else {
        "text/plain"
    };

    // Read file from pkg directory
    // Try multiple possible paths (depends on where cargo run is executed from)
    let possible_paths = [
        format!("pkg/{}", file),          // From workspace root
        format!("../../pkg/{}", file),    // From target/debug
        format!("../../../pkg/{}", file), // From target/debug/deps
    ];

    for pkg_path in &possible_paths {
        if let Ok(bytes) = std::fs::read(pkg_path) {
            return ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}
