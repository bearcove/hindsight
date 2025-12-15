# Hindsight Unified Plan
## The Unified Observability Hub (Pure Rapace, Bidirectional Introspection)

**Date**: 2025-12-15

This document unifies the earlier drafts into a single canonical plan:
- `PLAN.md` (current working plan in this repo)
- `docs/archive/PLAN_v1.md` (archived original core plan)
- `docs/archive/PLAN_v2_picante.md` (archived Picante integration plan)

---

## Vision

Every bearcove tool needs observability:
- **Rapace**: cell topology, transport stats, RPC metrics
- **Picante**: incremental query graphs, cache behavior, dependency tracking
- **Dodeca**: build info, page generation, template rendering

The goal is **one** observability hub that:
1. Receives spans/traces from apps.
2. Discovers app capabilities at runtime.
3. Adapts UI dynamically based on discovered services.
4. Uses **pure Rapace RPC** end-to-end (HTTP only for static UI bootstrapping).

---

## Architecture

### Pure Rapace (one protocol everywhere)

- Apps talk to Hindsight via Rapace RPC (TCP/SHM/Unix/WebSocket transport variants).
- Browser UI uses Rapace-over-WebSocket.
- No REST API for trace data.
- Minimal HTTP exists only to serve the static page/WASM bundle.

### Bidirectional RPC

There are two directions:

**1) Apps → Hindsight (Push)**
- Apps call `HindsightService.ingest_spans()` to send spans (batched).

**2) Hindsight → Apps (Pull)**
- On connect, Hindsight calls `ServiceIntrospection.list_services()` on the app.
- If the app exposes framework-specific introspection (Picante/Rapace/Dodeca), Hindsight enables specialized views and can fetch **live** state.

### Connection flow (high-level)

1. App connects to Hindsight (Rapace session).
2. Hindsight discovers services via `ServiceIntrospection`.
3. App pushes spans via `HindsightService.ingest_spans()`.
4. UI renders generic traces always; specialized tabs appear when app capabilities exist.

---

## Avoiding Infinite Loops (self-tracing)

If Hindsight traces itself, it will recurse.

Rules:
- **Hindsight’s own Rapace sessions MUST be untraced.**
- Calls that Hindsight makes *back into apps* for introspection should also be untraced (to avoid clutter and accidental recursion).
- Tracing in apps is **opt-in**: a Rapace session is not traced unless a tracer is explicitly attached.

---

## Core Protocol

### Span schema (W3C trace context + app metadata)

Core requirements:
- W3C-compatible identifiers: `TraceId` (128-bit), `SpanId` (64-bit).
- `parent_span_id` for hierarchy.
- `service_name` for grouping/filtering.
- timestamps/duration.
- `attributes` and `events` for extensibility.

Important decision:
- Include a human-readable method name directly in spans (e.g. `"Calculator.add"` / `"Query::parse_file"`) so historical traces remain usable even if the app disconnects.

### HindsightService (core)

Core RPC surface:
- `ingest_spans(spans)` (batch ingest)
- `get_trace(trace_id)`
- `list_traces(filter)`
- `stream_traces()` (live events)
- `ping()`

Optional extensions:
- classification in summaries (Generic/Picante/Rapace/Mixed).

---

## Service Discovery Protocol

Hindsight relies on a standard introspection service exposed by apps:

### ServiceIntrospection

RPCs:
- `list_services() -> Vec<ServiceInfo>`
- `describe_service(name) -> Option<ServiceInfo>`
- `has_method(method_id) -> bool`

This is the foundation for dynamic UI adaptation and framework-specific features.

---

## Standard Services (recommended for all apps)

Every app SHOULD expose:
- `ServiceIntrospection` (so Hindsight can discover capabilities)

Every app MAY expose:
- framework-specific introspection (below)

---

## App-Specific Services

### PicanteIntrospection

Goals:
- show dependency graphs, cache behavior (hit/miss/validated), query stats
- enable both historical trace overlays and live runtime views

Expected span attributes (schema contract emitted by Picante):
- `picante.query: true`
- `picante.query_kind: <string>`
- `picante.query_key: <string>`
- `picante.cache_status: "hit" | "miss" | "validated"`
- optional: `picante.revision`, `picante.dependency_count`, etc.

Introspection RPCs (example set):
- `get_query_graph(trace_id) -> Option<PicanteGraph>`
- `get_active_queries()`
- `get_cache_stats()`
- streaming query events (optional)

### RapaceIntrospection

Goals:
- visualize cell topology and live RPC activity
- show transport metrics, SHM stats, active calls

Introspection RPCs (example set):
- `get_topology()`
- `get_transport_metrics()`
- `get_active_rpcs()`
- streaming RPC events (optional)

### DodecaIntrospection

Goals:
- build info, pages list, template stats
- build events streaming (optional)

---

## UI Strategy

### Dynamic adaptation

UI tabs appear/disappear based on discovered services:
- Always: Traces (generic), Services (introspection browser)
- If Picante: Picante Graph, Picante Stats
- If Rapace: Topology, Metrics
- If Dodeca: Build, Pages, Templates

### Generic trace views

Core views should work for any app:
- trace list with filters
- trace detail waterfall / timeline
- error highlighting
- live streaming updates

---

## Implementation Phases (suggested order)

### Phase 1: Core Hindsight
- protocol types + `HindsightService`
- server: ingestion + in-memory storage + streaming events
- client: tracer library + batching

### Phase 2: Service discovery integration
- call `ServiceIntrospection` on connect
- store capabilities per connection
- trace classification (Generic/Picante/Rapace/Mixed)

### Phase 3: Generic Web UI
- WASM client over WebSocket transport
- list/get/stream traces and render generic views

### Phase 4: Picante integration
- Picante emits spans with `picante.*` attrs
- Hindsight specializes view and can call PicanteIntrospection

### Phase 5: Rapace integration
- topology + metrics via RapaceIntrospection

### Phase 6: Dodeca integration
- build/page/template introspection

### Phase 7: TUI (optional but useful)
- connect over TCP and render traces + specialized views where feasible

### Phase 8: Streaming polish + persistence (optional)
- streaming “live” UX improvements
- optional persistence (SQLite or similar), export, sampling

---

## Testing Strategy

- Unit: protocol encode/decode; store + classification; discovery logic
- Integration: app connects → discovery → ingest spans → query traces
- End-to-end: generic app; Picante app; Rapace multi-cell app; mixed traces

---

## Notes / Open Questions

- Dependency representation for Picante graphs: parent/child spans vs explicit `depends_on` attributes vs both.
- Payload size and secrecy: whether to include query inputs/results; prefer off-by-default and explicit opt-in.
- Real-time graph streaming: start with batch/trace-complete graphs, then add streaming updates if needed.

