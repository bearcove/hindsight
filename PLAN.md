# Hindsight Implementation Plan
## The Unified Observability Hub - Bidirectional RPC + Extensible Discovery

**Date**: 2025-12-15
**Unifies**:
- `docs/archive/PLAN_v1.md` (original core plan)
- `docs/archive/PLAN_v2_picante.md` (Picante integration plan)
**Key Insight**: Hindsight discovers app capabilities at runtime and adapts its UI dynamically

---

## Table of Contents

1. [Vision](#vision)
2. [Architecture](#architecture)
3. [Service Discovery Protocol](#service-discovery-protocol)
4. [Core Protocol](#core-protocol)
5. [Standard Services](#standard-services)
6. [App-Specific Services](#app-specific-services)
7. [Implementation Phases](#implementation-phases)
8. [UI Dynamic Adaptation](#ui-dynamic-adaptation)
9. [Examples](#examples)
10. [Notes from earlier drafts](#notes-from-earlier-drafts)

---

## Vision

### The Problem

Every bearcove tool needs observability:
- **Rapace** - Cell topology, RPC metrics, transport stats
- **Picante** - Query graphs, cache behavior, dependency tracking
- **Dodeca** - Build info, page generation, template rendering

**Bad approach**: Build separate debug UIs for each tool (3 WebUIs, 3 TUIs, maintenance nightmare)

**Good approach**: **One unified hub that discovers app capabilities and adapts**

### Hindsight is THE Hub

```
                    ┌────────────────────────────────┐
                    │   Hindsight (The Hub)          │
                    │                                │
                    │  • Receives traces             │
                    │  • Discovers app capabilities  │
                    │  • Adapts UI dynamically       │
                    │  • Pure Rapace RPC             │
                    └────────────┬───────────────────┘
                                 │
                    Bidirectional Rapace RPC
                    (Apps → Hindsight: push traces)
                    (Hindsight → Apps: pull app data)
                                 │
        ┌────────────────────────┼────────────────────────┐
        ↓                        ↓                        ↓
   ┌─────────┐            ┌──────────┐            ┌──────────┐
   │ Rapace  │            │ Picante  │            │  Dodeca  │
   │   App   │            │   App    │            │   App    │
   └─────────┘            └──────────┘            └──────────┘
        │                       │                       │
    Exposes:              Exposes:                Exposes:
    • TracingExporter     • TracingExporter       • TracingExporter
    • ServiceIntrospect.  • ServiceIntrospect.    • ServiceIntrospect.
    • RapaceIntrospect.   • PicanteIntrospect.    • DodecaIntrospect.
      (cell topology)       (query graphs)          (build info)
```

**Key Innovation**: Apps expose introspection services. Hindsight calls `ServiceIntrospection.list_services()` on connect to discover what the app supports, then adapts its UI!

---

## Architecture

### Bidirectional RPC

Unlike the original push-only draft, this plan uses **bidirectional Rapace**:

**Direction 1: Apps → Hindsight** (Push)
- Apps call `HindsightService.ingest_spans()` to send traces
- Standard span data with W3C trace context
- Includes method names directly (no separate lookup needed)

**Direction 2: Hindsight → Apps** (Pull)
- Hindsight calls `ServiceIntrospection.list_services()` to discover capabilities
- Hindsight calls app-specific services (e.g., `PicanteIntrospection.get_query_graph()`)
- Real-time data, not just historical traces

### Connection Flow

```
1. App connects to Hindsight via Rapace
   └─ TCP transport: HindsightClient::connect("localhost:1991")
   └─ Or HTTP Upgrade: connect to localhost:1990 and upgrade to Rapace

2. Hindsight accepts bidirectional session
   └─ Session has TWO roles simultaneously:
      • HindsightService (Hindsight side) - receives ingest_spans()
      • AppIntrospection (App side) - Hindsight can call app methods

3. Hindsight discovers app capabilities
   └─ Calls app.list_services()
   └─ Finds: ["ServiceIntrospection", "PicanteIntrospection", ...]

4. Hindsight UI adapts
   └─ Shows generic "Traces" tab (always)
   └─ Shows "Picante Query Graph" tab (if PicanteIntrospection found)
   └─ Shows "Rapace Topology" tab (if RapaceIntrospection found)

5. App sends traces
   └─ Calls hindsight.ingest_spans([...spans with method_name included])

6. User views trace
   └─ Hindsight calls app.get_picante_graph(trace_id) for details
   └─ Renders specialized view
```

### Transport Matrix

**Single Port Architecture**: Port **1990** handles everything via HTTP Upgrade!

| Client          | Connection Method           | Protocol After Upgrade |
|-----------------|-----------------------------|------------------------|
| Native apps     | HTTP Upgrade: rapace        | Raw Rapace TCP         |
| Browser (WASM)  | HTTP Upgrade: websocket     | WebSocket → Rapace RPC |
| Web UI          | HTTP GET /                  | HTML (no upgrade)      |
| Cells (plugins) | SHM (direct)                | Rapace (no HTTP)       |

**Port 1991** (optional): Raw TCP Rapace for clients that want to skip HTTP handshake

**The Upgrade Handshake** (native clients):
```
Client → Server:
  GET / HTTP/1.1
  Host: localhost:1990
  Upgrade: rapace
  Connection: Upgrade

Server → Client:
  HTTP/1.1 101 Switching Protocols
  Upgrade: rapace
  Connection: Upgrade

[Both sides switch to raw Rapace binary protocol]
```

**Client Implementation**: Just 30 lines - write text, read until `\r\n\r\n`, check for `101`, done!

**Why this works**:
- ✅ Single port for all HTTP-based connections
- ✅ Works through HTTP proxies
- ✅ WebSocket is already HTTP Upgrade
- ✅ Minimal overhead (one roundtrip)
- ✅ No heavy HTTP client dependency needed

---

## Service Discovery Protocol

### The Foundation (Built Today!)

We just implemented complete service discovery in rapace:

1. **Global Registry** - `ServiceRegistry::with_global()`
2. **Auto-Registration** - Services register on creation via `#[rapace::service]` macro
3. **ServiceIntrospection** RPC trait:
   ```rust
   #[rapace::service]
   trait ServiceIntrospection {
       async fn list_services(&self) -> Vec<ServiceInfo>;
       async fn describe_service(&self, name: String) -> Option<ServiceInfo>;
       async fn has_method(&self, method_id: u32) -> bool;
   }
   ```

### Types (Already Implemented)

```rust
// In rapace-registry/src/introspection.rs

#[derive(Facet)]
pub struct ServiceInfo {
    pub name: String,
    pub doc: String,
    pub methods: Vec<MethodInfo>,
}

#[derive(Facet)]
pub struct MethodInfo {
    pub id: u32,
    pub name: String,
    pub full_name: String,  // "Calculator.add"
    pub doc: String,
    pub args: Vec<ArgInfo>,
    pub is_streaming: bool,
}

#[derive(Facet)]
pub struct ArgInfo {
    pub name: String,
    pub type_name: String,
}
```

### Discovery Flow

```rust
// Hindsight discovers app capabilities on connect:

let services = app_session.call_service_introspection_list_services().await?;

for service in services {
    match service.name.as_str() {
        "PicanteIntrospection" => {
            println!("📊 Picante app detected! Enabling query graph view");
            ui_state.enable_picante_view();
        }
        "RapaceIntrospection" => {
            println!("🔌 Rapace cell detected! Enabling topology view");
            ui_state.enable_rapace_view();
        }
        "DodecaIntrospection" => {
            println!("📄 Dodeca app detected! Enabling build info view");
            ui_state.enable_dodeca_view();
        }
        _ => {
            // Generic service, no special handling
        }
    }
}
```

---

## Core Protocol

### Span Schema (Enhanced from PLAN.md)

```rust
// In hindsight-protocol/src/span.rs

#[derive(Clone, Debug, Facet)]
pub struct Span {
    pub trace_id: TraceId,           // W3C: 128-bit
    pub span_id: SpanId,             // W3C: 64-bit
    pub parent_span_id: Option<SpanId>,

    // NEW: Include method name directly (don't require lookup)
    pub name: String,                // "Query::parse_file" or "Calculator.add"
    pub method_id: Option<u32>,      // Optional, for correlation
    pub method_name: Option<String>, // Optional, "Calculator.add" (RECOMMENDED)

    pub service_name: String,        // "my-app", "dodeca-server"
    pub start_time_nanos: u64,
    pub end_time_nanos: Option<u64>,

    pub attributes: HashMap<String, AttributeValue>,
    pub events: Vec<SpanEvent>,
}

#[derive(Clone, Debug, Facet)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    // Future: Bytes, Array
}
```

**Key Decision**: Include `method_name` directly in span! No round-trip to app to resolve method_id → name.

**Rationale**:
- Works even if app disconnects after sending spans
- Zero latency for historical traces
- Standard pattern (OpenTelemetry does this with `rpc.method`)
- ServiceIntrospection still useful for **live** debugging, just not historical trace viewing

### HindsightService (Core)

```rust
// In hindsight-protocol/src/service.rs

#[rapace::service]
pub trait HindsightService {
    /// Ingest spans from an application.
    ///
    /// Apps batch spans and send periodically (e.g., every 5 seconds or 100 spans).
    async fn ingest_spans(&self, spans: Vec<Span>) -> IngestResult;

    /// Get a complete trace by ID.
    async fn get_trace(&self, trace_id: TraceId) -> Option<Trace>;

    /// List recent traces with optional filtering.
    async fn list_traces(&self, filter: TraceFilter) -> Vec<TraceSummary>;

    /// Stream trace updates in real-time.
    ///
    /// Returns a stream of TraceEvent (new trace, trace updated, etc.).
    async fn stream_traces(&self) -> Streaming<TraceEvent>;

    /// Health check (useful for monitoring Hindsight itself).
    async fn ping(&self) -> String;
}

#[derive(Facet)]
pub struct IngestResult {
    pub accepted: u32,
    pub rejected: u32,
    pub errors: Vec<String>,
}

#[derive(Facet)]
pub struct TraceSummary {
    pub trace_id: TraceId,
    pub root_span_name: String,
    pub service_name: String,
    pub start_time_nanos: u64,
    pub duration_nanos: u64,
    pub span_count: u32,
    pub error_count: u32,

    // NEW: Classification for UI rendering
    pub trace_type: TraceType,
}

#[derive(Facet)]
pub enum TraceType {
    Generic,
    Picante,
    Rapace,
    Mixed,
}

#[derive(Facet)]
pub struct TraceFilter {
    pub service_name: Option<String>,
    pub min_duration_nanos: Option<u64>,
    pub has_errors: Option<bool>,
    pub limit: u32,
}
```

---

## Standard Services

Every app SHOULD expose these (provided by rapace-introspection):

### 1. ServiceIntrospection (Already Built!)

```rust
#[rapace::service]
pub trait ServiceIntrospection {
    async fn list_services(&self) -> Vec<ServiceInfo>;
    async fn describe_service(&self, name: String) -> Option<ServiceInfo>;
    async fn has_method(&self, method_id: u32) -> bool;
}
```

**Implementation**: `rapace-introspection::DefaultServiceIntrospection` (reads from global registry)

**Usage in apps**:
```rust
use rapace_cell::run_multi;

run_multi(|builder| {
    builder
        .add_service(MyServiceServer::new(impl))
        .with_introspection()  // ← One line!
}).await?;
```

---

## App-Specific Services

### 1. PicanteIntrospection (from the Picante integration draft)

```rust
// In picante/crates/picante-introspection/src/lib.rs

#[rapace::service]
pub trait PicanteIntrospection {
    /// Get the dependency graph for a specific trace.
    ///
    /// Returns None if trace not found or not a Picante trace.
    async fn get_query_graph(&self, trace_id: TraceId) -> Option<PicanteGraph>;

    /// Get currently executing queries (live view).
    async fn get_active_queries(&self) -> Vec<ActiveQuery>;

    /// Get cache statistics.
    async fn get_cache_stats(&self) -> CacheStats;

    /// Stream query execution events in real-time.
    async fn stream_query_events(&self) -> Streaming<QueryEvent>;
}

#[derive(Facet)]
pub struct PicanteGraph {
    pub nodes: Vec<QueryNode>,
    pub edges: Vec<QueryEdge>,
}

#[derive(Facet)]
pub struct QueryNode {
    pub span_id: SpanId,
    pub query_kind: String,      // "parse_file", "file_text"
    pub query_key: String,        // "src/main.rs"
    pub cache_status: CacheStatus,  // Hit, Miss, Validated
    pub duration_nanos: u64,
    pub revision: Option<u64>,
}

#[derive(Facet)]
pub enum CacheStatus {
    Hit,        // Retrieved from memo table (blue in UI)
    Miss,       // Recomputed (yellow in UI)
    Validated,  // Early cutoff verified unchanged (green in UI)
}

#[derive(Facet)]
pub struct QueryEdge {
    pub from: SpanId,  // dependency
    pub to: SpanId,    // dependent
}

#[derive(Facet)]
pub struct ActiveQuery {
    pub query_kind: String,
    pub query_key: String,
    pub started_nanos: u64,
    pub state: QueryState,
}

#[derive(Facet)]
pub enum QueryState {
    CheckingCache,
    Computing,
    WaitingOnDependency(String),
}

#[derive(Facet)]
pub struct CacheStats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_validations: u64,
    pub cache_hit_rate: f64,
    pub by_query_kind: HashMap<String, QueryKindStats>,
}

#[derive(Facet)]
pub struct QueryKindStats {
    pub query_kind: String,
    pub total_executions: u64,
    pub cache_hits: u64,
    pub avg_duration_nanos: u64,
    pub max_duration_nanos: u64,
}
```

**Picante Span Attributes** (from the Picante integration draft):
```rust
// When emitting spans, Picante includes:
attributes: {
    "picante.query": true,
    "picante.query_kind": "parse_file",
    "picante.query_key": "src/main.rs",
    "picante.cache_status": "hit" | "miss" | "validated",
    "picante.revision": 42,
    "picante.dependency_count": 3,
}
```

### 2. RapaceIntrospection (from original debug UI plan)

```rust
// In rapace/crates/rapace-introspection-ext/src/lib.rs

#[rapace::service]
pub trait RapaceIntrospection {
    /// Get cell topology (sessions, channels, peers).
    async fn get_topology(&self) -> Topology;

    /// Get transport metrics (frame counts, throughput, SHM stats).
    async fn get_transport_metrics(&self) -> Vec<TransportMetrics>;

    /// Get active RPC calls.
    async fn get_active_rpcs(&self) -> Vec<ActiveRpc>;

    /// Stream RPC events in real-time.
    async fn stream_rpc_events(&self) -> Streaming<RpcEvent>;
}

#[derive(Facet)]
pub struct Topology {
    pub sessions: Vec<SessionInfo>,
    pub connections: Vec<Connection>,
}

#[derive(Facet)]
pub struct SessionInfo {
    pub session_id: u64,
    pub label: String,
    pub session_type: SessionType,  // Host or Cell
    pub transport_type: String,     // "SHM", "TCP", "WebSocket"
    pub created_at: u64,
}

#[derive(Facet)]
pub enum SessionType {
    Host { channel_ids: String },  // "1, 3, 5, ..."
    Cell { channel_ids: String },  // "2, 4, 6, ..."
}

#[derive(Facet)]
pub struct Connection {
    pub from_session_id: u64,
    pub to_session_id: u64,
    pub active_channels: u32,
}

#[derive(Facet)]
pub struct TransportMetrics {
    pub session_id: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub inline_payload_count: u64,
    pub external_payload_count: u64,
    pub shm_metrics: Option<ShmMetrics>,
}

#[derive(Facet)]
pub struct ShmMetrics {
    pub zero_copy_count: u64,
    pub slot_allocations: u64,
    pub slot_usage_pct: f64,
}

#[derive(Facet)]
pub struct ActiveRpc {
    pub channel_id: u32,
    pub method_name: String,
    pub started_nanos: u64,
    pub state: RpcState,
}

#[derive(Facet)]
pub enum RpcState {
    Pending,
    Dispatched,
    WaitingForResponse,
}
```

### 3. DodecaIntrospection

```rust
// In dodeca/crates/dodeca-introspection/src/lib.rs

#[rapace::service]
pub trait DodecaIntrospection {
    /// Get current build information.
    async fn get_build_info(&self) -> BuildInfo;

    /// List all pages in the site.
    async fn list_pages(&self) -> Vec<PageInfo>;

    /// Get template rendering stats.
    async fn get_template_stats(&self) -> TemplateStats;

    /// Stream build events (page compiled, asset generated, etc.).
    async fn stream_build_events(&self) -> Streaming<BuildEvent>;
}

#[derive(Facet)]
pub struct BuildInfo {
    pub site_name: String,
    pub output_dir: String,
    pub page_count: u32,
    pub asset_count: u32,
    pub last_build_duration_nanos: u64,
    pub build_state: BuildState,
}

#[derive(Facet)]
pub enum BuildState {
    Idle,
    Building { progress_pct: f64 },
    Error { message: String },
}

#[derive(Facet)]
pub struct PageInfo {
    pub path: String,
    pub template: String,
    pub last_rendered_nanos: u64,
    pub render_duration_nanos: u64,
}

#[derive(Facet)]
pub struct TemplateStats {
    pub by_template: HashMap<String, TemplateKindStats>,
}

#[derive(Facet)]
pub struct TemplateKindStats {
    pub template_name: String,
    pub render_count: u64,
    pub avg_duration_nanos: u64,
}
```

---

## Implementation Phases

### Phase 1: Core Hindsight ✅ **COMPLETE**

**Goal**: Basic trace ingestion and viewing

1.1. **Protocol & Types** (`hindsight-protocol`) ✅
   - `Span`, `Trace`, `TraceId`, `SpanId` (W3C compliant)
   - `HindsightService` trait
   - Include `method_name` in Span
   - `TraceType` enum (Generic, Picante, Rapace, Dodeca, Mixed)

1.2. **Server** (`hindsight-server`) ✅
   - HTTP server on port **1990** (serves HTML, handles WebSocket upgrade)
   - TCP listener on port **1991** (raw Rapace, optional)
   - In-memory storage (`DashMap<TraceId, Trace>`)
   - TTL-based expiration (background cleanup every 60s)
   - `HindsightServiceImpl` (untraced sessions!)
   - Live event streaming via broadcast channel

1.3. **Client Library** (`hindsight`) ✅
   - `Tracer::new(transport)` → creates bidirectional session
   - Batch spans (100 spans or 100ms intervals)
   - Automatic W3C trace context propagation
   - `SpanBuilder` fluent API with `.with_attribute()`, `.with_parent()`
   - `ActiveSpan` for in-flight operations

**What's Built**:
- `examples/simple_client.rs` - Sends test spans
- `examples/query_traces.rs` - Queries stored traces
- `examples/test_classification.rs` - Tests all 5 trace types

**Status**: End-to-end verified with 10 traces successfully ingested and queried!

### Phase 2: Trace Classification ✅ **COMPLETE**

**Goal**: Automatic trace type detection based on span attributes

2.1. **TraceType Classification** (`hindsight-protocol/src/span.rs`) ✅
   - `TraceType` enum: Generic, Picante, Rapace, Dodeca, Mixed
   - `Trace::classify_type()` method that detects framework attributes:
     - **Picante**: `picante.query` attribute
     - **Rapace**: `rpc.system="rapace"` attribute
     - **Dodeca**: `dodeca.build` attribute
     - **Mixed**: Multiple framework types in one trace

2.2. **Defensive Trace Building** ✅
   - Changed `Trace::from_spans()` to return `Option<Trace>`
   - Handles out-of-order span arrival (child before parent)
   - Gracefully skips trace building when root span not yet received

2.3. **Storage Integration** ✅
   - `TraceSummary` includes `trace_type` field
   - Automatic classification in `list_traces()`
   - Storage handles Optional trace building

**What's Built**:
- `examples/test_classification.rs` - Sends all 5 trace types
- Classification verified with mixed parent-child traces
- Out-of-order span arrival handled correctly

**Status**: All 5 trace types successfully classified and verified!

**Note**: Full bidirectional service discovery (calling `ServiceIntrospection.list_services()` on app connect) is deferred to when we need dynamic UI adaptation in Phase 3+.

### Phase 2.5: HTTP Upgrade for Rapace (Planned)

**Goal**: Single-port architecture with HTTP Upgrade

**Server** (`hindsight-server/src/main.rs`):
- Port 1990 Axum server handles:
  - `GET /` → serve HTML (web UI)
  - `Upgrade: websocket` → WebSocket for WASM
  - `Upgrade: rapace` → raw Rapace TCP for native clients
- Port 1991 (optional): Direct TCP Rapace (skip HTTP handshake)

**Client** (`hindsight/src/tracer.rs`):
- Add `Tracer::connect_http(addr)` method
- Writes HTTP upgrade request (~30 lines):
  ```rust
  stream.write_all(b"GET / HTTP/1.1\r\n").await?;
  stream.write_all(b"Host: ...\r\n").await?;
  stream.write_all(b"Upgrade: rapace\r\n").await?;
  stream.write_all(b"Connection: Upgrade\r\n\r\n").await?;

  // Read until \r\n\r\n, check for "101", done!
  ```
- No heavy HTTP client dependency needed!

**Benefits**:
- ✅ Single port for everything
- ✅ Works through HTTP proxies
- ✅ Minimal overhead (one roundtrip)
- ✅ Client is trivial (no hyper dependency)

**Status**: Architecture designed, implementation deferred

### Phase 3: Generic Web UI

**Goal**: View traces in browser

3.1. **WASM Client** (`hindsight-wasm`)
   - rapace-transport-websocket
   - `HindsightServiceClient::new(ws_session)`
   - Call `list_traces()`, `get_trace()`

3.2. **UI Views**
   - Trace list (table)
   - Trace detail (waterfall/flamegraph)
   - Filter by service, duration, errors

**Test**: Open browser, see traces

**Time Estimate**: 3-4 days

### Phase 4: Picante Integration (from the Picante integration draft)

**Goal**: Specialized Picante visualization

4.1. **Picante emits spans** (in Picante repo)
   - Instrument `Runtime` to emit spans
   - Include picante.* attributes
   - Send to Hindsight via `ingest_spans()`

4.2. **Hindsight detects Picante** (`hindsight-protocol/picante.rs`)
   - `PicanteGraph`, `PicanteNode`, `CacheStatus` types
   - `build_picante_graph(trace)` function
   - Detect picante.* attributes

4.3. **Hindsight calls back to Picante**
   - When viewing Picante trace, call `app.get_query_graph(trace_id)`
   - Get live cache stats with `app.get_cache_stats()`

4.4. **UI: Picante Trace View**
   - D3.js dependency graph
   - Color nodes by cache status (blue=hit, yellow=miss, green=validated)
   - Show cache hit rate, query timeline

4.5. **UI: Picante Stats Dashboard**
   - Global cache hit rate
   - Per-query-kind statistics table
   - Trends over time

**Test**: Run Picante app, see dependency graph with colored nodes

**Time Estimate**: 4-5 days

### Phase 5: Rapace Integration

**Goal**: Cell topology and RPC metrics

5.1. **Rapace exposes introspection** (in Rapace repo)
   - Implement `RapaceIntrospection` service
   - Collect topology, metrics, active RPCs
   - Opt-in via feature flag

5.2. **Hindsight detects Rapace**
   - Call `app.get_topology()`, `app.get_transport_metrics()`
   - Store in `RapaceState` per connection

5.3. **UI: Rapace Topology View**
   - Network graph (vis.js)
   - Nodes = sessions, edges = connections
   - Live RPC calls (flash animation)

5.4. **UI: Rapace Metrics Dashboard**
   - Transport stats (frames/sec, throughput)
   - SHM metrics (zero-copy efficiency)
   - Active RPC list

**Test**: Run multi-cell app, see topology graph

**Time Estimate**: 3-4 days

### Phase 6: Dodeca Integration

**Goal**: Build info and page stats

6.1. **Dodeca exposes introspection** (in Dodeca repo)
   - Implement `DodecaIntrospection` service
   - Track build state, pages, templates

6.2. **Hindsight detects Dodeca**
   - Call `app.get_build_info()`, `app.list_pages()`

6.3. **UI: Dodeca Build View**
   - Build status, progress bar
   - Page list with render times
   - Template statistics

**Test**: Run Dodeca build, watch progress in Hindsight

**Time Estimate**: 2-3 days

### Phase 7: TUI (from PLAN.md)

**Goal**: Terminal UI with ratatui

7.1. **TUI Client** (`hindsight-tui`)
   - rapace-transport-tcp
   - Views: trace list, trace detail, stats

7.2. **TUI Dynamic Views**
   - Show Picante/Rapace/Dodeca tabs based on discovery
   - ASCII dependency graph for Picante
   - Table views for stats

**Test**: `hindsight-tui --connect localhost:9090`

**Time Estimate**: 3-4 days

### Phase 8: Streaming & Real-Time

**Goal**: Live updates

8.1. **Server streaming**
   - `stream_traces()` → Streaming<TraceEvent>
   - `stream_query_events()` (Picante)
   - `stream_rpc_events()` (Rapace)
   - `stream_build_events()` (Dodeca)

8.2. **UI animations**
   - Flash nodes as queries execute (Picante)
   - Flash edges as RPCs sent (Rapace)
   - Update build progress bar (Dodeca)

**Test**: Watch live execution with animations

**Time Estimate**: 2-3 days

---

## UI Dynamic Adaptation

### How Hindsight UI Adapts

```rust
// Pseudo-code for UI adaptation logic

struct HindsightUI {
    tabs: Vec<Tab>,
}

impl HindsightUI {
    async fn on_app_connected(&mut self, app: &AppSession) {
        // Always show generic views
        self.tabs.push(Tab::Traces);
        self.tabs.push(Tab::Services);  // Show ServiceIntrospection results

        // Discover app capabilities
        let services = app.list_services().await?;

        for service in services {
            match service.name.as_str() {
                "PicanteIntrospection" => {
                    self.tabs.push(Tab::PicanteGraph);
                    self.tabs.push(Tab::PicanteStats);
                    self.enable_picante_rendering();
                }
                "RapaceIntrospection" => {
                    self.tabs.push(Tab::RapaceTopology);
                    self.tabs.push(Tab::RapaceMetrics);
                    self.enable_rapace_rendering();
                }
                "DodecaIntrospection" => {
                    self.tabs.push(Tab::DodecaBuild);
                    self.tabs.push(Tab::DodecaPages);
                    self.enable_dodeca_rendering();
                }
                _ => {
                    // Unknown service, no special handling
                }
            }
        }
    }

    async fn render_trace(&self, trace: &Trace, app: &AppSession) {
        match trace.trace_type {
            TraceType::Generic => {
                self.render_waterfall(trace);
            }
            TraceType::Picante => {
                // Call back to app for detailed graph
                let graph = app.get_picante_graph(trace.id).await?;
                self.render_picante_graph(graph);
            }
            TraceType::Rapace => {
                // Show RPC call chain with transport details
                let rpcs = extract_rapace_spans(trace);
                self.render_rpc_chain(rpcs);
            }
            TraceType::Mixed => {
                // Show all views with tabs
                self.render_multi_view(trace, app);
            }
        }
    }
}
```

### Example UI States

**Scenario 1: Generic Rust App**
```
Connected to: my-rust-app
Discovered services:
  • ServiceIntrospection

UI Tabs:
  [Traces] [Services]
```

**Scenario 2: Picante App**
```
Connected to: picante-app
Discovered services:
  • ServiceIntrospection
  • PicanteIntrospection

UI Tabs:
  [Traces] [Services] [Picante Graph] [Picante Stats]
```

**Scenario 3: Multi-Cell Rapace App**
```
Connected to: my-app-host
Discovered services:
  • ServiceIntrospection
  • RapaceIntrospection

UI Tabs:
  [Traces] [Services] [Rapace Topology] [Rapace Metrics]
```

**Scenario 4: Full Stack (Rapace + Picante + Dodeca)**
```
Connected to: dodeca-server
Discovered services:
  • ServiceIntrospection
  • PicanteIntrospection
  • RapaceIntrospection
  • DodecaIntrospection

UI Tabs:
  [Traces] [Services] [Picante Graph] [Rapace Topology] [Dodeca Build]
```

---

## Examples

### Example 1: Simple Rapace RPC

**App code**:
```rust
use hindsight::Tracer;
use rapace_cell::run_multi;

#[tokio::main]
async fn main() {
    // Connect to Hindsight
    let tracer = Tracer::connect("localhost:9090").await?;

    // Run cell with introspection
    run_multi(|builder| {
        builder
            .add_service(CalculatorServer::new(calc_impl))
            .with_introspection()  // ← Exposes ServiceIntrospection
    })
    .with_tracer(tracer)  // ← Send spans to Hindsight
    .await?;
}
```

**What Hindsight sees**:
```
1. App connects via TCP
2. Hindsight calls app.list_services()
   → Returns: ["ServiceIntrospection", "Calculator"]
3. Hindsight UI shows:
   - Traces tab (generic view)
   - Services tab (lists "Calculator" with methods)
```

**When RPC called**:
```rust
// App emits span:
Span {
    name: "Calculator.add",
    method_name: Some("Calculator.add"),
    method_id: Some(12345),
    service_name: "my-app",
    attributes: {
        "rpc.system": "rapace",
        "rpc.service": "Calculator",
        "rpc.method": "add",
    }
}
```

### Example 2: Picante Query

**App code**:
```rust
use hindsight::Tracer;
use picante::Runtime;

#[tokio::main]
async fn main() {
    let tracer = Tracer::connect("localhost:9090").await?;
    let runtime = Runtime::new()
        .with_hindsight_tracer(tracer)
        .with_introspection();  // ← Exposes PicanteIntrospection

    // Run query
    let result = runtime.query(parse_file("main.rs"));
}
```

**What Hindsight sees**:
```
1. App connects
2. Hindsight calls app.list_services()
   → Returns: ["ServiceIntrospection", "PicanteIntrospection"]
3. Hindsight UI shows:
   - Traces tab
   - Services tab
   - Picante Graph tab  ← NEW!
   - Picante Stats tab  ← NEW!

4. App sends spans with picante.* attributes
5. User clicks trace in Hindsight
6. Hindsight calls app.get_picante_graph(trace_id)
7. Renders dependency graph with colored nodes
```

### Example 3: Mixed Trace (Picante → Rapace → Dodeca)

**Scenario**: Dodeca (static site generator) uses Picante for incremental compilation, makes RPC to external markdown service

**Spans emitted**:
```rust
// Span 1: Picante query
Span {
    trace_id: trace_abc,
    span_id: span_001,
    name: "Query::render_markdown",
    method_name: Some("Query::render_markdown"),
    service_name: "dodeca",
    attributes: {
        "picante.query": true,
        "picante.query_kind": "render_markdown",
        "picante.cache_status": "miss",
    }
}

// Span 2: Rapace RPC (child of Picante query)
Span {
    trace_id: trace_abc,
    span_id: span_002,
    parent_span_id: Some(span_001),
    name: "MarkdownService.parse",
    method_name: Some("MarkdownService.parse"),
    service_name: "dodeca",
    attributes: {
        "rpc.system": "rapace",
        "rpc.service": "MarkdownService",
    }
}

// Span 3: Markdown parsing (child of RPC)
Span {
    trace_id: trace_abc,
    span_id: span_003,
    parent_span_id: Some(span_002),
    name: "parse_markdown_ast",
    service_name: "markdown-service",
}
```

**Hindsight shows**:
```
Trace: render_markdown (Mixed)

Generic View:
  render_markdown
    ├─ MarkdownService.parse (RPC)
    │   └─ parse_markdown_ast
    └─ (other work)

Picante View:
  [🟡 Query: render_markdown]  ← Yellow = cache miss
      ↓
  (shows only Picante nodes)

Rapace View:
  (shows RPC call with transport metrics)
```

---

## Notes from earlier drafts

### What changed in the unified plan

**Added**:
1. ✅ **Bidirectional RPC** - Apps expose services, Hindsight can call them
2. ✅ **Service Discovery** - Hindsight discovers capabilities via `ServiceIntrospection`
3. ✅ **Dynamic UI** - UI adapts based on discovered services
4. ✅ **method_name in Span** - No separate lookup needed for historical traces
5. ✅ **App-specific services** - PicanteIntrospection, RapaceIntrospection, DodecaIntrospection
6. ✅ **Real-time data** - Not just historical traces, also live app state

**Kept from the original core plan**:
- ✅ Pure Rapace architecture (no REST APIs)
- ✅ W3C Trace Context
- ✅ Multiple transports (TCP, WebSocket, SHM)
- ✅ Untraced sessions (no infinite loops)
- ✅ In-memory storage (ephemeral by default)
- ✅ WASM browser client + TUI

**Incorporated from the Picante integration plan**:
- ✅ Picante span schema (picante.* attributes)
- ✅ PicanteGraph types and visualization
- ✅ Cache status tracking
- ✅ Dependency graph rendering
- ✅ Statistics dashboard

**Enhanced**:
- Service discovery enables extensibility (any framework can add introspection)
- UI automatically adapts without code changes
- Apps can expose custom views (Picante query graph, Rapace topology, etc.)
- Historical traces are self-contained (include method names)
- Live introspection for current state (active queries, build progress, etc.)

### Implementation Order Change

The original core plan suggested: Protocol → Server → Client → Web UI → TUI → Integrations

This plan suggests:
1. Core (Phase 1) ← Same
2. **Service Discovery** (Phase 2) ← NEW, do early!
3. Generic Web UI (Phase 3) ← Same
4. **Picante** (Phase 4) ← Higher priority
5. **Rapace** (Phase 5) ← Higher priority
6. **Dodeca** (Phase 6) ← Higher priority
7. TUI (Phase 7) ← Same timing
8. Streaming (Phase 8) ← Same

**Rationale**: Service discovery is the foundation for extensibility. Do it early, then integrate frameworks in parallel.

---

## Avoiding Infinite Loops

**Still applies!** Hindsight's RPC sessions MUST be untraced:

```rust
// In hindsight-server
let session = RpcSession::new(transport);
// ❌ NO .with_tracer() call!

session.set_dispatcher(HindsightServiceServer::new(service));
```

**Additional consideration**: When Hindsight calls back to apps (e.g., `app.get_picante_graph()`), those calls should also be untraced to avoid clutter.

---

## Testing Strategy

### Unit Tests
- `hindsight-protocol`: Serialize/deserialize all types
- `hindsight-server`: Storage, classification, discovery logic
- `rapace-introspection`: DefaultServiceIntrospection

### Integration Tests
- App connects → Hindsight discovers services → correct UI state
- Send spans → classify trace type → retrieve via get_trace()
- Picante app → get_query_graph() → valid graph structure
- Mixed trace → correct parent/child relationships

### End-to-End Tests
1. **Generic app**: Send spans, view in browser, see generic waterfall
2. **Picante app**: Send query spans, see dependency graph with colors
3. **Multi-cell app**: See Rapace topology with live RPC calls
4. **Full stack**: Dodeca + Picante + Rapace, all views visible

### Manual Testing
- Connect with TUI: `hindsight-tui --connect localhost:1991` (raw TCP)
- Connect with browser: http://localhost:1990 (HTTP + WebSocket)
- Run demo apps (one per framework)
- Verify UI tabs appear/disappear correctly

### Current Testing (Phase 1-2 Complete)
✅ **End-to-end verified**:
- `cargo run -p hindsight-server -- serve` (ports 1990-1991)
- `cargo run -p hindsight --example simple_client` (10 spans)
- `cargo run -p hindsight --example test_classification` (5 trace types)
- `cargo run -p hindsight --example query_traces` (verify classification)

---

## Future Enhancements (from PLAN.md + new ideas)

### Persistence
- SQLite storage (optional)
- Query historical traces beyond current session
- Indexing by service, time range, trace type

### Sampling
- Only send 1% of traces
- Configurable sampling rate per service

### Export
- Export trace as JSON
- Export Picante graph as DOT/SVG
- Export statistics as CSV

### Alerting
- "Cache hit rate below 50%" → notify
- "RPC call taking > 1s" → alert
- Custom rules via config file

### Multi-Hindsight
- Multiple Hindsight instances
- Forward traces to central collector
- Distributed topology view

### Time-Travel Debugging
- Record all query executions
- "Replay" trace with different inputs
- Compare "before" vs "after" dependency graphs

---

## Summary

**Hindsight v3** is the **unified observability hub** for all bearcove tools:

1. **Pure Rapace** - Dogfooding, efficient binary protocol
2. **Bidirectional** - Apps push traces, Hindsight pulls app data
3. **Extensible** - Service discovery enables framework-specific views
4. **Self-Contained Traces** - Include method names, work offline
5. **Live Introspection** - Not just historical traces, current state too
6. **Dynamic UI** - Adapts automatically based on discovered capabilities

**One UI, many frameworks, zero config.**

---

**Next Steps**:
1. ✅ ~~Finalize this plan (review + approval)~~
2. ✅ ~~Implement Phase 1 (core Hindsight)~~
3. ✅ ~~Implement Phase 2 (trace classification)~~
4. 🚧 **NEXT**: Implement Phase 2.5 (HTTP Upgrade for Rapace) - optional
5. 🚧 **NEXT**: Build generic Web UI (Phase 3) - highest value now!
6. ⏳ Integrate Picante (Phase 4) - query graphs
7. ⏳ Integrate Rapace (Phase 5) - topology
8. ⏳ Integrate Dodeca (Phase 6) - build progress
9. ⏳ Polish TUI (Phase 7)
10. ⏳ Add streaming (Phase 8)

**Estimated Remaining Time**: 18-28 days (2 phases complete!)

---

**Authors**:
- Claude (Sonnet 4.5) - Plan synthesis & implementation
- Bearcove team - Architecture decisions

**Status**: ✅ **Phases 1-2 Complete** - Core functionality working!

**Current State**:
- ✅ Protocol & types (W3C trace context, spans, traces)
- ✅ Server (3 transports: HTTP, WebSocket, TCP on ports 1990-1991)
- ✅ Client library (tracer, span builder, batching)
- ✅ Trace classification (5 types: Generic, Picante, Rapace, Dodeca, Mixed)
- ✅ In-memory storage with TTL
- ✅ Live event streaming
- ✅ Out-of-order span handling
- ✅ End-to-end verified with examples

**Dependencies**:
- ✅ Service discovery (rapace-registry, rapace-introspection)
- ✅ Hindsight core implementation (Phases 1-2)
- ⏳ Framework introspection implementations (Picante, Dodeca, Rapace extensions)
- ⏳ Web UI (WASM + visualization libs)
