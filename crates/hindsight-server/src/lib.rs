mod storage;
mod service_impl;

use axum::{
    extract::{Path, Request, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response, Json},
    routing::{get, post},
    Router,
};
use hindsight_protocol::*;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use rapace::RpcSession;
use std::sync::Arc;
use std::time::Duration;

use crate::service_impl::HindsightServiceImpl;
use crate::storage::TraceStore;

pub async fn run_server(host: impl Into<String>, http_port: u16, tcp_port: u16, ttl_secs: u64) -> anyhow::Result<()> {
    let host = host.into();
    tracing::info!("🔍 Hindsight server starting");

    let store = TraceStore::new(Duration::from_secs(ttl_secs));
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

/// Unified HTTP server on port 1990
/// Handles: HTTP GET /, WebSocket upgrade, Rapace upgrade
async fn serve_http_unified(
    host: &str,
    port: u16,
    service: Arc<HindsightServiceImpl>,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get({
            let service = service.clone();
            move |headers: HeaderMap, ws: Option<WebSocketUpgrade>, req: Request| {
                handle_root(headers, ws, req, service.clone())
            }
        }))
        .route("/api/traces", get(list_traces_handler))
        .route("/api/traces/:trace_id", get(get_trace_handler))
        .route("/app.js", get(serve_js))
        .with_state(service);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🌐 Unified server listening on http://{}", addr);
    tracing::info!("  - HTTP GET / → Web UI");
    tracing::info!("  - Upgrade: websocket → WebSocket (for WASM clients)");
    tracing::info!("  - Upgrade: rapace → Raw Rapace TCP (for native clients)");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Handle requests to "/" - detect upgrade type or serve HTML
async fn handle_root(
    headers: HeaderMap,
    ws: Option<WebSocketUpgrade>,
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
            // WebSocket upgrade
            if let Some(ws) = ws {
                ws.on_upgrade(move |socket| handle_websocket(socket, service))
                    .into_response()
            } else {
                (StatusCode::BAD_REQUEST, "WebSocket upgrade failed").into_response()
            }
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

/// Handle WebSocket upgrade (for WASM clients)
async fn handle_websocket(
    _socket: axum::extract::ws::WebSocket,
    _service: Arc<HindsightServiceImpl>,
) {
    tracing::info!("New WebSocket connection");

    // TODO: Actually create the Rapace WebSocket transport here
    // For now, this is a placeholder - we'll need to properly bridge
    // the Axum WebSocket to rapace-transport-websocket
    tracing::warn!("WebSocket Rapace transport not yet implemented in unified server");
}

/// Handle Rapace HTTP upgrade (for native clients)
async fn handle_rapace_upgrade(
    mut req: Request,
    service: Arc<HindsightServiceImpl>,
) -> Response {
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

    // Use the Sync-safe DuplexStream with StreamTransport
    let transport = Arc::new(rapace::transport::StreamTransport::new(server_stream));

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
        tracing::error!("Rapace HTTP upgrade session error: {}", e);
    }
}

/// REST API handler: List traces
async fn list_traces_handler(
    State(service): State<Arc<HindsightServiceImpl>>,
) -> Json<serde_json::Value> {
    let filter = TraceFilter {
        service: None,
        min_duration_nanos: None,
        max_duration_nanos: None,
        has_errors: None,
        limit: Some(100),
    };

    let summaries = service.list_traces(filter).await;

    // Convert to JSON
    let traces: Vec<serde_json::Value> = summaries.iter().map(|s| {
        serde_json::json!({
            "trace_id": s.trace_id.to_hex(),
            "root_span_name": s.root_span_name,
            "service_name": s.service_name,
            "start_time_nanos": s.start_time.0,
            "duration_nanos": s.duration_nanos,
            "span_count": s.span_count,
            "error_count": if s.has_errors { 1 } else { 0 },
            "trace_type": format!("{:?}", s.trace_type),
        })
    }).collect();

    Json(serde_json::json!({
        "traces": traces,
        "total": summaries.len(),
    }))
}

/// REST API handler: Get trace by ID
async fn get_trace_handler(
    State(service): State<Arc<HindsightServiceImpl>>,
    Path(trace_id): Path<String>,
) -> Json<serde_json::Value> {
    // Parse trace ID from hex string
    let trace_id = match TraceId::from_hex(&trace_id) {
        Ok(id) => id,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid trace ID format",
            }));
        }
    };

    match service.get_trace(trace_id).await {
        Some(trace) => {
            // Convert trace to JSON
            let spans: Vec<serde_json::Value> = trace.spans.iter().map(|s| {
                let attributes: Vec<serde_json::Value> = s.attributes.iter().map(|(k, v)| {
                    let value_json = match v {
                        AttributeValue::String(s) => serde_json::Value::String(s.clone()),
                        AttributeValue::Int(i) => serde_json::json!(i),
                        AttributeValue::Float(f) => serde_json::json!(f),
                        AttributeValue::Bool(b) => serde_json::Value::Bool(*b),
                    };
                    serde_json::json!({
                        "key": k,
                        "value": value_json,
                    })
                }).collect();

                serde_json::json!({
                    "trace_id": s.trace_id.to_hex(),
                    "span_id": s.span_id.to_hex(),
                    "parent_span_id": s.parent_span_id.map(|id| id.to_hex()),
                    "name": s.name,
                    "service_name": s.service_name,
                    "start_time_nanos": s.start_time.0,
                    "end_time_nanos": s.end_time.map(|t| t.0),
                    "duration_nanos": s.duration_nanos(),
                    "attributes": attributes,
                })
            }).collect();

            Json(serde_json::json!({
                "trace": {
                    "trace_id": trace.trace_id.to_hex(),
                    "root_span_id": trace.root_span_id.to_hex(),
                    "trace_type": format!("{:?}", trace.classify_type()),
                    "spans": spans,
                }
            }))
        }
        None => {
            Json(serde_json::json!({
                "error": "Trace not found",
            }))
        }
    }
}

/// Serve JavaScript file
async fn serve_js() -> impl IntoResponse {
    let js = include_str!("ui/app.js");
    Response::builder()
        .header("Content-Type", "application/javascript")
        .body(js.to_string())
        .unwrap()
}
