# Hindsight Implementation Plan
## Distributed Tracing Made Simple - Pure Rapace Edition

> Archived draft. Superseded by the unified plan at `PLAN.md`.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Avoiding Infinite Loops](#avoiding-infinite-loops)
3. [Phase 1: Protocol & Core Types](#phase-1-protocol--core-types)
4. [Phase 2: Rapace Service Definition](#phase-2-rapace-service-definition)
5. [Phase 3: Client Library](#phase-3-client-library)
6. [Phase 4: Server Implementation](#phase-4-server-implementation)
7. [Phase 5: Web UI (WASM + Rapace)](#phase-5-web-ui-wasm--rapace)
8. [Phase 6: TUI (Native Rapace)](#phase-6-tui-native-rapace)
9. [Phase 7: Integrations](#phase-7-integrations)
10. [Testing Strategy](#testing-strategy)
11. [Future Enhancements](#future-enhancements)

---

## Architecture Overview

### Pure Rapace Design

**Key Decision:** Hindsight uses **only Rapace RPC** for all communication. No REST APIs, no custom protocols.

```
Applications                        Hindsight Server
┌───────────────┐                  ┌──────────────────────────┐
│  Your App     │                  │  HindsightService        │
│  (rapace)     │──Rapace RPC─────▶│  (Rapace service)        │
│               │  TCP/SHM/Unix    │  - ingest_spans()        │
└───────────────┘                  │  - get_trace()           │
                                   │  - list_traces()         │
┌───────────────┐                  │  - stream_traces()       │
│  Picante      │──Rapace RPC─────▶│                          │
│  (rapace)     │  TCP/SHM/Unix    └───────────┬──────────────┘
└───────────────┘                              │
                                               │ Rapace RPC
┌───────────────┐                              │ (different transports)
│  Dodeca       │──Rapace RPC─────▶            │
│  (rapace)     │  TCP/SHM/Unix    ┌───────────┴──────────────┐
└───────────────┘                  │                          │
                                   │                          │
                    ┌──────────────┴─┐          ┌─────────────┴─────┐
                    ▼                ▼          ▼                   ▼
              ┌─────────┐      ┌─────────┐  ┌─────────┐      ┌──────────┐
              │ Browser │      │   TUI   │  │ Rapace  │      │  Picante │
              │ (WASM)  │      │ (Native)│  │  Cell   │      │  (opt-in)│
              └─────────┘      └─────────┘  └─────────┘      └──────────┘
                   │                │            │                  │
                   │ rapace-        │ rapace-    │ rapace-         │ rapace-
                   │ transport-     │ transport- │ transport-      │ transport-
                   │ websocket      │ tcp        │ tcp             │ shm
                   │                │            │                  │
                   └────────────────┴────────────┴──────────────────┘
                          ALL USING RAPACE RPC!
```

### Why Pure Rapace?

1. **Dogfooding** - We use our own technology
2. **Efficient** - Binary protocol, multiplexed channels
3. **Type-safe** - Generated clients, no manual JSON parsing
4. **Consistent** - Same protocol everywhere (native, browser, embedded)
5. **Streaming** - Native Rapace streaming, not WebSocket hacks
6. **Transport-agnostic** - TCP, WebSocket, SHM, Unix sockets

### The Only HTTP

**One tiny route** to serve the static HTML page that loads the WASM client:

```rust
// Minimal axum server ONLY for serving the WASM client
Router::new()
    .route("/", get(|| async {
        Html(include_str!("ui/index.html"))
    }))
```

Everything else? **Pure Rapace!**

---

## Avoiding Infinite Loops

### The Problem

If Hindsight traces itself, we get infinite recursion:

```
1. App sends span to Hindsight via Rapace
2. Hindsight's Rapace session is traced
3. Rapace sends span to Hindsight
4. Hindsight's Rapace session is traced...
∞. Stack overflow! 💥
```

### Solution 1: Untraced Sessions

**Mark Hindsight's internal sessions as untraced:**

```rust
// In hindsight-server: create RPC session WITHOUT tracer
let session = RpcSession::new(transport);
// NO .with_tracer() call!

// This session receives spans but doesn't emit them
let service = HindsightServiceImpl::new(store);
session.set_dispatcher(HindsightServiceServer::new(service));
```

**Key rule:** Hindsight's own RPC sessions **never** have a tracer attached.

### Solution 2: Service-Level Filtering

Applications can configure which services to trace:

```rust
// In your application
let tracer = Tracer::connect("tcp://localhost:9090").await?
    .with_service_filter(|service_name| {
        // Don't trace calls TO Hindsight
        service_name != "HindsightService"
    });

let session = RpcSession::new(transport)
    .with_tracer(tracer);
```

### Solution 3: Opt-In Tracing

**Default:** Rapace sessions are **not traced** unless explicitly opted in.

```rust
// Rapace without tracing (default)
let session = RpcSession::new(transport);

// Rapace WITH tracing (explicit opt-in)
let session = RpcSession::new(transport)
    .with_tracer(tracer); // <-- Only if you want tracing
```

This means:
- Hindsight's own sessions: **no tracer** ✅
- Your app's sessions: **optionally traced** ✅
- Picante's internal RPC: **optionally traced** ✅

### Solution 4: Trace ID Propagation

Hindsight can detect cycles by checking trace IDs:

```rust
impl HindsightServiceImpl {
    async fn ingest_spans(&self, spans: Vec<Span>) {
        // Filter out spans that are FROM Hindsight itself
        let spans: Vec<_> = spans.into_iter()
            .filter(|span| span.service_name != "hindsight-server")
            .collect();

        self.store.ingest(spans);
    }
}
```

### Documentation

Clear docs in README.md:

```markdown
## Avoiding Self-Tracing

⚠️ **Important:** Do NOT attach a tracer to Hindsight's RPC sessions!

**Correct:**
```rust
// Hindsight server creates session WITHOUT tracer
let session = RpcSession::new(transport);
session.set_dispatcher(HindsightServiceServer::new(service));
```

**Incorrect (causes infinite loop):**
```rust
// ❌ DON'T DO THIS!
let session = RpcSession::new(transport)
    .with_tracer(tracer); // <-- NO! Infinite loop!
```
```

---

## Phase 1: Protocol & Core Types

**Goal:** Define W3C Trace Context and span data model using Facet for serialization.

**Crate:** `hindsight-protocol`

### 1.1 Dependencies

```toml
[dependencies]
facet = { path = "../../../facet" }
serde = { version = "1", features = ["derive"] }
time = { version = "0.3", features = ["serde"] }
thiserror = "1"
getrandom = "0.2"
hex = "0.4"
```

### 1.2 W3C Trace Context (`src/trace_context.rs`)

```rust
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 16-byte trace ID (128 bits)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, Facet)]
pub struct TraceId(pub [u8; 16]);

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
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, Facet)]
pub struct SpanId(pub [u8; 8]);

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
#[derive(Clone, Debug, Facet)]
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
            parent_span_id: None,
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

### 1.3 Span Types (`src/span.rs`)

```rust
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace_context::{TraceId, SpanId};

/// Timestamp in nanoseconds since UNIX epoch
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Facet)]
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
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
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

/// Attribute value
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Event within a span
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: Timestamp,
    pub attributes: HashMap<String, AttributeValue>,
}

/// Span completion status
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
#[serde(tag = "type")]
pub enum SpanStatus {
    Ok,
    Error { message: String },
}

/// Complete trace (collection of spans)
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct Trace {
    pub trace_id: TraceId,
    pub spans: Vec<Span>,
    pub root_span_id: SpanId,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
}

impl Trace {
    /// Build a trace from a flat list of spans
    pub fn from_spans(mut spans: Vec<Span>) -> Self {
        spans.sort_by_key(|s| s.start_time.0);

        let trace_id = spans[0].trace_id;
        let root_span = spans.iter()
            .find(|s| s.parent_span_id.is_none())
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

/// Summary of a trace (for listing)
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct TraceSummary {
    pub trace_id: TraceId,
    pub root_span_name: String,
    pub service_name: String,
    pub start_time: Timestamp,
    pub duration_nanos: Option<u64>,
    pub span_count: usize,
    pub has_errors: bool,
}

/// Filter for querying traces
#[derive(Clone, Debug, Default, Serialize, Deserialize, Facet)]
pub struct TraceFilter {
    pub service: Option<String>,
    pub min_duration_nanos: Option<u64>,
    pub max_duration_nanos: Option<u64>,
    pub has_errors: Option<bool>,
    pub limit: Option<usize>,
}
```

### 1.4 Event Types (`src/events.rs`)

```rust
use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::span::*;
use crate::trace_context::*;

/// Live event stream from Hindsight server
#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
#[serde(tag = "type")]
pub enum TraceEvent {
    /// New trace started
    TraceStarted {
        trace_id: TraceId,
        root_span_name: String,
        service_name: String,
    },

    /// Trace completed
    TraceCompleted {
        trace_id: TraceId,
        duration_nanos: u64,
        span_count: usize,
    },

    /// New span added to a trace
    SpanAdded {
        trace_id: TraceId,
        span: Span,
    },
}
```

**File Structure:**
```
crates/hindsight-protocol/src/
├── lib.rs
├── trace_context.rs
├── span.rs
└── events.rs
```

---

## Phase 2: Rapace Service Definition

**Goal:** Define the HindsightService using Rapace's service macro.

**Crate:** `hindsight-protocol`

### 2.1 Service Definition (`src/service.rs`)

```rust
use rapace::Streaming;
use facet::Facet;

use crate::span::*;
use crate::trace_context::*;
use crate::events::*;

/// Hindsight tracing service (pure Rapace RPC)
#[rapace::service]
pub trait HindsightService {
    /// Ingest a batch of spans
    ///
    /// Returns the number of spans accepted.
    ///
    /// Note: This is an untraced method to prevent infinite loops!
    async fn ingest_spans(&self, spans: Vec<Span>) -> u32;

    /// Get a specific trace by ID
    ///
    /// Returns None if the trace is not found or has expired.
    async fn get_trace(&self, trace_id: TraceId) -> Option<Trace>;

    /// List recent traces with optional filtering
    async fn list_traces(&self, filter: TraceFilter) -> Vec<TraceSummary>;

    /// Stream live trace events
    ///
    /// Emits events as traces are created, spans are added, and traces complete.
    /// This is a long-lived stream that continues until the client disconnects.
    async fn stream_traces(&self) -> Streaming<TraceEvent>;

    /// Health check (useful for monitoring)
    async fn ping(&self) -> String;
}
```

### 2.2 Update lib.rs

```rust
pub mod trace_context;
pub mod span;
pub mod events;
pub mod service;

pub use trace_context::*;
pub use span::*;
pub use events::*;
pub use service::*;
```

---

## Phase 3: Client Library

**Goal:** Provide a simple API for sending spans via Rapace.

**Crate:** `hindsight`

### 3.1 Dependencies

```toml
[dependencies]
hindsight-protocol = { path = "../hindsight-protocol" }
rapace = { path = "../../../rapace/crates/rapace" }

tokio = { version = "1", features = ["full"] }
facet = { path = "../../../facet" }

[features]
default = []
```

### 3.2 Tracer (`src/tracer.rs`)

```rust
use hindsight_protocol::*;
use rapace::{RpcSession, Transport};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Main entry point for sending spans
pub struct Tracer {
    inner: Arc<TracerInner>,
}

struct TracerInner {
    service_name: String,
    client: HindsightServiceClient,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl Tracer {
    /// Connect to a Hindsight server via Rapace
    ///
    /// # Example
    /// ```no_run
    /// // TCP transport
    /// let transport = rapace_transport_tcp::TcpTransport::connect("localhost:9090").await?;
    /// let tracer = Tracer::new(transport).await?;
    ///
    /// // SHM transport (for same-machine communication)
    /// let transport = rapace_transport_shm::ShmTransport::open("/tmp/hindsight.shm").await?;
    /// let tracer = Tracer::new(transport).await?;
    /// ```
    pub async fn new<T: Transport + 'static>(transport: T) -> Result<Self, TracerError> {
        // Detect service name (from env, or default)
        let service_name = std::env::var("HINDSIGHT_SERVICE_NAME")
            .unwrap_or_else(|_| "unknown".to_string());

        // Create Rapace session
        // IMPORTANT: Do NOT attach a tracer to this session!
        // (Prevents infinite loop)
        let session = Arc::new(RpcSession::new(Arc::new(transport)));

        // Create Rapace client
        let client = HindsightServiceClient::new(session.clone());

        // Channel for buffering spans before sending
        let (span_tx, mut span_rx) = mpsc::unbounded_channel();

        // Background task to batch and send spans
        let client_clone = client.clone();
        tokio::spawn(async move {
            let mut batch = Vec::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            let _ = client_clone.ingest_spans(batch.clone()).await;
                            batch.clear();
                        }
                    }
                    Some(span) = span_rx.recv() => {
                        batch.push(span);
                        if batch.len() >= 100 {
                            let _ = client_clone.ingest_spans(batch.clone()).await;
                            batch.clear();
                        }
                    }
                }
            }
        });

        let inner = Arc::new(TracerInner {
            service_name,
            client,
            span_tx,
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

    /// Get the underlying Hindsight client (for advanced usage)
    pub fn client(&self) -> &HindsightServiceClient {
        &self.inner.client
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TracerError {
    #[error("failed to connect to server")]
    ConnectionFailed,
}
```

### 3.3 Span Builder (`src/span_builder.rs`)

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

### 3.4 Example Usage

```rust
use hindsight::Tracer;
use rapace_transport_tcp::TcpTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Hindsight via TCP
    let transport = TcpTransport::connect("localhost:9090").await?;
    let tracer = Tracer::new(transport).await?;

    // Create a span
    let span = tracer.span("process_request")
        .with_attribute("user_id", 123)
        .with_attribute("endpoint", "/api/users")
        .start();

    // Do work...
    process_request().await?;

    // End the span (sends to Hindsight)
    span.end();

    Ok(())
}
```

---

## Phase 4: Server Implementation

**Goal:** Implement the HindsightService and serve it via Rapace.

**Crate:** `hindsight-server`

### 4.1 Dependencies

```toml
[dependencies]
hindsight-protocol = { path = "../hindsight-protocol" }
rapace = { path = "../../../rapace/crates/rapace" }
rapace-transport-tcp = { path = "../../../rapace/crates/rapace-transport-tcp" }
rapace-transport-websocket = { path = "../../../rapace/crates/rapace-transport-websocket" }

tokio = { version = "1", features = ["full"] }
facet = { path = "../../../facet" }

# Storage
dashmap = "5"
parking_lot = "0.12"

# HTTP (only for serving static HTML)
axum = { version = "0.7", features = ["ws"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Tracing (self-instrumentation - NOT sent to Hindsight!)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

time = "0.3"
anyhow = "1"
```

### 4.2 Storage (`src/storage.rs`)

```rust
use dashmap::DashMap;
use hindsight_protocol::*;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;

/// In-memory trace store with TTL
pub struct TraceStore {
    traces: DashMap<TraceId, StoredTrace>,
    spans: DashMap<SpanId, Span>,
    ttl: Duration,
    event_tx: broadcast::Sender<TraceEvent>,
}

struct StoredTrace {
    trace: Trace,
    created_at: SystemTime,
}

impl TraceStore {
    pub fn new(ttl: Duration) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1000);

        let store = Arc::new(Self {
            traces: DashMap::new(),
            spans: DashMap::new(),
            ttl,
            event_tx,
        });

        // Background task to clean up expired traces
        let store_weak = Arc::downgrade(&store);
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

        store
    }

    /// Ingest spans and build/update traces
    pub fn ingest(&self, spans: Vec<Span>) -> u32 {
        let count = spans.len() as u32;

        for span in spans {
            // Check if this is a new trace
            let is_new_trace = span.parent_span_id.is_none()
                && !self.spans.contains_key(&span.span_id);

            if is_new_trace {
                let _ = self.event_tx.send(TraceEvent::TraceStarted {
                    trace_id: span.trace_id,
                    root_span_name: span.name.clone(),
                    service_name: span.service_name.clone(),
                });
            }

            // Emit span added event
            let _ = self.event_tx.send(TraceEvent::SpanAdded {
                trace_id: span.trace_id,
                span: span.clone(),
            });

            self.spans.insert(span.span_id, span.clone());

            // Try to build/update trace
            self.update_trace(span.trace_id);
        }

        count
    }

    /// Get a complete trace by ID
    pub fn get_trace(&self, trace_id: TraceId) -> Option<Trace> {
        self.traces.get(&trace_id).map(|entry| entry.trace.clone())
    }

    /// List traces with filtering
    pub fn list_traces(&self, filter: TraceFilter) -> Vec<TraceSummary> {
        let mut summaries: Vec<TraceSummary> = self.traces
            .iter()
            .filter_map(|entry| {
                let trace = &entry.trace;

                // Apply filters
                if let Some(service) = &filter.service {
                    if !trace.spans.iter().any(|s| &s.service_name == service) {
                        return None;
                    }
                }

                let duration = trace.end_time.map(|e| e.0 - trace.start_time.0);

                if let Some(min_dur) = filter.min_duration_nanos {
                    if duration.map_or(true, |d| d < min_dur) {
                        return None;
                    }
                }

                if let Some(max_dur) = filter.max_duration_nanos {
                    if duration.map_or(false, |d| d > max_dur) {
                        return None;
                    }
                }

                let has_errors = trace.spans.iter().any(|s| matches!(s.status, SpanStatus::Error { .. }));

                if let Some(filter_errors) = filter.has_errors {
                    if has_errors != filter_errors {
                        return None;
                    }
                }

                let root_span = trace.spans.iter()
                    .find(|s| s.span_id == trace.root_span_id)?;

                Some(TraceSummary {
                    trace_id: trace.trace_id,
                    root_span_name: root_span.name.clone(),
                    service_name: root_span.service_name.clone(),
                    start_time: trace.start_time,
                    duration_nanos: duration,
                    span_count: trace.spans.len(),
                    has_errors,
                })
            })
            .collect();

        // Sort by start time (newest first)
        summaries.sort_by(|a, b| b.start_time.0.cmp(&a.start_time.0));

        // Apply limit
        let limit = filter.limit.unwrap_or(100);
        summaries.truncate(limit);

        summaries
    }

    /// Subscribe to live trace events
    pub fn subscribe_events(&self) -> broadcast::Receiver<TraceEvent> {
        self.event_tx.subscribe()
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

            // Check if trace is complete
            let is_complete = trace.end_time.is_some()
                && trace.spans.iter().all(|s| s.end_time.is_some());

            if is_complete {
                if let Some(duration) = trace.end_time.map(|e| e.0 - trace.start_time.0) {
                    let _ = self.event_tx.send(TraceEvent::TraceCompleted {
                        trace_id,
                        duration_nanos: duration,
                        span_count: trace.spans.len(),
                    });
                }
            }

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

### 4.3 Service Implementation (`src/service_impl.rs`)

```rust
use hindsight_protocol::*;
use rapace::Streaming;
use std::sync::Arc;
use futures::stream::Stream;
use std::pin::Pin;

use crate::storage::TraceStore;

pub struct HindsightServiceImpl {
    store: Arc<TraceStore>,
}

impl HindsightServiceImpl {
    pub fn new(store: Arc<TraceStore>) -> Self {
        Self { store }
    }
}

#[rapace::async_trait]
impl HindsightService for HindsightServiceImpl {
    async fn ingest_spans(&self, spans: Vec<Span>) -> u32 {
        // Filter out any spans from Hindsight itself (prevent infinite loop!)
        let spans: Vec<_> = spans.into_iter()
            .filter(|span| span.service_name != "hindsight-server")
            .collect();

        self.store.ingest(spans)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Option<Trace> {
        self.store.get_trace(trace_id)
    }

    async fn list_traces(&self, filter: TraceFilter) -> Vec<TraceSummary> {
        self.store.list_traces(filter)
    }

    async fn stream_traces(&self) -> Streaming<TraceEvent> {
        let mut rx = self.store.subscribe_events();

        let stream = async_stream::stream! {
            while let Ok(event) = rx.recv().await {
                yield Ok(event);
            }
        };

        Box::pin(stream)
    }

    async fn ping(&self) -> String {
        "pong".to_string()
    }
}
```

### 4.4 Main Server (`src/main.rs`)

```rust
mod storage;
mod service_impl;

use axum::{response::Html, routing::get, Router};
use clap::{Parser, Subcommand};
use hindsight_protocol::*;
use rapace::RpcSession;
use rapace_transport_tcp::TcpTransport;
use rapace_transport_websocket::WebSocketTransport;
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
    tokio::spawn(async move {
        if let Err(e) = serve_tcp(&host, port, service_tcp).await {
            tracing::error!("TCP server error: {}", e);
        }
    });

    // Spawn WebSocket server (for browser WASM clients)
    let service_ws = service.clone();
    tokio::spawn(async move {
        if let Err(e) = serve_websocket(&host, ws_port, service_ws).await {
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
            let transport = Arc::new(TcpTransport::from_stream(stream));

            // IMPORTANT: No tracer attached! (Prevents infinite loop)
            let session = Arc::new(RpcSession::new(transport));

            let server = HindsightServiceServer::new(service);
            session.set_dispatcher(server);

            if let Err(e) = session.run().await {
                tracing::error!("Session error: {}", e);
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

            let transport = Arc::new(WebSocketTransport::new(ws_stream));

            // IMPORTANT: No tracer attached! (Prevents infinite loop)
            let session = Arc::new(RpcSession::new(transport));

            let server = HindsightServiceServer::new(service);
            session.set_dispatcher(server);

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
```

---

## Phase 5: Web UI (WASM + Rapace)

**Goal:** Browser UI that uses Rapace WebSocket transport.

**Crate:** `hindsight-ui` (new WASM crate)

### 5.1 Create New Crate

```toml
# crates/hindsight-ui/Cargo.toml
[package]
name = "hindsight-ui"
version.workspace = true
edition.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
hindsight-protocol = { path = "../hindsight-protocol" }
rapace = { path = "../../../rapace/crates/rapace" }
rapace-transport-websocket = { path = "../../../rapace/crates/rapace-transport-websocket" }

wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["Window", "Document", "HtmlElement"] }
facet = { path = "../../../facet" }
```

### 5.2 WASM Entry Point (`src/lib.rs`)

```rust
use wasm_bindgen::prelude::*;
use hindsight_protocol::*;
use rapace::RpcSession;
use rapace_transport_websocket::WebSocketTransport;
use std::sync::Arc;

#[wasm_bindgen(start)]
pub async fn main() {
    // Connect to Hindsight via WebSocket
    let ws_url = format!("ws://{}/ws", web_sys::window().unwrap().location().host().unwrap());

    let transport = WebSocketTransport::connect(&ws_url).await
        .expect("Failed to connect to Hindsight");

    let session = Arc::new(RpcSession::new(Arc::new(transport)));
    let client = HindsightServiceClient::new(session);

    // Load initial traces
    let traces = client.list_traces(TraceFilter::default()).await
        .expect("Failed to list traces");

    render_traces(&traces);

    // Stream live updates
    let mut stream = client.stream_traces().await
        .expect("Failed to stream traces");

    while let Some(event) = stream.recv().await {
        handle_event(event);
    }
}

fn render_traces(traces: &[TraceSummary]) {
    // TODO: D3.js rendering (similar to Phase 4 from old plan)
}

fn handle_event(event: TraceEvent) {
    // TODO: Update UI based on live events
}
```

### 5.3 HTML Template (`src/ui/index.html` in hindsight-server)

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hindsight - Distributed Tracing</title>
    <script type="module">
        import init from '/hindsight_ui.js';
        init().then(() => {
            console.log('Hindsight UI loaded');
        });
    </script>
    <style>
        /* Styling from old plan */
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
                <span id="status">Connecting...</span>
            </div>
            <div id="trace-view"></div>
        </div>
    </div>
</body>
</html>
```

**Note:** Full WASM implementation is beyond the scope of this plan, but the architecture is established.

---

## Phase 6: TUI (Native Rapace)

**Goal:** Terminal UI using native Rapace TCP transport.

**Crate:** `hindsight-tui`

### 6.1 Main App (`src/app.rs`)

```rust
use crossterm::event::{self, Event, KeyCode};
use hindsight_protocol::*;
use rapace::RpcSession;
use rapace_transport_tcp::TcpTransport;
use ratatui::{/* ... */};
use std::sync::Arc;

pub struct App {
    client: HindsightServiceClient,
    traces: Vec<TraceSummary>,
    selected_index: usize,
    selected_trace: Option<Trace>,
}

impl App {
    pub async fn new(server_addr: String) -> anyhow::Result<Self> {
        // Connect via TCP (not HTTP!)
        let transport = TcpTransport::connect(&server_addr).await?;
        let session = Arc::new(RpcSession::new(Arc::new(transport)));

        // IMPORTANT: No tracer attached! (TUI doesn't trace itself)
        let client = HindsightServiceClient::new(session);

        Ok(Self {
            client,
            traces: Vec::new(),
            selected_index: 0,
            selected_trace: None,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Setup terminal
        // ... (same as old plan)

        // Fetch initial data using Rapace RPC!
        self.refresh().await?;

        // Event loop
        // ... (same as old plan)
    }

    async fn refresh(&mut self) -> anyhow::Result<()> {
        // Use Rapace RPC instead of HTTP!
        self.traces = self.client.list_traces(TraceFilter {
            limit: Some(50),
            ..Default::default()
        }).await?;

        if !self.traces.is_empty() {
            self.select_current_trace().await?;
        }

        Ok(())
    }

    async fn select_current_trace(&mut self) -> anyhow::Result<()> {
        if let Some(summary) = self.traces.get(self.selected_index) {
            // Use Rapace RPC instead of HTTP!
            self.selected_trace = self.client.get_trace(summary.trace_id).await?;
        }
        Ok(())
    }

    // ... (rest of TUI implementation from old plan)
}
```

---

## Phase 7: Integrations

### 7.1 Rapace Integration

**Location:** `rapace-core/src/tracing.rs` (new file, feature-gated)

```rust
#[cfg(feature = "hindsight")]
use hindsight::{Tracer, ActiveSpan};

pub struct TracingContext {
    tracer: Tracer,
    service_name: String,
}

impl<T: Transport> RpcSession<T> {
    /// Attach a Hindsight tracer to this session
    ///
    /// ⚠️ WARNING: Do NOT use this on Hindsight's own RPC sessions!
    /// This will cause infinite loops.
    ///
    /// Only use this on application-level sessions.
    #[cfg(feature = "hindsight")]
    pub fn with_hindsight_tracer(mut self, tracer: Tracer) -> Self {
        self.tracing_context = Some(TracingContext {
            tracer,
            service_name: std::env::var("SERVICE_NAME")
                .unwrap_or_else(|_| "rapace-app".to_string()),
        });
        self
    }

    // In call() method:
    pub async fn call(&self, channel_id: u32, method_id: u32, payload: Bytes)
        -> Result<ReceivedFrame, RpcError>
    {
        #[cfg(feature = "hindsight")]
        let _span = if let Some(ctx) = &self.tracing_context {
            let method_name = self.method_registry
                .lookup(method_id)
                .unwrap_or_else(|| format!("method_{}", method_id));

            Some(ctx.tracer.span(format!("RPC: {}", method_name))
                .with_attribute("channel_id", channel_id as i64)
                .with_attribute("method_id", method_id as i64)
                .start())
        } else {
            None
        };

        // ... existing call logic ...

        #[cfg(feature = "hindsight")]
        if let Some(span) = _span {
            if result.is_err() {
                span.set_error(format!("{:?}", result));
            }
            span.end();
        }

        result
    }
}
```

**Usage:**
```rust
// Application code (NOT Hindsight itself!)
let tracer = hindsight::Tracer::new(tcp_transport).await?;

let session = RpcSession::new(app_transport)
    .with_hindsight_tracer(tracer); // ✅ OK for application sessions

// ❌ DON'T do this in Hindsight server:
// let session = RpcSession::new(transport)
//     .with_hindsight_tracer(tracer); // ❌ INFINITE LOOP!
```

### 7.2 Picante Integration

**Location:** `picante/src/runtime.rs`

```rust
impl Runtime {
    #[cfg(feature = "hindsight")]
    pub fn with_hindsight_tracer(mut self, tracer: hindsight::Tracer) -> Self {
        self.hindsight_tracer = Some(tracer);
        self
    }
}

// In query execution:
#[cfg(feature = "hindsight")]
if let Some(tracer) = &db.runtime().hindsight_tracer {
    let _span = tracer.span(format!("Query: {}", query_kind_name))
        .with_attribute("cache_hit", cache_hit)
        .with_attribute("revision", revision as i64)
        .start();

    // ... execute query ...

    _span.end(); // Sends to Hindsight
}
```

**Usage:**
```rust
let tracer = hindsight::Tracer::new(tcp_transport).await?;

let runtime = Runtime::new()
    .with_hindsight_tracer(tracer); // ✅ Traces all query executions
```

---

## Testing Strategy

### Unit Tests
- `hindsight-protocol`: Trace context parsing, span serialization
- `hindsight`: Span builder API, batching logic
- `hindsight-server`: Storage, TTL cleanup, event broadcasting

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_rapace_trace() {
    // Start Hindsight server
    let store = TraceStore::new(Duration::from_secs(3600));
    let service = Arc::new(HindsightServiceImpl::new(store));

    // Create in-memory transport (for testing)
    let (client_transport, server_transport) = create_test_transport_pair();

    // Server side
    let session = Arc::new(RpcSession::new(server_transport));
    session.set_dispatcher(HindsightServiceServer::new(service.clone()));
    tokio::spawn(session.run());

    // Client side
    let tracer = Tracer::new(client_transport).await.unwrap();

    // Send a span
    let span = tracer.span("test_operation").start();
    span.end();

    // Wait for batching
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Query via Rapace RPC (not HTTP!)
    let client = HindsightServiceClient::new(session.clone());
    let traces = client.list_traces(TraceFilter::default()).await.unwrap();

    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].root_span_name, "test_operation");
}
```

### Manual Testing

1. **Start server:**
   ```bash
   cargo run -p hindsight-server -- serve
   ```

2. **Run TUI:**
   ```bash
   cargo run -p hindsight-tui -- --connect tcp://localhost:9090
   ```

3. **Run example app:**
   ```bash
   cargo run --example traced-app
   ```

4. **Open browser:**
   ```
   http://localhost:8080
   ```

---

## Future Enhancements

1. **Persistent Storage**
   - SQLite backend for long-term storage
   - Configurable via `--storage=sqlite://traces.db`

2. **Sampling**
   - Server-side sampling (% of traces)
   - Head-based and tail-based sampling

3. **Export Formats**
   - Jaeger format
   - Zipkin format
   - OpenTelemetry OTLP

4. **Advanced Filtering**
   - Query by span attributes
   - Full-text search in span names

5. **Alerting**
   - Threshold-based alerts (latency > X, error rate > Y)
   - Webhook notifications

6. **SHM Transport**
   - For ultra-low-latency same-machine tracing
   - Perfect for Picante → Hindsight on dev machines

---

## Avoiding Infinite Loops - Summary

### The Rule

**Hindsight's own RPC sessions MUST NOT have a tracer attached.**

### How to Ensure This

1. **In Hindsight server:** Never call `.with_hindsight_tracer()` on sessions
2. **In applications:** Only trace application-level sessions, not Hindsight client sessions
3. **Service filtering:** Filter out `HindsightService` from tracing
4. **Server-side filtering:** Hindsight rejects spans with `service_name == "hindsight-server"`

### Example (Correct)

```rust
// Application that uses both Hindsight (for tracing) AND Rapace (for business logic)

// Tracer for sending spans (uses one Rapace session internally)
let hindsight_transport = TcpTransport::connect("localhost:9090").await?;
let tracer = Tracer::new(hindsight_transport).await?; // ✅ No tracer on this!

// Application RPC session (traced)
let app_transport = TcpTransport::connect("localhost:5000").await?;
let app_session = RpcSession::new(app_transport)
    .with_hindsight_tracer(tracer); // ✅ Traces business logic RPCs

// Use app_session normally - all RPCs will be traced!
```

---

## File Checklist

- [x] `README.md`
- [x] `Cargo.toml` (workspace)
- [x] `.gitignore`
- [x] `LICENSE-MIT`, `LICENSE-APACHE`
- [x] `.github/workflows/ci.yml`
- [x] `crates/hindsight-protocol/` - Protocol, service definition
- [x] `crates/hindsight/` - Client library
- [x] `crates/hindsight-server/` - Server implementation
- [x] `crates/hindsight-tui/` - TUI client
- [ ] `crates/hindsight-ui/` - WASM web UI (to be created)
- [x] `PLAN.md`

---

## Getting Started (For New Contributors)

1. **Read the README** to understand the project vision
2. **Understand the "Avoiding Infinite Loops" section** - this is critical!
3. **Start with Phase 1** (protocol) - foundation for everything
4. **Phase 2** (Rapace service definition) - defines the RPC interface
5. **Phase 3** (client library) - test with simple examples
6. **Phase 4** (server) - get it serving via TCP and WebSocket
7. **Phase 5-6** (UI) - polish the visualization

The goal is a working MVP by the end of Phase 4, with UI in Phases 5-6.

**Key Principle:** Everything uses Rapace RPC. No HTTP APIs except serving the HTML page.

Happy hacking! 🚀
