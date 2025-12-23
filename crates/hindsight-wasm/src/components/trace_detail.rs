//! Trace detail view component

use hindsight_protocol::*;
use rapace::{RpcSession, WebSocketTransport};
use std::collections::HashMap;
use std::sync::Arc;
use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::navigation::NavigationState;
use crate::routing::Route;

/// Hierarchical span node for tree rendering
#[derive(Clone, Debug)]
struct SpanNode {
    span: Span,
    children: Vec<SpanNode>,
    depth: usize,
}

impl SpanNode {
    fn from_trace(trace: &Trace) -> Vec<SpanNode> {
        let mut span_map: HashMap<SpanId, &Span> = HashMap::new();
        let mut children_map: HashMap<SpanId, Vec<SpanId>> = HashMap::new();
        let mut roots = Vec::new();

        // Build maps
        for span in &trace.spans {
            span_map.insert(span.span_id, span);
            if let Some(parent_id) = span.parent_span_id {
                children_map
                    .entry(parent_id)
                    .or_default()
                    .push(span.span_id);
            } else {
                roots.push(span.span_id);
            }
        }

        // Build tree recursively
        fn build_tree(
            span_id: SpanId,
            span_map: &HashMap<SpanId, &Span>,
            children_map: &HashMap<SpanId, Vec<SpanId>>,
            depth: usize,
        ) -> SpanNode {
            let span = span_map.get(&span_id).unwrap();
            let mut children = Vec::new();

            if let Some(child_ids) = children_map.get(&span_id) {
                // Sort children by start time
                let mut sorted_children = child_ids.clone();
                sorted_children
                    .sort_by_key(|id| span_map.get(id).map(|s| s.start_time.0).unwrap_or(0));

                for child_id in sorted_children {
                    children.push(build_tree(child_id, span_map, children_map, depth + 1));
                }
            }

            SpanNode {
                span: (*span).clone(),
                children,
                depth,
            }
        }

        roots
            .into_iter()
            .map(|root_id| build_tree(root_id, &span_map, &children_map, 0))
            .collect()
    }

    fn flatten(&self) -> Vec<(Span, usize, bool)> {
        let mut result = Vec::new();
        let has_children = !self.children.is_empty();
        result.push((self.span.clone(), self.depth, has_children));
        for child in &self.children {
            result.extend(child.flatten());
        }
        result
    }
}

/// Trace detail view - shows full trace information
#[component]
pub fn TraceDetail(props: TraceDetailProps) -> View {
    let nav_state = props.nav_state;
    let trace_id = props.trace_id.clone();

    let trace = create_signal(Option::<Trace>::None);
    let loading = create_signal(true);
    let error = create_signal(Option::<String>::None);

    // Fetch trace on mount
    {
        let trace = trace.clone();
        let loading = loading.clone();
        let error = error.clone();
        let trace_id = trace_id.clone();

        spawn_local(async move {
            match init_client().await {
                Ok(client) => match client.get_trace(trace_id).await {
                    Ok(Some(t)) => {
                        trace.set(Some(t));
                        loading.set(false);
                    }
                    Ok(None) => {
                        error.set(Some("Trace not found".to_string()));
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Error fetching trace: {:?}", e)));
                        loading.set(false);
                    }
                },
                Err(e) => {
                    error.set(Some(format!("Connection error: {}", e)));
                    loading.set(false);
                }
            }
        });
    }

    // Back button handler
    let on_back = move |_| {
        nav_state.navigate_to(Route::TraceList);
    };

    let title = create_memo(move || {
        trace.with(|t| {
            t.as_ref()
                .and_then(|tr| tr.spans.iter().find(|s| s.span_id == tr.root_span_id))
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Trace Detail".to_string())
        })
    });

    view! {
        div(class="trace-detail") {
            div(class="detail-header") {
                button(class="btn", on:click=on_back) { "← Back" }
                div(class="detail-title") {
                    (title.with(|t| t.clone()))
                }
                div(class="detail-meta") {
                    span(class="trace-meta-label") { "id:" }
                    " "
                    span(style="font-family: var(--font-mono); font-size: var(--text-xs);") {
                        (trace_id.to_hex())
                    }
                }
            }

            div(class="detail-content") {
                (if loading.with(|l| *l) {
                    view! {
                        div(class="loading") {
                            div(class="spinner") {}
                            "Loading trace..."
                        }
                    }
                } else if error.with(|e| e.is_some()) {
                    let err_msg = error.with(|e| e.clone().unwrap_or_default());
                    view! {
                        div(class="placeholder") {
                            p(style="color: var(--signal-error);") { (err_msg) }
                        }
                    }
                } else {
                    trace.with(|t| {
                        if let Some(tr) = t.as_ref() {
                            let nodes = SpanNode::from_trace(tr);
                            let flat_spans: Vec<_> = nodes.iter().flat_map(|n| n.flatten()).collect();

                            view! {
                                div(class="waterfall") {
                                    div(class="waterfall-header") {
                                        div { "Operation" }
                                        div { "Service" }
                                        div { "Duration" }
                                    }
                                    (
                                        flat_spans.clone().into_iter().map(|(span, depth, has_children)| {
                                            span_row_view(span, depth, has_children)
                                        }).collect::<Vec<_>>()
                                    )
                                }
                            }
                        } else {
                            view! {
                                div(class="placeholder") {
                                    p { "No trace data" }
                                }
                            }
                        }
                    })
                })
            }
        }
    }
}

/// Create a span row view
fn span_row_view(span: Span, depth: usize, has_children: bool) -> View {
    let is_error = matches!(span.status, SpanStatus::Error { .. });

    let duration_text = if let Some(end) = span.end_time {
        let nanos = end.0.saturating_sub(span.start_time.0);
        let ms = nanos as f64 / 1_000_000.0;
        if ms < 1.0 {
            format!("{:.0}µs", nanos as f64 / 1_000.0)
        } else if ms < 10.0 {
            format!("{:.2}ms", ms)
        } else if ms < 1000.0 {
            format!("{:.1}ms", ms)
        } else {
            format!("{:.2}s", ms / 1000.0)
        }
    } else {
        "—".to_string()
    };

    view! {
        div(
            class="span-row",
            data-error=is_error.to_string(),
            data-has-children=has_children.to_string(),
            style=format!("--depth: {}", depth),
            tabindex="0"
        ) {
            div(class="span-name-container") {
                div(class="span-hierarchy-icon") { "▸" }
                div(class="span-name") { (span.name.clone()) }
            }
            div(class="span-service") { (span.service_name.clone()) }
            div(class="span-duration") { (duration_text) }
        }
    }
}

#[derive(Props)]
pub struct TraceDetailProps {
    pub trace_id: TraceId,
    pub nav_state: NavigationState,
}

/// Initialize the Rapace client connection
async fn init_client() -> Result<HindsightServiceClient<WebSocketTransport>, String> {
    let protocol = if web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .map(|p| p == "https:")
        .unwrap_or(false)
    {
        "wss:"
    } else {
        "ws:"
    };

    let host = web_sys::window()
        .and_then(|w| w.location().host().ok())
        .unwrap_or_else(|| "localhost:1990".to_string());

    let url = format!("{}//{}/", protocol, host);

    let transport = WebSocketTransport::connect(&url)
        .await
        .map_err(|e| format!("Transport error: {:?}", e))?;

    let transport = Arc::new(transport);
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    let session_clone = session.clone();
    spawn_local(async move {
        if let Err(e) = session_clone.run().await {
            tracing::error!("Session error: {:?}", e);
        }
    });

    Ok(HindsightServiceClient::new(session))
}
