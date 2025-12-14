# Hindsight Implementation Plan
## Distributed Tracing Made Simple

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Phase 1: Protocol & Core Types](#phase-1-protocol--core-types)
3. [Phase 2: Client Library](#phase-2-client-library)
4. [Phase 3: Server - Storage & API](#phase-3-server---storage--api)
5. [Phase 4: Web UI](#phase-4-web-ui)
6. [Phase 5: TUI](#phase-5-tui)
7. [Phase 6: Integrations](#phase-6-integrations)
8. [Testing Strategy](#testing-strategy)
9. [Future Enhancements](#future-enhancements)

---

## Architecture Overview

### System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Applications                              │
├──────────────┬──────────────┬──────────────┬────────────────┤
│  Your App    │   Rapace     │  Picante     │    Dodeca      │
│ (hindsight)  │  (opt-in)    │  (opt-in)    │  (hindsight)   │
└──────┬───────┴──────┬───────┴──────┬───────┴─────┬──────────┘
       │              │              │             │
       │ W3C Trace Context (traceparent header)    │
       │              │              │             │
       ▼              ▼              ▼             ▼
┌─────────────────────────────────────────────────────────────┐
│              Hindsight Server (port 9090)                    │
├─────────────────────────────────────────────────────────────┤
│  Ingestion Layer:                                           │
│  - POST /v1/traces (HTTP)                                   │
│  - WebSocket /v1/traces/stream                              │
│  - Future: Rapace RPC                                       │
├─────────────────────────────────────────────────────────────┤
│  Storage Layer:                                             │
│  - TraceStore (DashMap<TraceId, Trace>)                     │
│  - SpanStore (DashMap<SpanId, Span>)                        │
│  - TTL: 1 hour default (configurable)                       │
│  - Future: Disk persistence                                 │
├─────────────────────────────────────────────────────────────┤
│  Query Layer:                                               │
│  - GET /v1/traces/:trace_id                                 │
│  - GET /v1/traces (list, filter by service/time)            │
│  - WebSocket /v1/traces/live (streaming updates)            │
├─────────────────────────────────────────────────────────────┤
│  UI Layer:                                                  │
│  - GET / (embedded web UI)                                  │
│  - WebSocket /v1/ui/stream (live UI updates)                │
└─────────────────────────────────────────────────────────────┘
              │                            │
              ▼                            ▼
     ┌────────────────┐          ┌──────────────────┐
     │   Browser      │          │  hindsight-tui   │
     │   (Web UI)     │          │  (Terminal UI)   │
     └────────────────┘          └──────────────────┘
```

### Data Model

**W3C Trace Context:**
- **TraceId**: 16 bytes (128-bit random)
- **SpanId**: 8 bytes (64-bit random)
- **traceparent header**: `00-{trace_id}-{span_id}-{flags}`

**Span:**
```rust
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub attributes: HashMap<String, AttributeValue>,
    pub events: Vec<SpanEvent>,
    pub status: SpanStatus,
    pub service_name: String,
}
```

**Trace:**
```rust
pub struct Trace {
    pub trace_id: TraceId,
    pub spans: Vec<Span>,
    pub root_span_id: SpanId,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub duration: Option<Duration>,
}
```

---

## Phase 1: Protocol & Core Types

**Goal:** Define the shared protocol between client and server.

**Crate:** `hindsight-protocol`

### 1.1 W3C Trace Context (`src/trace_context.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

/// 16-byte trace ID (128 bits)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// Generate a new random trace ID
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).expect("failed to generate random trace ID");
        Self(bytes)
    }

    /// Parse from hex string (W3C format: 32 hex chars)
    pub fn from_hex(s: &str) -> Result<Self, TraceContextError> {
        if s.len() != 32 {
            return Err(TraceContextError::InvalidLength);
        }
        let bytes = hex::decode(s).map_err(|_| TraceContextError::InvalidHex)?;
        Ok(Self(bytes.try_into().unwrap()))
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// 8-byte span ID (64 bits)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// Generate a new random span ID
    pub fn new() -> Self {
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).expect("failed to generate random span ID");
        Self(bytes)
    }

    /// Parse from hex string (W3C format: 16 hex chars)
    pub fn from_hex(s: &str) -> Result<Self, TraceContextError> {
        if s.len() != 16 {
            return Err(TraceContextError::InvalidLength);
        }
        let bytes = hex::decode(s).map_err(|_| TraceContextError::InvalidHex)?;
        Ok(Self(bytes.try_into().unwrap()))
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// W3C traceparent header: "00-{trace_id}-{span_id}-{flags}"
#[derive(Clone, Debug)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub flags: u8,
}

impl TraceContext {
    /// Create a new root trace context
    pub fn new_root() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            parent_span_id: None,
            flags: 0x01, // Sampled
        }
    }

    /// Create a child span in the same trace
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: SpanId::new(),
            parent_span_id: Some(self.span_id),
            flags: self.flags,
        }
    }

    /// Parse from W3C traceparent header
    /// Format: "00-{trace_id}-{span_id}-{flags}"
    pub fn from_traceparent(header: &str) -> Result<Self, TraceContextError> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return Err(TraceContextError::InvalidFormat);
        }

        if parts[0] != "00" {
            return Err(TraceContextError::UnsupportedVersion);
        }

        let trace_id = TraceId::from_hex(parts[1])?;
        let span_id = SpanId::from_hex(parts[2])?;
        let flags = u8::from_str_radix(parts[3], 16)
            .map_err(|_| TraceContextError::InvalidHex)?;

        Ok(Self {
            trace_id,
            span_id,
            parent_span_id: None, // Parent not in traceparent header
            flags,
        })
    }

    /// Format as W3C traceparent header
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.span_id.to_hex(),
            self.flags
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TraceContextError {
    #[error("invalid traceparent format")]
    InvalidFormat,
    #[error("unsupported trace context version")]
    UnsupportedVersion,
    #[error("invalid hex encoding")]
    InvalidHex,
    #[error("invalid length")]
    InvalidLength,
}
```

### 1.2 Span Types (`src/span.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace_context::{TraceId, SpanId};

