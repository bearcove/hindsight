use hindsight_protocol::*;
use rapace::Streaming;
use std::sync::Arc;

use crate::storage::TraceStore;

#[derive(Clone)]
pub struct HindsightServiceImpl {
    store: Arc<TraceStore>,
}

impl HindsightServiceImpl {
    pub fn new(store: Arc<TraceStore>) -> Self {
        Self { store }
    }
}

impl HindsightService for HindsightServiceImpl {
    async fn ingest_spans(&self, spans: Vec<Span>) -> u32 {
        // Filter out any spans from Hindsight itself (prevent infinite loop!)
        let spans: Vec<_> = spans
            .into_iter()
            .filter(|span| span.service_name != "hindsight-server")
            .collect();

        self.store.ingest(spans)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Option<Trace> {
        self.store.get_trace(trace_id)
    }

    async fn list_traces(&self, filter: TraceFilter) -> Vec<TraceSummary> {
        self.store.list_traces(filter)
    }

    async fn stream_traces(&self) -> Streaming<TraceEvent> {
        let mut rx = self.store.subscribe_events();

        let stream = async_stream::stream! {
            while let Ok(event) = rx.recv().await {
                yield Ok(event);
            }
        };

        Box::pin(stream)
    }

    async fn ping(&self) -> String {
        "pong".to_string()
    }
}
