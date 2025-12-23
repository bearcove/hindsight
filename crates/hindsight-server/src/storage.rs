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
            let is_new_trace =
                span.parent_span_id.is_none() && !self.spans.contains_key(&span.span_id);

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
        let mut summaries: Vec<TraceSummary> = self
            .traces
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
                    if duration.is_none_or(|d| d < min_dur) {
                        return None;
                    }
                }

                if let Some(max_dur) = filter.max_duration_nanos {
                    if duration.is_some_and(|d| d > max_dur) {
                        return None;
                    }
                }

                let has_errors = trace
                    .spans
                    .iter()
                    .any(|s| matches!(s.status, SpanStatus::Error { .. }));

                if let Some(filter_errors) = filter.has_errors {
                    if has_errors != filter_errors {
                        return None;
                    }
                }

                let root_span = trace
                    .spans
                    .iter()
                    .find(|s| s.span_id == trace.root_span_id)?;

                // Classify trace type based on attributes
                let trace_type = trace.classify_type();

                Some(TraceSummary {
                    trace_id: trace.trace_id,
                    root_span_name: root_span.name.clone(),
                    service_name: root_span.service_name.clone(),
                    start_time: trace.start_time,
                    duration_nanos: duration,
                    span_count: trace.spans.len(),
                    has_errors,
                    trace_type,
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
        let spans: Vec<Span> = self
            .spans
            .iter()
            .filter(|entry| entry.value().trace_id == trace_id)
            .map(|entry| entry.value().clone())
            .collect();

        if !spans.is_empty() {
            if let Some(trace) = Trace::from_spans(spans) {
                // Check if trace is complete
                let is_complete =
                    trace.end_time.is_some() && trace.spans.iter().all(|s| s.end_time.is_some());

                if is_complete {
                    if let Some(duration) = trace.end_time.map(|e| e.0 - trace.start_time.0) {
                        let _ = self.event_tx.send(TraceEvent::TraceCompleted {
                            trace_id,
                            duration_nanos: duration,
                            span_count: trace.spans.len(),
                        });
                    }
                }

                self.traces.insert(
                    trace_id,
                    StoredTrace {
                        trace,
                        created_at: SystemTime::now(),
                    },
                );
            }
        }
    }

    fn cleanup_expired(&self) {
        let now = SystemTime::now();
        self.traces.retain(|_, stored| {
            now.duration_since(stored.created_at).unwrap_or_default() < self.ttl
        });
    }
}