/// Timestamp in nanoseconds since UNIX epoch
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos() as u64;
        Self(nanos)
    }
}

/// Span represents a single operation in a trace
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub attributes: HashMap<String, AttributeValue>,
    pub events: Vec<SpanEvent>,
    pub status: SpanStatus,
    pub service_name: String,
}

impl Span {
    /// Calculate span duration in nanoseconds
    pub fn duration_nanos(&self) -> Option<u64> {
        self.end_time.map(|end| end.0 - self.start_time.0)
    }
}

/// Attribute value (simplified for now)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Event within a span
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: Timestamp,
    pub attributes: HashMap<String, AttributeValue>,
}

/// Span completion status
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpanStatus {
    Ok,
    Error { message: String },
}

/// Complete trace (collection of spans)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: TraceId,
    pub spans: Vec<Span>,
    pub root_span_id: SpanId,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
}

impl Trace {
    /// Build a trace tree from a flat list of spans
    pub fn from_spans(mut spans: Vec<Span>) -> Self {
        spans.sort_by_key(|s| s.start_time.0);

        let trace_id = spans[0].trace_id;
        let root_span = spans.iter().find(|s| s.parent_span_id.is_none())
            .expect("no root span found");
        let root_span_id = root_span.span_id;
        let start_time = root_span.start_time;

        let end_time = spans.iter()
            .filter_map(|s| s.end_time)
            .max_by_key(|t| t.0);

        Self {
            trace_id,
            spans,
            root_span_id,
            start_time,
            end_time,
        }
    }

    /// Get children of a given span
    pub fn children(&self, span_id: SpanId) -> Vec<&Span> {
        self.spans.iter()
            .filter(|s| s.parent_span_id == Some(span_id))
            .collect()
    }
}
```

### 1.3 HTTP Protocol (`src/http.rs`)

```rust
use serde::{Deserialize, Serialize};
use crate::span::Span;

/// Request to ingest spans
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestRequest {
    pub spans: Vec<Span>,
}

/// Response from ingestion
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub accepted: usize,
}

