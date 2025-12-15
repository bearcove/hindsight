//! UI components for Hindsight

use sycamore::prelude::*;
use hindsight_protocol::*;

/// TraceCard component - displays a single trace summary
#[component]
pub fn TraceCard<'a, G: Html>(cx: Scope<'a>, props: TraceCardProps<'a>) -> View<G> {
    let trace = props.trace;
    let duration_ms = trace.duration_nanos as f64 / 1_000_000.0;
    let type_class = format!("type-{}", trace.trace_type.to_string().to_lowercase());

    view! { cx,
        div(class="trace-card") {
            div(class="trace-header") {
                div(class="trace-name") { (trace.root_span_name.clone()) }
                div(class="trace-duration") { (format!("{:.2}ms", duration_ms)) }
            }
            div(class="trace-meta") {
                span { "🏷️ " (trace.service_name.clone()) }
                span { "📊 " (trace.span_count) " spans" }
                (if trace.error_count > 0 {
                    view! { cx,
                        span(style="color: #ef4444;") {
                            "⚠️ " (trace.error_count) " errors"
                        }
                    }
                } else {
                    view! { cx, }
                })
                span(class=format!("trace-type-badge {}", type_class)) {
                    (trace.trace_type.to_string())
                }
            }
        }
    }
}

#[derive(Props)]
pub struct TraceCardProps<'a> {
    pub trace: &'a TraceSummary,
}
