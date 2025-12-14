# hindsight

[![MIT + Apache 2.0](https://img.shields.io/badge/license-MIT%20%2B%20Apache%202.0-blue)](./LICENSE-MIT)

**Distributed tracing made simple.** A standalone trace collection server and visualization platform for Rust applications.

## What is Hindsight?

Hindsight is a **tracing server** that collects W3C Trace Context spans from any application and provides beautiful visualization:

- 🔥 **Flamegraph view** - See where time is actually spent
- 🌳 **Trace tree** - Navigate parent-child span relationships
- 📊 **Waterfall timeline** - Visualize parallel execution and latency
- 💻 **TUI mode** - SSH-friendly terminal UI with `ratatui`

## Philosophy

**Simple, not SimplifiedTM.** Hindsight does one thing well: collect traces and show them to you. No complex configuration, no vendor lock-in, no heavyweight collectors.

**Language agnostic.** Any language that can send HTTP can send spans to Hindsight. Rust, JavaScript, Python, Go—it doesn't matter.

**Ephemeral by default.** Traces are kept in memory with a TTL. This keeps Hindsight fast and simple. Need persistence? Optional disk storage is available.

**W3C Trace Context compatible.** Works with OpenTelemetry, Jaeger, and any other tracing system that uses W3C trace context.

## Quick Start

**Install:**
```bash
cargo install hindsight
```

**Run the server:**
```bash
hindsight serve --port 9090
```

**Or use the TUI:**
```bash
hindsight tui --connect localhost:9090
```

**Instrument your Rust app:**
```rust
use hindsight::Tracer;

#[tokio::main]
async fn main() {
    let tracer = Tracer::connect("http://localhost:9090").await?;

    let span = tracer.span("processing_request")
        .with_attribute("user_id", 123)
        .start();

    // Do work...
    do_expensive_work().await?;

    span.end();
}
```

Navigate to `http://localhost:9090` and see your traces!

## Integration with Bearcove Projects

Hindsight has first-class integrations with the Bearcove ecosystem:

### Rapace (RPC Framework)

```rust
use rapace::RpcSession;
use hindsight::Tracer;

let tracer = Tracer::connect("http://localhost:9090").await?;
let session = RpcSession::new(transport)
    .with_tracer(tracer); // Automatic RPC span tracking!

// All RPC calls now appear in Hindsight
session.call(method_id, payload).await?;
```

### Picante (Incremental Computation)

```rust
use picante::Runtime;
use hindsight::Tracer;

let tracer = Tracer::connect("http://localhost:9090").await?;
let runtime = Runtime::new()
    .with_tracer(tracer); // Automatic query execution tracking!

// Query execution shows up as spans
let result = db.my_query.get(&db, key).await?;
```

### Dodeca (Static Site Generator)

```rust
use hindsight::Tracer;

let tracer = Tracer::connect("http://localhost:9090").await?;

// See your entire build pipeline traced:
// File change → Markdown parse → Image optimization → Template render
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│            Hindsight Server                     │
├─────────────────────────────────────────────────┤
│  Ingestion:  HTTP POST, WebSocket, Rapace RPC  │
│  Storage:    In-memory (TTL) + optional disk    │
│  Query API:  REST + WebSocket for live updates  │
│  UI:         Embedded web UI + TUI              │
└─────────────────────────────────────────────────┘
        ▲                            │
        │ W3C Trace Context          │
        │ (traceparent header)       │ HTTP/WS
        │                            ▼
┌───────┴────────┬──────────┬────────────────┐
│ Your Rust App  │  Rapace  │  Picante      │
│ (hindsight)    │  (opt-in)│  (opt-in)     │
└────────────────┴──────────┴────────────────┘
```

## Workspace Structure

```
crates/
├── hindsight/          # Client library (send spans)
├── hindsight-server/   # Server binary (collect + serve)
├── hindsight-tui/      # TUI client (ratatui)
├── hindsight-protocol/ # Shared span protocol
└── hindsight-ui/       # Web UI (embedded)
```

## Features

- ✅ **W3C Trace Context** - Standard `traceparent`/`tracestate` headers
- ✅ **Zero config** - Works out of the box
- ✅ **Fast** - In-memory storage, optimized for dev workflows
- ✅ **Beautiful UI** - Modern web interface + terminal TUI
- ✅ **Live updates** - WebSocket streaming of new spans
- ✅ **Multi-protocol** - HTTP, WebSocket, Rapace RPC ingestion
- 🚧 **Persistent storage** - Optional disk/DB backend (planned)
- 🚧 **Sampling** - Configurable trace sampling (planned)
- 🚧 **Export** - Export to Jaeger/Zipkin format (planned)

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
cargo run -p hindsight-server -- serve --port 9090
```

**Run the TUI:**
```bash
cargo run -p hindsight-tui -- --connect localhost:9090
```

## Contributing

See [PLAN.md](./PLAN.md) for the detailed implementation plan.

Contributions welcome! Please open issues and PRs.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](./LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
