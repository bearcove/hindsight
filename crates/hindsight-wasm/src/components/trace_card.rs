//! UI components for Hindsight

use hindsight_protocol::*;
use sycamore::prelude::*;

use crate::navigation::NavigationState;
use crate::routing::Route;

/// TraceCard component - displays a single trace summary
#[component]
pub fn TraceCard(TraceCardProps { trace, nav_state }: TraceCardProps) -> View {
    // Format duration - show sub-ms for very fast traces
    let duration_text = trace
        .duration_nanos
        .map(|nanos| {
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
        })
        .unwrap_or_else(|| "—".to_string());

    let trace_id = trace.trace_id.clone();
    let on_click = move |_| {
        nav_state.navigate_to(Route::TraceDetail {
            trace_id: trace_id.clone(),
        });
    };

    view! {
        div(
            class="trace-item",
            on:click=on_click,
            data-has-error=trace.has_errors.to_string()
        ) {
            div(class="trace-name") { (trace.root_span_name.clone()) }
            div(class="trace-duration") { (duration_text) }
            div(class="trace-meta") {
                div(class="trace-meta-item") {
                    span(class="trace-meta-label") { "svc:" }
                    span(class="trace-meta-value") { (trace.service_name.clone()) }
                }
                div(class="trace-meta-item") {
                    span(class="trace-meta-label") { "spans:" }
                    span(class="trace-meta-value") { (trace.span_count.to_string()) }
                }
                span(class="trace-type-badge") {
                    (trace.trace_type.to_string())
                }
            }
        }
    }
}

#[derive(Props)]
pub struct TraceCardProps {
    pub trace: TraceSummary,
    pub nav_state: NavigationState,
}
