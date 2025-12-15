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

    // Initialize tracing-wasm to send Rust logs to browser console
    tracing_wasm::set_as_global_default();

    tracing::info!("🔍 Hindsight WASM starting...");

    sycamore::render(|| view! { App {} });
}

/// Root application component
#[component]
fn App() -> View {
    // Connection state
    let connected = create_signal(false);
    let connection_status = create_signal("Connecting...".to_string());

    // Trace data
    let traces = create_signal(Vec::<TraceSummary>::new());
    let filtered_traces = create_signal(Vec::<TraceSummary>::new());
    let _selected_trace = create_signal(Option::<Trace>::None);

    // Filters (TODO: hook these up to actual UI controls)
    let _service_filter = create_signal(String::new());
    let _type_filter = create_signal(String::new());
    let _min_duration = create_signal(0u64);
    let _search_query = create_signal(String::new());

    // Statistics
    let total_traces = create_signal(0usize);
    let shown_traces = create_signal(0usize);

    // Initialize Rapace client
    spawn_local(async move {
        match init_client().await {
            Ok(client) => {
                tracing::info!("✅ Connected to Hindsight via Rapace!");
                connected.set(true);
                connection_status.set("Connected".to_string());

                // Load initial traces
                tracing::info!("Requesting trace list with default filter...");
                match client.list_traces(TraceFilter::default()).await {
                    Ok(trace_list) => {
                        tracing::info!("✅ Received {} traces", trace_list.len());
                        total_traces.set(trace_list.len());
                        shown_traces.set(trace_list.len());
                        traces.set(trace_list.clone());
                        filtered_traces.set(trace_list);
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to list traces: {:?}", e);
                    }
                }

                // TODO: Store client for future use
            }
            Err(e) => {
                tracing::error!("❌ Failed to connect: {:?}", e);
                connection_status.set("Disconnected".to_string());
            }
        }
    });

    view! {
        div(class="app") {
            // Header
            header(class="header") {
                h1 {
                    span { "🔍" }
                    " Hindsight"
                }
                div(class="status-badge") {
                    div(class="status-dot") {}
                    span { (connection_status.get_clone()) }
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
                        (if filtered_traces.with(|traces| traces.is_empty()) {
                            view! {
                                div(class="empty-state") {
                                    div(class="empty-state-icon") { "📭" }
                                    div(class="empty-state-title") { "No traces found" }
                                    div(class="empty-state-text") {
                                        "Send some traces from your application to see them here."
                                    }
                                }
                            }
                        } else {
                            view! {
                                Indexed(
                                    list=filtered_traces,
                                    view=|trace| view! {
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

    tracing::info!("Connecting to {}", url);

    let transport = WebSocketTransport::connect(&url)
        .await
        .map_err(|e| format!("Transport error: {:?}", e))?;

    tracing::debug!("WebSocket transport connected");

    let transport = Arc::new(transport);
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    tracing::debug!("RPC session created with channel_start=2");

    // Keep session running
    let session_clone = session.clone();
    spawn_local(async move {
        tracing::debug!("Starting RPC session run loop");
        if let Err(e) = session_clone.run().await {
            tracing::error!("Session error: {:?}", e);
        }
    });

    let client = HindsightServiceClient::new(session);
    tracing::debug!("HindsightServiceClient created");

    Ok(client)
}
