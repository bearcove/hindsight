use facet::Facet;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace_context::{TraceId, SpanId};

/// Timestamp in nanoseconds since UNIX epoch
#[derive(Clone, Copy, Debug, Facet, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub attributes: BTreeMap<String, AttributeValue>,
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
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Event within a span
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: Timestamp,
    pub attributes: BTreeMap<String, AttributeValue>,
}

/// Span completion status
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum SpanStatus {
    Ok,
    Error { message: String },
}

/// Complete trace (collection of spans)
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Facet, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, Facet, Serialize, Deserialize)]
pub struct TraceFilter {
    pub service: Option<String>,
    pub min_duration_nanos: Option<u64>,
    pub max_duration_nanos: Option<u64>,
    pub has_errors: Option<bool>,
    pub limit: Option<usize>,
}
