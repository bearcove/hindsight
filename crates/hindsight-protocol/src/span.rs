use facet::Facet;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace_context::{SpanId, TraceId};

/// Timestamp in nanoseconds since UNIX epoch
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
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
#[derive(Clone, Debug, Facet)]
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
#[derive(Clone, Debug, Facet)]
#[repr(u8)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Event within a span
#[derive(Clone, Debug, Facet)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: Timestamp,
    pub attributes: BTreeMap<String, AttributeValue>,
}

/// Span completion status
#[derive(Clone, Debug, Facet)]
#[repr(u8)]
pub enum SpanStatus {
    Ok,
    Error { message: String },
}

/// Complete trace (collection of spans)
#[derive(Clone, Debug, Facet)]
pub struct Trace {
    pub trace_id: TraceId,
    pub spans: Vec<Span>,
    pub root_span_id: SpanId,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
}

impl Trace {
    /// Build a trace from a flat list of spans
    pub fn from_spans(mut spans: Vec<Span>) -> Option<Self> {
        if spans.is_empty() {
            return None;
        }

        spans.sort_by_key(|s| s.start_time.0);

        let trace_id = spans[0].trace_id;

        // Find root span (one with no parent)
        let root_span = spans.iter().find(|s| s.parent_span_id.is_none())?;

        let root_span_id = root_span.span_id;
        let start_time = root_span.start_time;

        let end_time = spans.iter().filter_map(|s| s.end_time).max_by_key(|t| t.0);

        Some(Self {
            trace_id,
            spans,
            root_span_id,
            start_time,
            end_time,
        })
    }

    /// Get children of a given span
    pub fn children(&self, span_id: SpanId) -> Vec<&Span> {
        self.spans
            .iter()
            .filter(|s| s.parent_span_id == Some(span_id))
            .collect()
    }

    /// Classify trace type based on span attributes
    pub fn classify_type(&self) -> TraceType {
        let mut has_picante = false;
        let mut has_rapace = false;
        let mut has_dodeca = false;

        for span in &self.spans {
            // Check for Picante attributes
            if span.attributes.contains_key("picante.query") {
                has_picante = true;
            }

            // Check for Rapace RPC attributes
            if let Some(AttributeValue::String(s)) = span.attributes.get("rpc.system") {
                if s == "rapace" {
                    has_rapace = true;
                }
            }

            // Check for Dodeca attributes
            if span.attributes.contains_key("dodeca.build") {
                has_dodeca = true;
            }
        }

        // Count how many framework types detected
        let count = [has_picante, has_rapace, has_dodeca]
            .iter()
            .filter(|&&x| x)
            .count();

        match count {
            0 => TraceType::Generic,
            1 => {
                if has_picante {
                    TraceType::Picante
                } else if has_rapace {
                    TraceType::Rapace
                } else {
                    TraceType::Dodeca
                }
            }
            _ => TraceType::Mixed,
        }
    }
}

/// Type of trace based on framework detection
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TraceType {
    /// Generic trace with no special attributes
    #[default]
    Generic,
    /// Picante query execution trace
    Picante,
    /// Rapace RPC trace
    Rapace,
    /// Dodeca build trace
    Dodeca,
    /// Mixed trace with multiple framework types
    Mixed,
}

impl std::fmt::Display for TraceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceType::Generic => write!(f, "Generic"),
            TraceType::Picante => write!(f, "Picante"),
            TraceType::Rapace => write!(f, "Rapace"),
            TraceType::Dodeca => write!(f, "Dodeca"),
            TraceType::Mixed => write!(f, "Mixed"),
        }
    }
}

/// Summary of a trace (for listing)
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct TraceSummary {
    pub trace_id: TraceId,
    pub root_span_name: String,
    pub service_name: String,
    pub start_time: Timestamp,
    pub duration_nanos: Option<u64>,
    pub span_count: usize,
    pub has_errors: bool,
    pub trace_type: TraceType,
}

/// Filter for querying traces
#[derive(Clone, Debug, Default, Facet)]
pub struct TraceFilter {
    pub service: Option<String>,
    pub min_duration_nanos: Option<u64>,
    pub max_duration_nanos: Option<u64>,
    pub has_errors: Option<bool>,
    pub limit: Option<usize>,
}
