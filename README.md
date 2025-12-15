# hindsight

[![MIT + Apache 2.0](https://img.shields.io/badge/license-MIT%20%2B%20Apache%202.0-blue)](./LICENSE-MIT)

**Unified observability hub for Bearcove tools.** Distributed tracing + live introspection over **Rapace RPC**.

## Status

This repo is under active development. The README is the short synthesis of the current plan; `PLAN.md` is the detailed design doc/spec.

## What is Hindsight?

Hindsight is a **trace collection server + UI** that:
- collects W3C Trace Context spans from apps (via Rapace RPC transports),
- discovers app capabilities at runtime (service introspection),
- and adapts its UI dynamically (generic trace views + framework-specific views).

The goal is one place to debug:
- **Rapace** (RPC): topology, transport metrics, active calls
- **Picante** (incremental): query graphs, cache hits/misses/validation
- **Dodeca** (build): build progress, pages, template stats

## Philosophy

**Pure Rapace.** One protocol end-to-end. HTTP exists only to serve a tiny static page that loads the browser UI; trace data flows over Rapace.

**Extensible by discovery.** Apps expose introspection services; Hindsight calls `ServiceIntrospection.list_services()` and enables views accordingly.

**Ephemeral by default.** In-memory storage with TTL (persistence/export are planned).

**Avoid self-tracing loops.** Hindsight’s own Rapace sessions are untraced; tracing in apps is explicit opt-in.

## Quick Start

### Run locally (recommended for now)

```bash
cargo run -p hindsight-server -- serve
```

Defaults:
- **Single port**: `http://127.0.0.1:1990`
  - `GET /` serves the web UI bootstrap page
  - `Upgrade: websocket` upgrades to WebSocket → Rapace RPC (browser/WASM)
  - `Upgrade: rapace` upgrades to raw Rapace (native clients)

### Install the server binary (local path)

```bash
cargo install --path crates/hindsight-server
hindsight serve
```

Then open `http://127.0.0.1:1990`.

## Integration with Bearcove Projects

Hindsight’s plan is to provide **generic tracing** plus **framework-specific views** when the app exposes introspection services.

### Rapace (RPC Framework)

```rust
use rapace::RpcSession;
use hindsight::Tracer;

// Create a tracer that exports spans to Hindsight.
// (Transport setup omitted here for brevity.)
let tracer = /* ... */;

let session = RpcSession::new(transport)
    .with_tracer(tracer); // Automatic RPC span tracking!

// All RPC calls now appear in Hindsight
session.call(method_id, payload).await?;
```

### Picante (Incremental Computation)

```rust
use picante::Runtime;
use hindsight::Tracer;

let tracer = /* ... */;
let runtime = Runtime::new()
    .with_tracer(tracer); // Planned: emit spans with picante.* attributes

// Query execution shows up as spans
let result = db.my_query.get(&db, key).await?;
```

### Dodeca (Static Site Generator)

```rust
use hindsight::Tracer;

let tracer = /* ... */;

// See your entire build pipeline traced:
// File change → Markdown parse → Image optimization → Template render
```

## Architecture

```
Apps (native / WASM)                     Hindsight (hub)
┌─────────────────────────┐              ┌──────────────────────────┐
│ App emits spans          │──Rapace RPC─▶│ HindsightService         │
│ + exposes introspection  │              │ - ingest_spans           │
│ services (optional)      │◀─Rapace RPC──│ - list/get/stream traces │
└─────────────────────────┘              │                          │
                                         │ UI adapts based on:      │
                                         │ - ServiceIntrospection   │
                                         │ - PicanteIntrospection   │
                                         │ - RapaceIntrospection    │
                                         │ - DodecaIntrospection    │
                                         └──────────────────────────┘
```

## Workspace Structure

```
crates/
├── hindsight/          # Client library (emit/export spans)
├── hindsight-server/   # Server binary (`hindsight`)
├── hindsight-tui/      # TUI client (planned; currently a stub)
└── hindsight-protocol/ # Shared protocol types + RPC service trait
```

## Features

- ✅ **W3C Trace Context** (`traceparent`/`tracestate`)
- ✅ **Pure Rapace RPC ingestion** (TCP + WebSocket transport)
- ✅ **Ephemeral in-memory store** (TTL)
- 🚧 **Service discovery driven UI** (planned: dynamic tabs per app capabilities)
- 🚧 **Framework-specific views** (Picante/Rapace/Dodeca via introspection)
- 🚧 **Persistence / sampling / export** (planned)

## Example: Distributed Trace Across Systems

```rust
// In your web server
let span = tracer.span("handle_request").start();

// Make an RPC call (trace context auto-propagated)
let result = rpc_client.call(method, payload).await?;

// That RPC triggers a Picante query in another process
// All show up in ONE trace:
//
// handle_request (50ms)
//   ├─ RPC: calculate (40ms)
//   │   ├─ Picante: load_data (5ms, cache hit)
//   │   └─ Picante: compute (35ms, recomputed)
//   └─ format_response (10ms)

span.end();
```

## Development

**Build:**
```bash
cargo build --workspace
```

**Run tests:**
```bash
cargo test --workspace
```

**Run the server locally:**
```bash
cargo run -p hindsight-server -- serve
```

**Plan/spec docs:**
- `UNIFIED_PLAN.md` (short synthesis)
- `PLAN.md` (detailed design doc/spec)

## Contributing

See `PLAN.md` for the detailed design doc/spec, and `UNIFIED_PLAN.md` for the short synthesis.

Contributions welcome! Please open issues and PRs.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](./LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