/// Query parameters for listing traces
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListTracesQuery {
    pub service: Option<String>,
    pub limit: Option<usize>,
    pub since: Option<u64>, // Unix timestamp
}
```

**File Structure:**
```
crates/hindsight-protocol/src/
├── lib.rs
├── trace_context.rs
├── span.rs
└── http.rs
```

**Testing:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traceparent_roundtrip() {
        let ctx = TraceContext::new_root();
        let header = ctx.to_traceparent();
        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
    }

    #[test]
    fn test_child_span() {
        let root = TraceContext::new_root();
        let child = root.child();
        assert_eq!(child.trace_id, root.trace_id);
        assert_eq!(child.parent_span_id, Some(root.span_id));
    }
}
```

---

## Phase 2: Client Library

**Goal:** Provide a simple API for applications to send spans.

**Crate:** `hindsight`

### 2.1 Tracer (`src/tracer.rs`)

```rust
use hindsight_protocol::*;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Main entry point for sending spans
pub struct Tracer {
    inner: Arc<TracerInner>,
}

struct TracerInner {
    server_url: String,
    service_name: String,
    http_client: reqwest::Client,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl Tracer {
    /// Connect to a Hindsight server
    pub async fn connect(server_url: impl Into<String>) -> Result<Self, TracerError> {
        let server_url = server_url.into();

        // Detect service name (from env, or default)
        let service_name = std::env::var("HINDSIGHT_SERVICE_NAME")
            .unwrap_or_else(|_| "unknown".to_string());

        let http_client = reqwest::Client::new();

        // Channel for buffering spans before sending
        let (span_tx, mut span_rx) = mpsc::unbounded_channel();

        // Background task to batch and send spans
        let inner = Arc::new(TracerInner {
            server_url: server_url.clone(),
            service_name,
            http_client: http_client.clone(),
            span_tx,
        });

        let inner_clone = inner.clone();
        tokio::spawn(async move {
            let mut batch = Vec::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            send_batch(&inner_clone, &batch).await;
                            batch.clear();
                        }
                    }
                    Some(span) = span_rx.recv() => {
                        batch.push(span);
                        if batch.len() >= 100 {
                            send_batch(&inner_clone, &batch).await;
                            batch.clear();
                        }
                    }
                }
            }
        });

        Ok(Self { inner })
    }

    /// Start building a new span
    pub fn span(&self, name: impl Into<String>) -> SpanBuilder {
        SpanBuilder::new(
            name.into(),
            self.inner.service_name.clone(),
            self.inner.span_tx.clone(),
        )
    }
}

async fn send_batch(inner: &TracerInner, spans: &[Span]) {
    let req = IngestRequest {
        spans: spans.to_vec(),
    };

    let url = format!("{}/v1/traces", inner.server_url);
    let _ = inner.http_client
        .post(&url)
        .json(&req)
        .send()
        .await;
    // Ignore errors (fire-and-forget for now)
}

#[derive(Debug, thiserror::Error)]
pub enum TracerError {
    #[error("failed to connect to server")]
    ConnectionFailed,
}
```

### 2.2 Span Builder (`src/span_builder.rs`)

