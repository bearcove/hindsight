#![cfg(target_arch = "wasm32")]

//! Hindsight WASM frontend using Sycamore
//!
//! Pure Rust UI that connects to Hindsight server via Rapace over WebSocket.

use std::sync::Arc;
use sycamore::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use rapace::{RpcSession, WebSocketTransport};
use hindsight_protocol::*;

mod components;

use components::*;

/// Main entry point - renders the Hindsight app
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();

    web_sys::console::log_1(&"🔍 Hindsight WASM starting...".into());

    sycamore::render(|cx| {
        view! { cx,
            App {}
        }
    });
}

/// Root application component
#[component]
fn App<G: Html>(cx: Scope) -> View<G> {
    // Connection state
    let connected = create_signal(cx, false);
    let connection_status = create_signal(cx, "Connecting...".to_string());

    // Trace data
    let traces = create_signal(cx, Vec::<TraceSummary>::new());
    let filtered_traces = create_signal(cx, Vec::<TraceSummary>::new());
    let selected_trace = create_signal(cx, Option::<Trace>::None);

    // Filters
    let service_filter = create_signal(cx, String::new());
    let type_filter = create_signal(cx, String::new());
    let min_duration = create_signal(cx, 0u64);
    let search_query = create_signal(cx, String::new());

    // Statistics
    let total_traces = create_signal(cx, 0usize);
    let shown_traces = create_signal(cx, 0usize);

    // Initialize Rapace client
    spawn_local(async move {
        match init_client().await {
            Ok(client) => {
                web_sys::console::log_1(&"✅ Connected to Hindsight via Rapace!".into());
                connected.set(true);
                connection_status.set("Connected".to_string());

                // Load initial traces
                if let Ok(trace_list) = client.list_traces(TraceFilter::default()).await {
                    traces.set(trace_list.clone());
                    filtered_traces.set(trace_list.clone());
                    total_traces.set(trace_list.len());
                    shown_traces.set(trace_list.len());
                }

                // TODO: Store client for future use
            }
            Err(e) => {
                web_sys::console::error_1(&format!("❌ Failed to connect: {:?}", e).into());
                connection_status.set("Disconnected".to_string());
            }
        }
    });

    view! { cx,
        div(class="app") {
            // Header
            header(class="header") {
                h1 {
                    span { "🔍" }
                    " Hindsight"
                }
                div(class="status-badge") {
                    div(class="status-dot") {}
                    span { (connection_status.get()) }
                }
            }

            // Main content
            div(class="content") {
                // Sidebar with filters
                aside(class="sidebar") {
                    div(class="sidebar-section") {
                        h2 { "Filters" }
                        // TODO: Filter components
                    }

                    div(class="sidebar-section") {
                        h2 { "Statistics" }
                        p { "Total Traces: " strong { (total_traces.get()) } }
                        p { "Shown: " strong { (shown_traces.get()) } }
                    }
                }

                // Main panel with trace list
                main(class="main-panel") {
                    div(class="panel-header") {
                        h2 { "Traces" }
                        button(class="btn") { "Refresh" }
                    }

                    div(class="trace-list") {
                        (if filtered_traces.get().is_empty() {
                            view! { cx,
                                div(class="empty-state") {
                                    div(class="empty-state-icon") { "📭" }
                                    div(class="empty-state-title") { "No traces found" }
                                    div(class="empty-state-text") {
                                        "Send some traces from your application to see them here."
                                    }
                                }
                            }
                        } else {
                            view! { cx,
                                Indexed(
                                    iterable=filtered_traces,
                                    view=|cx, trace| view! { cx,
                                        TraceCard(trace=trace)
                                    }
                                )
                            }
                        })
                    }
                }
            }
        }
    }
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

    web_sys::console::log_1(&format!("Connecting to {}", url).into());

    let transport = WebSocketTransport::connect(&url)
        .await
        .map_err(|e| format!("Transport error: {:?}", e))?;

    let transport = Arc::new(transport);
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    // Keep session running
    let session_clone = session.clone();
    spawn_local(async move {
        if let Err(e) = session_clone.run().await {
            web_sys::console::error_1(&format!("Session error: {:?}", e).into());
        }
    });

    let client = HindsightServiceClient::new(session);

    Ok(client)
}
