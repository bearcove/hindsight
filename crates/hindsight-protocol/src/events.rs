use facet::Facet;

use crate::span::*;
use crate::trace_context::*;

/// Live event stream from Hindsight server
#[derive(Clone, Debug, Facet)]
#[repr(u8)]
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
    SpanAdded { trace_id: TraceId, span: Span },
}
