# Hindsight + Picante Integration Plan
## Teaching Hindsight to Visualize Incremental Computation

**Date**: 2025-12-15
**Depends On**: PLAN.md (Hindsight core implementation)
**Purpose**: Extend Hindsight to understand Picante's incremental computation model

---

## Table of Contents

1. [Vision](#vision)
2. [Architecture](#architecture)
3. [Picante Span Schema](#picante-span-schema)
4. [Hindsight Detection Logic](#hindsight-detection-logic)
5. [UI Extensions](#ui-extensions)
6. [Implementation Phases](#implementation-phases)
7. [Examples](#examples)

---

## Vision

**Problem**: Picante's incremental computation system has domain-specific concepts that generic span tracing can't fully capture:
- Dependency graphs (not just parent/child hierarchies)
- Cache hits vs misses vs early cutoff validation
- Query kind grouping and statistics
- "What changed" propagation through the system

**Solution**: Hindsight learns to detect Picante traces via span attributes and renders specialized visualizations.

**Benefit**: One unified observability UI instead of maintaining separate debug UIs.

---

## Architecture

### The Flow

```
Picante Application                     Hindsight Server                 Hindsight UI
┌───────────────────┐                  ┌─────────────────┐             ┌──────────────┐
│ Query execution   │                  │ HindsightService│             │ Generic View │
│                   │                  │                 │             │ (all traces) │
│ Emit spans with   │──Rapace RPC─────▶│ Store spans     │◀──Rapace───│              │
│ picante.* attrs   │  ingest_spans()  │                 │   RPC      │ Picante View │
└───────────────────┘                  │ Detect Picante  │             │ (specialized)│
                                       │ Build dep graph │             └──────────────┘
                                       └─────────────────┘
```

### Key Principle

**Picante doesn't build UI** - it just emits detailed spans.
**Hindsight detects** Picante-specific attributes and adapts its UI.

---

## Picante Span Schema

### Required Attributes

When Picante emits a span representing a query execution, it MUST include:

```rust
Span {
    trace_id: TraceId,           // W3C trace context
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    name: String,                // e.g., "Query::parse_file"
    service_name: "my-app",

    attributes: {
        // REQUIRED: Identifies this as a Picante query
        "picante.query": true,

        // REQUIRED: Query identification
        "picante.query_kind": "parse_file",
        "picante.query_key": "src/main.rs",

        // REQUIRED: Cache behavior
        "picante.cache_status": "hit" | "miss" | "validated",

        // OPTIONAL: Additional context
        "picante.revision": 42,
        "picante.dependency_count": 3,
        "picante.input_changed": false,
    }
}
```

### Cache Status Values

- **`"miss"`**: Query was recomputed (cache miss)
- **`"hit"`**: Query result retrieved from memo table
- **`"validated"`**: Query used early cutoff (verified unchanged via dependencies)

### Input Set Spans

When an input is set, emit a special span:

```rust
Span {
    name: "Input::set",
    attributes: {
        "picante.input": true,
        "picante.input_kind": "file_text",
        "picante.input_key": "src/main.rs",
        "picante.old_revision": 41,
        "picante.new_revision": 42,
    }
}
```

### Dependency Relationships

Dependencies are represented via standard span parent/child relationships:

```
[Span] Input::set (file_text, "src/main.rs")
   └─ [Span] Query::parse_file ("src/main.rs")  ← child of input
       └─ [Span] Query::extract_functions ("src/main.rs")  ← child of parse
```

**Alternative**: Use `picante.depends_on` attribute with span IDs:
```rust
attributes: {
    "picante.depends_on": "[span_id_1, span_id_2, span_id_3]"  // JSON array
}
```

This allows non-hierarchical dependencies (query A depends on queries B and C, but they're not parent/child).

---

## Hindsight Detection Logic

### Trace Classification

When Hindsight ingests spans, it detects Picante traces:

```rust
// In hindsight-server/src/storage.rs

fn classify_trace(trace: &Trace) -> TraceType {
    let has_picante_spans = trace.spans.iter()
        .any(|s| s.attributes.get("picante.query") == Some(&AttributeValue::Bool(true)));

    if has_picante_spans {
        TraceType::Picante
    } else {
        TraceType::Generic
    }
}

enum TraceType {
    Generic,
    Picante,
    // Future: Rapace, Http, etc.
}
```

### Dependency Graph Construction

For Picante traces, build a dependency graph from span relationships:

```rust
// In hindsight-protocol/src/picante.rs (new file)

use crate::span::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct PicanteGraph {
    pub nodes: Vec<PicanteNode>,
    pub edges: Vec<PicanteEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct PicanteNode {
    pub span_id: SpanId,
    pub query_kind: String,
    pub query_key: String,
    pub cache_status: CacheStatus,
    pub duration_nanos: u64,
    pub is_input: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub enum CacheStatus {
    Hit,
    Miss,
    Validated,
}

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct PicanteEdge {
    pub from: SpanId,  // dependency
    pub to: SpanId,    // dependent
}

pub fn build_picante_graph(trace: &Trace) -> Option<PicanteGraph> {
    // Extract Picante spans
    let picante_spans: Vec<_> = trace.spans.iter()
        .filter(|s| s.attributes.get("picante.query") == Some(&AttributeValue::Bool(true)))
        .collect();

    if picante_spans.is_empty() {
        return None;
    }

    // Build nodes
    let nodes = picante_spans.iter().map(|span| {
        PicanteNode {
            span_id: span.span_id,
            query_kind: extract_string(&span.attributes, "picante.query_kind")
                .unwrap_or_else(|| "unknown".to_string()),
            query_key: extract_string(&span.attributes, "picante.query_key")
                .unwrap_or_else(|| "?".to_string()),
            cache_status: extract_cache_status(span),
            duration_nanos: span.duration_nanos().unwrap_or(0),
            is_input: span.attributes.get("picante.input") == Some(&AttributeValue::Bool(true)),
        }
    }).collect();

    // Build edges from parent/child relationships
    let mut edges = Vec::new();
    for span in &picante_spans {
        if let Some(parent_id) = span.parent_span_id {
            edges.push(PicanteEdge {
                from: parent_id,
                to: span.span_id,
            });
        }
    }

    Some(PicanteGraph { nodes, edges })
}

fn extract_cache_status(span: &Span) -> CacheStatus {
    match extract_string(&span.attributes, "picante.cache_status").as_deref() {
        Some("hit") => CacheStatus::Hit,
        Some("validated") => CacheStatus::Validated,
        _ => CacheStatus::Miss,
    }
}

fn extract_string(attrs: &HashMap<String, AttributeValue>, key: &str) -> Option<String> {
    match attrs.get(key) {
        Some(AttributeValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}
```

---

## UI Extensions

### Service Definition Extension

Add Picante-specific RPC methods to `HindsightService`:

```rust
// In hindsight-protocol/src/service.rs

#[rapace::service]
pub trait HindsightService {
    // ... existing methods ...

    /// Get Picante-specific graph for a trace
    ///
    /// Returns None if this is not a Picante trace.
    async fn get_picante_graph(&self, trace_id: TraceId) -> Option<PicanteGraph>;

    /// Get Picante statistics across all traces
    async fn get_picante_stats(&self) -> PicanteStats;
}
```

### Statistics

```rust
// In hindsight-protocol/src/picante.rs

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct PicanteStats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_validations: u64,
    pub cache_hit_rate: f64,  // 0.0 - 1.0

    /// Stats grouped by query kind
    pub by_query_kind: HashMap<String, QueryKindStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Facet)]
pub struct QueryKindStats {
    pub query_kind: String,
    pub total_executions: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_validations: u64,
    pub avg_duration_nanos: u64,
    pub max_duration_nanos: u64,
}
```

### Web UI Views

#### 1. Trace List View (Enhanced)

When listing traces, show Picante-specific metadata:

```
┌─────────────────────────────────────────────────────────────────┐
│ Traces                                             [Filter ▼]   │
├─────────────────────────────────────────────────────────────────┤
│ 🔍 4bf92f... · parse_markdown · 2.3s                           │
│    Spans: 12 · Errors: 0 · Service: dodeca                      │
│                                                                  │
│ 🔄 7a8d32... · file_text("main.rs") · 145ms [PICANTE]          │
│    Queries: 5 · Cache hits: 4 (80%) · Service: my-app          │
│                                                                  │
│ 🔍 9e1f45... · HTTP GET /api/users · 89ms                      │
│    Spans: 8 · Errors: 1 · Service: api-server                   │
└─────────────────────────────────────────────────────────────────┘
```

#### 2. Picante Trace View (New)

When viewing a Picante trace, show specialized UI:

```html
<!-- In hindsight-server/src/ui/picante_trace.html -->

<div id="picante-trace-view">
    <!-- Left panel: Dependency graph -->
    <div id="graph-panel">
        <svg id="dependency-graph">
            <!-- D3.js force-directed graph -->
            <!-- Nodes colored by cache status:
                 🔴 Red = input changed
                 🟡 Yellow = cache miss (recomputed)
                 🟢 Green = validated (early cutoff)
                 🔵 Blue = cache hit (memo)
            -->
        </svg>
    </div>

    <!-- Right panel: Details -->
    <div id="details-panel">
        <div id="trace-summary">
            <h3>Trace Summary</h3>
            <div>Total queries: <span id="total-queries"></span></div>
            <div>Cache hit rate: <span id="cache-hit-rate"></span></div>
            <div>Duration: <span id="duration"></span></div>
        </div>

        <div id="query-details">
            <h3>Selected Query</h3>
            <!-- Show details when clicking a node -->
        </div>

        <div id="query-timeline">
            <h3>Execution Timeline</h3>
            <!-- Waterfall view of query execution order -->
        </div>
    </div>
</div>
```

#### 3. Picante Statistics View (New)

Global view of Picante performance:

```
┌─────────────────────────────────────────────────────────────────┐
│ Picante Statistics                                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│ Overall                                                          │
│ ├─ Total queries: 1,234                                         │
│ ├─ Cache hit rate: 87.3%                                        │
│ ├─ Cache hits: 892                                              │
│ ├─ Cache validations: 186                                       │
│ └─ Cache misses: 156                                            │
│                                                                  │
│ By Query Kind                                                    │
│ ┌────────────────┬───────┬─────────┬──────────┬─────────────┐  │
│ │ Query Kind     │ Count │ Hit %   │ Avg Time │ Max Time    │  │
│ ├────────────────┼───────┼─────────┼──────────┼─────────────┤  │
│ │ parse_file     │ 342   │ 92.1%   │ 12.3ms   │ 234ms       │  │
│ │ file_text      │ 289   │ 95.8%   │ 1.2ms    │ 45ms        │  │
│ │ extract_funcs  │ 203   │ 78.3%   │ 8.7ms    │ 123ms       │  │
│ └────────────────┴───────┴─────────┴──────────┴─────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Picante Span Schema (Coordination)

**Work in Picante repository**:
- Define span attribute schema (this document)
- Instrument Picante Runtime to emit spans
- Add `hindsight::Tracer` integration
- Test span emission with mock Hindsight

**Deliverable**: Picante emits properly formatted spans

### Phase 2: Hindsight Detection (Server)

**Work in Hindsight repository**:
- Create `hindsight-protocol/src/picante.rs`
- Add `PicanteGraph`, `PicanteStats` types
- Implement `build_picante_graph()` function
- Add trace classification logic in storage

**Deliverable**: Hindsight detects and classifies Picante traces

### Phase 3: Hindsight RPC Extensions (Server)

**Work in Hindsight repository**:
- Add `get_picante_graph()` to `HindsightService`
- Add `get_picante_stats()` to `HindsightService`
- Implement server-side logic in `HindsightServiceImpl`

**Deliverable**: Clients can query Picante-specific data

### Phase 4: Web UI - Trace List Enhancement

**Work in Hindsight repository**:
- Detect Picante traces in trace list
- Show `[PICANTE]` badge
- Display cache hit rate in summary

**Deliverable**: Users can identify Picante traces

### Phase 5: Web UI - Picante Trace View

**Work in Hindsight repository**:
- Create dedicated Picante trace view
- D3.js dependency graph visualization
- Color nodes by cache status
- Show query details panel

**Deliverable**: Beautiful Picante-specific visualization

### Phase 6: Web UI - Statistics Dashboard

**Work in Hindsight repository**:
- Global Picante statistics view
- Query kind performance table
- Cache hit rate trends

**Deliverable**: Performance insights across all Picante traces

### Phase 7: TUI Support

**Work in Hindsight repository**:
- Add Picante view to TUI
- ASCII dependency graph (or simplified list view)
- Statistics summary

**Deliverable**: TUI can view Picante traces

---

## Examples

### Example 1: Simple File Parse

**Picante emits**:
```rust
// Span 1: Input set
Span {
    name: "Input::set",
    attributes: {
        "picante.input": true,
        "picante.input_kind": "file_text",
        "picante.input_key": "main.rs",
    }
}

// Span 2: Parse file (child of input)
Span {
    name: "Query::parse_file",
    parent_span_id: Some(span_1_id),
    attributes: {
        "picante.query": true,
        "picante.query_kind": "parse_file",
        "picante.query_key": "main.rs",
        "picante.cache_status": "miss",  // Recomputed because input changed
    }
}
```

**Hindsight shows**:
```
Graph:
  [🔴 Input: file_text("main.rs")]
      ↓
  [🟡 Query: parse_file("main.rs")]  ← Yellow = cache miss
```

### Example 2: Cached Query Chain

**Picante emits**:
```rust
// User requests parse_file("main.rs"), no input changed

Span {
    name: "Query::file_text",
    attributes: {
        "picante.query": true,
        "picante.query_kind": "file_text",
        "picante.query_key": "main.rs",
        "picante.cache_status": "hit",  // Memo hit
    }
}

Span {
    name: "Query::parse_file",
    parent_span_id: Some(file_text_span_id),
    attributes: {
        "picante.query": true,
        "picante.query_kind": "parse_file",
        "picante.query_key": "main.rs",
        "picante.cache_status": "validated",  // Early cutoff
    }
}
```

**Hindsight shows**:
```
Graph:
  [🔵 Query: file_text("main.rs")]  ← Blue = cache hit
      ↓
  [🟢 Query: parse_file("main.rs")]  ← Green = validated
```

### Example 3: Distributed Trace (Picante → Rapace → Dodeca)

**Picante emits**:
```rust
Span {
    name: "Query::render_markdown",
    trace_id: trace_123,
    span_id: span_456,
    attributes: {
        "picante.query": true,
        "picante.query_kind": "render_markdown",
        "picante.cache_status": "miss",
    }
}
```

**Rapace emits** (child of Picante span):
```rust
Span {
    name: "RPC: DodecaService::parse_markdown",
    trace_id: trace_123,
    parent_span_id: Some(span_456),
    attributes: {
        "rapace.service": "DodecaService",
        "rapace.method": "parse_markdown",
    }
}
```

**Dodeca emits** (child of Rapace span):
```rust
Span {
    name: "markdown_parser::parse",
    trace_id: trace_123,
    parent_span_id: Some(rapace_span_id),
    attributes: {
        "dodeca.file": "README.md",
    }
}
```

**Hindsight shows**:
```
Mixed trace (Picante + Rapace + Dodeca):
  [🟡 Picante: render_markdown]
      ↓
  [RPC: DodecaService::parse_markdown]
      ↓
  [Dodeca: markdown_parser::parse]

User can switch between:
- Generic view (all spans as waterfall)
- Picante view (focused on query graph)
```

---

## Open Questions

### 1. Dependency Representation

Should we use:
- **Option A**: Parent/child span relationships (existing W3C standard)
- **Option B**: `picante.depends_on` attribute with span IDs (more flexible)
- **Option C**: Both (parent/child for execution order, depends_on for logical deps)

**Recommendation**: Start with Option A, add Option B if needed.

### 2. Revision Tracking

Should Picante include revision numbers in spans?
- Useful for debugging "why did this recompute?"
- Might be too verbose

**Recommendation**: Include as optional attribute, don't require it.

### 3. Value Serialization

Should Picante include query results/inputs in spans?
- Pro: Full debuggability
- Con: Large payload, potential secrets

**Recommendation**: Feature-gated, off by default. Allow users to register custom serializers.

### 4. Real-time vs Batch

Should Hindsight stream Picante graph updates in real-time?
- Would enable "flash animations" showing recomputation cascade
- Requires streaming protocol

**Recommendation**: Phase 1 = batch (query after trace completes), Phase 2 = streaming.

---

## Coordination Points

### Picante Team (that's us!)

**Must define**:
- Exact attribute schema (finalize from this doc)
- Span emission timing (when to start/end spans)
- Integration API (`Runtime::with_hindsight_tracer()`)

**Must implement**:
- Instrumentation in `runtime.rs`, `ingredient/derived.rs`
- Span emission with correct attributes
- Tests to verify span format

### Hindsight Team (also us, different hat!)

**Must implement**:
- Picante detection logic
- Graph construction algorithm
- RPC method extensions
- UI views

**Must test**:
- Handle malformed Picante attributes gracefully
- Work with mixed traces (Picante + Rapace + generic spans)
- Performance with large dependency graphs

---

## Success Criteria

**Phase 1 Success**: Picante emits spans, Hindsight ingests them (generic view works)

**Phase 2 Success**: Hindsight detects Picante traces, builds dependency graph

**Phase 3 Success**: Hindsight UI shows Picante-specific view with colored graph

**Final Success**: Developer can:
1. Run Hindsight server
2. Run Picante app with `hindsight::Tracer` configured
3. Open Hindsight UI
4. See Picante traces with dependency graph
5. Understand cache behavior at a glance
6. Debug "why did this recompute?" questions

---

## Future Enhancements

### Live Flash Animations

Stream trace updates in real-time, animate nodes as they execute:
```
User edits file → [🔴 Input flash]
                    ↓
                [🟡 Query flashes yellow (recomputing)]
                    ↓
                [🟢 Dependent queries flash green (validated)]
```

### Diff Mode

Compare two traces, highlight what changed:
```
Trace A (before):  5 cache misses
Trace B (after):   2 cache misses  ✅ Improved!
```

### Query Timeline

Waterfall view showing when queries executed in parallel:
```
Time →
0ms   [file_text ████]
      [parse_file        ████████]
      [extract_funcs              ████]
      [highlight_syntax           ██████]
```

### Export

Export Picante graph as:
- DOT file (for Graphviz)
- SVG image
- JSON (for custom tools)

---

## File Structure

```
hindsight/
├── PLAN.md                          # Core Hindsight (existing)
├── PLAN2.md                         # This document
├── crates/
│   ├── hindsight-protocol/
│   │   └── src/
│   │       ├── picante.rs           # NEW: Picante-specific types
│   │       └── service.rs           # MODIFY: Add Picante RPC methods
│   ├── hindsight-server/
│   │   └── src/
│   │       ├── storage.rs           # MODIFY: Add Picante detection
│   │       ├── service_impl.rs      # MODIFY: Implement Picante RPCs
│   │       └── ui/
│   │           ├── picante_trace.html   # NEW: Picante trace view
│   │           └── picante_stats.html   # NEW: Statistics view
│   └── hindsight-tui/
│       └── src/
│           └── picante_view.rs      # NEW: TUI Picante view
```

---

## Next Steps

1. **Finalize this plan** - Get agreement on attribute schema
2. **Implement in Picante** - Emit spans (Phase 1 from Picante side)
3. **Wait for core Hindsight** - Need PLAN.md phases 1-4 complete
4. **Implement Hindsight detection** - Phase 2 from this plan
5. **Build UI** - Phases 4-6 from this plan

---

**Author**: Claude (Sonnet 4.5)
**For**: Bearcove team
**Status**: Draft - awaiting review