```rust
use hindsight_protocol::*;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Builder for creating and starting spans
pub struct SpanBuilder {
    name: String,
    service_name: String,
    attributes: HashMap<String, AttributeValue>,
    parent: Option<TraceContext>,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl SpanBuilder {
    pub(crate) fn new(
        name: String,
        service_name: String,
        span_tx: mpsc::UnboundedSender<Span>,
    ) -> Self {
        Self {
            name,
            service_name,
            attributes: HashMap::new(),
            parent: None,
            span_tx,
        }
    }

    /// Set the parent trace context (for propagation)
    pub fn with_parent(mut self, parent: TraceContext) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Add an attribute to the span
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Start the span
    pub fn start(self) -> ActiveSpan {
        let context = if let Some(parent) = self.parent {
            parent.child()
        } else {
            TraceContext::new_root()
        };

        let span = Span {
            trace_id: context.trace_id,
            span_id: context.span_id,
            parent_span_id: context.parent_span_id,
            name: self.name,
            start_time: Timestamp::now(),
            end_time: None,
            attributes: self.attributes,
            events: Vec::new(),
            status: SpanStatus::Ok,
            service_name: self.service_name,
        };

        ActiveSpan {
            span,
            context,
            span_tx: self.span_tx,
        }
    }
}

/// Active span (not yet finished)
pub struct ActiveSpan {
    span: Span,
    context: TraceContext,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl ActiveSpan {
    /// Get the trace context (for propagation to downstream calls)
    pub fn context(&self) -> &TraceContext {
        &self.context
    }

    /// Add an event to the span
    pub fn add_event(&mut self, name: impl Into<String>) {
        self.span.events.push(SpanEvent {
            name: name.into(),
            timestamp: Timestamp::now(),
            attributes: HashMap::new(),
        });
    }

    /// Mark the span as errored
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.span.status = SpanStatus::Error {
            message: message.into(),
        };
    }

    /// End the span and send it to the server
    pub fn end(mut self) {
        self.span.end_time = Some(Timestamp::now());
        let _ = self.span_tx.send(self.span);
    }
}

// Conversion helpers
impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        AttributeValue::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        AttributeValue::String(s)
    }
}

impl From<i64> for AttributeValue {
    fn from(i: i64) -> Self {
        AttributeValue::Int(i)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        AttributeValue::Bool(b)
    }
}
```

**Example Usage:**
```rust
use hindsight::Tracer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer = Tracer::connect("http://localhost:9090").await?;

    let span = tracer.span("process_request")
        .with_attribute("user_id", 123)
        .with_attribute("endpoint", "/api/users")
        .start();

    // Do work...
    process_request().await?;

    span.end();
    Ok(())
}
```

---

## Phase 3: Server - Storage & API

**Goal:** Receive, store, and query traces.

**Crate:** `hindsight-server`

### 3.1 Storage (`src/storage.rs`)

```rust
use dashmap::DashMap;
use hindsight_protocol::*;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// In-memory trace store with TTL
pub struct TraceStore {
    traces: DashMap<TraceId, StoredTrace>,
    spans: DashMap<SpanId, Span>,
    ttl: Duration,
}

struct StoredTrace {
    trace: Trace,
    created_at: SystemTime,
}

impl TraceStore {
    pub fn new(ttl: Duration) -> Self {
        let store = Self {
            traces: DashMap::new(),
            spans: DashMap::new(),
            ttl,
        };

        // Background task to clean up expired traces
        let store_clone = Arc::new(store);
        let store_weak = Arc::downgrade(&store_clone);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Some(store) = store_weak.upgrade() {
                    store.cleanup_expired();
                } else {
                    break;
                }
            }
        });

        Arc::try_unwrap(store_clone).unwrap()
    }

    /// Ingest spans and build traces
    pub fn ingest(&self, spans: Vec<Span>) {
        for span in spans {
            self.spans.insert(span.span_id, span.clone());

            // Try to build/update trace
            self.update_trace(span.trace_id);
        }
    }

    /// Get a complete trace by ID
    pub fn get_trace(&self, trace_id: TraceId) -> Option<Trace> {
        self.traces.get(&trace_id).map(|entry| entry.trace.clone())
    }

    /// List all traces (with optional filters)
    pub fn list_traces(&self, service: Option<&str>, limit: usize) -> Vec<Trace> {
        self.traces
            .iter()
            .filter(|entry| {
                if let Some(service_name) = service {
                    entry.trace.spans.iter().any(|s| s.service_name == service_name)
                } else {
                    true
                }
            })
            .take(limit)
            .map(|entry| entry.trace.clone())
            .collect()
    }

    fn update_trace(&self, trace_id: TraceId) {
        // Collect all spans for this trace
        let spans: Vec<Span> = self.spans
            .iter()
            .filter(|entry| entry.value().trace_id == trace_id)
            .map(|entry| entry.value().clone())
            .collect();

        if !spans.is_empty() {
            let trace = Trace::from_spans(spans);
            self.traces.insert(trace_id, StoredTrace {
                trace,
                created_at: SystemTime::now(),
            });
        }
    }

    fn cleanup_expired(&self) {
        let now = SystemTime::now();
        self.traces.retain(|_, stored| {
            now.duration_since(stored.created_at).unwrap_or_default() < self.ttl
        });
    }
}
```

