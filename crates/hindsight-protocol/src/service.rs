use rapace::Streaming;

use crate::events::*;
use crate::span::*;
use crate::trace_context::*;

/// Hindsight tracing service (pure Rapace RPC)
#[allow(async_fn_in_trait)]
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