### 3.2 HTTP Server (`src/server.rs`)

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use hindsight_protocol::*;
use std::sync::Arc;
use std::time::Duration;

use crate::storage::TraceStore;

pub async fn run_server(host: &str, port: u16) -> anyhow::Result<()> {
    let store = Arc::new(TraceStore::new(Duration::from_secs(3600))); // 1 hour TTL

    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/v1/traces", post(ingest_traces))
        .route("/v1/traces/:trace_id", get(get_trace))
        .route("/v1/traces", get(list_traces))
        .with_state(store);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🔍 Hindsight server listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_ui() -> &'static str {
    // TODO: Serve embedded HTML
    "Hindsight UI coming soon!"
}

async fn ingest_traces(
    State(store): State<Arc<TraceStore>>,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, StatusCode> {
    let count = req.spans.len();
    store.ingest(req.spans);

    Ok(Json(IngestResponse { accepted: count }))
}

async fn get_trace(
    State(store): State<Arc<TraceStore>>,
    Path(trace_id): Path<String>,
) -> Result<Json<Trace>, StatusCode> {
    let trace_id = TraceId::from_hex(&trace_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    store.get_trace(trace_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn list_traces(
    State(store): State<Arc<TraceStore>>,
    Query(query): Query<ListTracesQuery>,
) -> Json<Vec<Trace>> {
    let traces = store.list_traces(
        query.service.as_deref(),
        query.limit.unwrap_or(100),
    );
    Json(traces)
}
```

### 3.3 Main Binary (`src/main.rs`)

Update the placeholder to actually run the server:

```rust
// ... (keep existing CLI code) ...

match cli.command {
    Commands::Serve { port, host } => {
        println!("🔍 Hindsight server starting on http://{}:{}", host, port);
        println!("   Web UI: http://{}:{}", host, port);
        println!("   API: http://{}:{}/v1/traces", host, port);

        crate::server::run_server(&host, port).await?;
        Ok(())
    }
}
```

---

## Phase 4: Web UI

**Goal:** Beautiful browser-based trace visualization.

**Location:** `crates/hindsight-server/src/ui/`

### 4.1 Embedded HTML (`src/ui/index.html`)

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hindsight - Distributed Tracing</title>
    <script src="https://unpkg.com/d3@7"></script>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: #0d1117;
            color: #c9d1d9;
        }
        #app {
            display: flex;
            height: 100vh;
        }
        #sidebar {
            width: 300px;
            background: #161b22;
            border-right: 1px solid #30363d;
            padding: 20px;
            overflow-y: auto;
        }
        #main {
            flex: 1;
            display: flex;
            flex-direction: column;
        }
        #toolbar {
            background: #161b22;
            border-bottom: 1px solid #30363d;
            padding: 15px 20px;
            display: flex;
            gap: 10px;
        }
        #trace-view {
            flex: 1;
            overflow: auto;
            padding: 20px;
        }

        /* Trace list */
        .trace-item {
            background: #0d1117;
            border: 1px solid #30363d;
            border-radius: 6px;
            padding: 12px;
            margin-bottom: 10px;
            cursor: pointer;
            transition: border-color 0.2s;
        }
        .trace-item:hover {
            border-color: #58a6ff;
        }
        .trace-id {
            font-family: monospace;
            font-size: 11px;
            color: #8b949e;
        }
        .trace-duration {
            font-weight: 600;
            color: #58a6ff;
        }

        /* Waterfall view */
        .waterfall {
            position: relative;
        }
        .span-row {
            display: flex;
            align-items: center;
            padding: 4px 0;
            border-bottom: 1px solid #21262d;
        }
        .span-label {
            width: 200px;
            font-size: 12px;
            padding-left: var(--indent);
        }
        .span-timeline {
            flex: 1;
            height: 24px;
            position: relative;
        }
        .span-bar {
            position: absolute;
            height: 18px;
            background: #58a6ff;
            border-radius: 3px;
            top: 3px;
        }
        .span-bar.error {
            background: #f85149;
        }
        .span-duration {
            position: absolute;
            font-size: 11px;
            color: #fff;
            padding: 0 6px;
            line-height: 18px;
        }

        /* Button */
        button {
            background: #238636;
            color: #fff;
            border: none;
            padding: 8px 16px;
            border-radius: 6px;
            cursor: pointer;
            font-size: 14px;
        }
        button:hover {
            background: #2ea043;
        }
    </style>
</head>
<body>
    <div id="app">
        <div id="sidebar">
            <h2>Traces</h2>
            <div id="trace-list"></div>
        </div>
        <div id="main">
            <div id="toolbar">
                <button onclick="refreshTraces()">Refresh</button>
                <span id="status">Loading...</span>
            </div>
            <div id="trace-view"></div>
        </div>
    </div>

    <script>
        // State
        let traces = [];
        let selectedTrace = null;

        // Load traces
        async function refreshTraces() {
            document.getElementById('status').textContent = 'Loading...';
            const res = await fetch('/v1/traces?limit=50');
            traces = await res.json();
            renderTraceList();
            document.getElementById('status').textContent = `${traces.length} traces`;
        }

        // Render trace list
        function renderTraceList() {
            const list = document.getElementById('trace-list');
            list.innerHTML = traces.map(trace => `
                <div class="trace-item" onclick="selectTrace('${trace.trace_id}')">
                    <div class="trace-id">${trace.trace_id}</div>
                    <div>
                        <span class="trace-duration">${formatDuration(trace.end_time - trace.start_time)}</span>
                        ${trace.spans.length} spans
                    </div>
                </div>
            `).join('');
        }

        // Select and render trace
        async function selectTrace(traceId) {
            const res = await fetch(`/v1/traces/${traceId}`);
            selectedTrace = await res.json();
            renderTraceWaterfall();
        }

        // Render waterfall
        function renderTraceWaterfall() {
            if (!selectedTrace) return;

            const view = document.getElementById('trace-view');
            const startTime = selectedTrace.start_time;
            const endTime = selectedTrace.end_time || Date.now() * 1e6;
            const totalDuration = endTime - startTime;

            // Build tree
            const tree = buildSpanTree(selectedTrace);

            view.innerHTML = `
                <h3>Trace ${selectedTrace.trace_id.substring(0, 16)}...</h3>
                <div class="waterfall">
                    ${renderSpanTree(tree, 0, startTime, totalDuration)}
                </div>
            `;
        }

        function buildSpanTree(trace) {
            const spanMap = new Map(trace.spans.map(s => [s.span_id, s]));
            const root = trace.spans.find(s => s.span_id === trace.root_span_id);

            function buildNode(span) {
                const children = trace.spans
                    .filter(s => s.parent_span_id === span.span_id)
                    .map(buildNode);
                return { span, children };
            }

            return buildNode(root);
        }

        function renderSpanTree(node, depth, traceStart, totalDuration) {
            const span = node.span;
            const start = span.start_time - traceStart;
            const duration = (span.end_time || Date.now() * 1e6) - span.start_time;
            const left = (start / totalDuration) * 100;
            const width = (duration / totalDuration) * 100;

            const errorClass = span.status.type === 'Error' ? 'error' : '';

            const html = `
                <div class="span-row">
                    <div class="span-label" style="--indent: ${depth * 20}px">
                        ${span.name}
                    </div>
                    <div class="span-timeline">
                        <div class="span-bar ${errorClass}" style="left: ${left}%; width: ${width}%">
                            <span class="span-duration">${formatDuration(duration)}</span>
                        </div>
                    </div>
                </div>
            `;

            return html + node.children.map(child => renderSpanTree(child, depth + 1, traceStart, totalDuration)).join('');
        }

        function formatDuration(nanos) {
            if (nanos < 1000) return `${nanos}ns`;
            if (nanos < 1e6) return `${(nanos / 1000).toFixed(1)}µs`;
            if (nanos < 1e9) return `${(nanos / 1e6).toFixed(1)}ms`;
            return `${(nanos / 1e9).toFixed(2)}s`;
        }

        // Initial load
        refreshTraces();
        setInterval(refreshTraces, 5000); // Auto-refresh every 5s
    </script>
</body>
</html>
```

### 4.2 Serve UI (`src/server.rs`)

Update `serve_ui` function:

```rust
async fn serve_ui() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("ui/index.html"))
}
```

---

## Phase 5: TUI

**Goal:** Terminal UI for SSH-friendly trace viewing.

**Crate:** `hindsight-tui`

### 5.1 Main TUI App (`src/app.rs`)

```rust
use crossterm::event::{self, Event, KeyCode};
use hindsight_protocol::*;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;

pub struct App {
    server_url: String,
    traces: Vec<Trace>,
    selected_index: usize,
    selected_trace: Option<Trace>,
}

impl App {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            traces: Vec::new(),
            selected_index: 0,
            selected_trace: None,
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        // Setup terminal
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Fetch initial data
        self.refresh().await?;

        // Event loop
        loop {
            terminal.draw(|f| self.ui(f))?;

            if event::poll(std::time::Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('r') => {
                            self.refresh().await?;
                        }
                        KeyCode::Up => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                                self.select_current_trace().await?;
                            }
                        }
                        KeyCode::Down => {
                            if self.selected_index < self.traces.len() - 1 {
                                self.selected_index += 1;
                                self.select_current_trace().await?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Cleanup terminal
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;

        Ok(())
    }

    async fn refresh(&mut self) -> io::Result<()> {
        let url = format!("{}/v1/traces?limit=50", self.server_url);
        self.traces = reqwest::get(&url)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            .json()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        if !self.traces.is_empty() {
            self.select_current_trace().await?;
        }

        Ok(())
    }

    async fn select_current_trace(&mut self) -> io::Result<()> {
        if let Some(trace) = self.traces.get(self.selected_index) {
            let url = format!("{}/v1/traces/{}", self.server_url, trace.trace_id);
            self.selected_trace = Some(
                reqwest::get(&url)
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                    .json()
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
            );
        }
        Ok(())
    }

    fn ui(&self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(f.size());

        // Trace list (left panel)
        let items: Vec<ListItem> = self
            .traces
            .iter()
            .enumerate()
            .map(|(i, trace)| {
                let duration = trace.end_time.map(|e| e.0 - trace.start_time.0).unwrap_or(0);
                let text = format!(
                    "{} | {} | {} spans",
                    &trace.trace_id.to_string()[0..8],
                    format_duration(duration),
                    trace.spans.len()
                );
                let style = if i == self.selected_index {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Traces (↑/↓ to select, r to refresh, q to quit)").borders(Borders::ALL));

        f.render_widget(list, chunks[0]);

        // Trace details (right panel)
        if let Some(trace) = &self.selected_trace {
            let lines: Vec<Line> = trace
                .spans
                .iter()
                .map(|span| {
                    let indent = "  ".repeat(self.calculate_depth(trace, span.span_id));
                    let duration = span.end_time.map(|e| e.0 - span.start_time.0).unwrap_or(0);
                    Line::from(vec![
                        Span::raw(indent),
                        Span::styled(&span.name, Style::default().fg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(format_duration(duration), Style::default().fg(Color::Green)),
                    ])
                })
                .collect();

            let paragraph = Paragraph::new(lines)
                .block(Block::default().title("Trace Details").borders(Borders::ALL));

            f.render_widget(paragraph, chunks[1]);
        }
    }

    fn calculate_depth(&self, trace: &Trace, span_id: SpanId) -> usize {
        let span = trace.spans.iter().find(|s| s.span_id == span_id).unwrap();
        if let Some(parent_id) = span.parent_span_id {
            1 + self.calculate_depth(trace, parent_id)
        } else {
            0
        }
    }
}

fn format_duration(nanos: u64) -> String {
    if nanos < 1000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.1}µs", nanos as f64 / 1000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.1}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}
```

### 5.2 Main Binary (`src/main.rs`)

Update to run the TUI:

```rust
mod app;

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

    let mut app = app::App::new(cli.connect);
    app.run().await?;

    Ok(())
}
```

---

## Phase 6: Integrations

### 6.1 Rapace Integration

**Location:** `rapace-core/src/tracing_support.rs` (new file, feature-gated)

```rust
#[cfg(feature = "hindsight")]
use hindsight::{Tracer, ActiveSpan};

impl<T: Transport> RpcSession<T> {
    #[cfg(feature = "hindsight")]
    pub fn with_tracer(mut self, tracer: Tracer) -> Self {
        self.tracer = Some(tracer);
        self
    }

    // In call() method, wrap with span:
    pub async fn call(&self, channel_id: u32, method_id: u32, payload: Bytes) -> Result<ReceivedFrame, RpcError> {
        #[cfg(feature = "hindsight")]
        let span = if let Some(tracer) = &self.tracer {
            Some(tracer.span(format!("RPC: method_{}", method_id))
                .with_attribute("channel_id", channel_id as i64)
                .start())
        } else {
            None
        };

        // ... existing call logic ...

        #[cfg(feature = "hindsight")]
        if let Some(span) = span {
            span.end();
        }

        result
    }
}
```

### 6.2 Picante Integration

**Location:** `picante/src/runtime.rs`

```rust
impl Runtime {
    #[cfg(feature = "hindsight")]
    pub fn with_tracer(mut self, tracer: hindsight::Tracer) -> Self {
        self.tracer = Some(tracer);
        self
    }
}

// In query execution, emit spans
#[cfg(feature = "hindsight")]
if let Some(tracer) = &db.runtime().tracer {
    let span = tracer.span(format!("Query: {}", query_name))
        .with_attribute("cache_hit", cache_hit)
        .start();
    // ... execution ...
    span.end();
}
```

---

## Testing Strategy

### Unit Tests
- `hindsight-protocol`: Trace context parsing, span serialization
- `hindsight`: Span builder API, batching logic
- `hindsight-server`: Storage, TTL cleanup

### Integration Tests
```rust
#[tokio::test]
async fn test_end_to_end_trace() {
    // Start server
    let server = start_test_server().await;

    // Send spans via client
    let tracer = Tracer::connect(server.url()).await.unwrap();
    let span = tracer.span("test").start();
    span.end();

    // Wait for batch send
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Query trace
    let traces = server.list_traces().await;
    assert_eq!(traces.len(), 1);
}
```

### Manual Testing
1. Run server: `cargo run -p hindsight-server -- serve`
2. Run example app that sends spans
3. View in browser at http://localhost:9090
4. Run TUI: `cargo run -p hindsight-tui`

---

## Future Enhancements

1. **Persistent Storage**
   - SQLite backend for long-term trace storage
   - Export to Parquet for analysis

2. **Sampling**
   - Configurable sampling rate (e.g., 1% of traces)
   - Head-based and tail-based sampling

3. **Advanced UI**
   - Flamegraph view
   - Service dependency graph
   - Latency histograms

4. **Export Formats**
   - Jaeger format
   - Zipkin format
   - OpenTelemetry OTLP

5. **Rapace RPC Ingestion**
   - Alternative to HTTP for lower overhead
   - Bi-directional streaming

6. **Alerting**
   - Threshold-based alerts (latency, error rate)
   - Webhook notifications

---

## File Checklist

- [x] `README.md`
- [x] `Cargo.toml` (workspace)
- [x] `.gitignore`
- [x] `LICENSE-MIT`, `LICENSE-APACHE`
- [x] `.github/workflows/ci.yml`
- [x] `crates/hindsight-protocol/` (complete implementation needed)
- [x] `crates/hindsight/` (complete implementation needed)
- [x] `crates/hindsight-server/` (complete implementation needed)
- [x] `crates/hindsight-tui/` (complete implementation needed)
- [x] `PLAN.md`

---

## Getting Started (For New Contributors)

1. **Read the README** to understand the project vision
2. **Start with Phase 1** (protocol) - it's the foundation
3. **Run the tests** as you implement each phase
4. **Build incrementally** - get each phase working before moving to the next
5. **Ask questions** if anything is unclear!

The goal is to have a working MVP by the end of Phase 3, with UI polish in Phases 4-5.

Happy hacking! 🚀
