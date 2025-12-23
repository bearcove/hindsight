//! Navigation state management and cross-reference links
//!
//! This module provides abstractions for navigating between views and
//! managing application navigation state (active tab, selected items, etc.).

use hindsight_protocol::TraceId;
use sycamore::prelude::*;

use crate::routing::{navigate_to_route, Route};

/// Which tab is currently active
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabId {
    Traces,
    Picante,
    Rapace,
    Services,
}

impl TabId {
    /// Get the display label for this tab
    pub fn label(&self) -> &'static str {
        match self {
            TabId::Traces => "Traces",
            TabId::Picante => "Picante",
            TabId::Rapace => "Rapace",
            TabId::Services => "Services",
        }
    }

    /// Get the route for this tab's default view
    pub fn default_route(&self) -> Route {
        match self {
            TabId::Traces => Route::TraceList,
            TabId::Picante => Route::TraceList, // TODO: Default picante view?
            TabId::Rapace => Route::RapaceTopology,
            TabId::Services => Route::Services,
        }
    }

    /// Extract tab from route (which tab should be active for this route?)
    pub fn from_route(route: &Route) -> Self {
        match route {
            Route::TraceList | Route::TraceDetail { .. } | Route::TraceDetailSpan { .. } => {
                TabId::Traces
            }
            Route::PicanteGraph { .. } => TabId::Picante,
            Route::RapaceTopology => TabId::Rapace,
            Route::Services => TabId::Services,
        }
    }
}

/// Global navigation state
///
/// This struct holds all navigation-related signals and is passed down
/// through the component tree.
#[derive(Clone, Copy)]
pub struct NavigationState {
    /// Currently active tab
    pub active_tab: Signal<TabId>,
    /// Which tabs are currently available (based on discovered services)
    pub available_tabs: Signal<Vec<TabId>>,
    /// Currently selected trace (if any)
    pub selected_trace_id: Signal<Option<TraceId>>,
    /// Currently selected span within a trace (if any)
    pub selected_span_id: Signal<Option<String>>,
    /// Current route
    pub current_route: Signal<Route>,
}

impl NavigationState {
    /// Create a new navigation state with default values
    pub fn new() -> Self {
        Self {
            active_tab: create_signal(TabId::Traces),
            available_tabs: create_signal(vec![TabId::Traces]),
            selected_trace_id: create_signal(None),
            selected_span_id: create_signal(None),
            current_route: create_signal(Route::TraceList),
        }
    }

    /// Navigate to a route and update all relevant state
    pub fn navigate_to(&self, route: Route) {
        // Update selected items based on route
        match &route {
            Route::TraceList => {
                self.selected_trace_id.set(None);
                self.selected_span_id.set(None);
            }
            Route::TraceDetail { trace_id } => {
                self.selected_trace_id.set(Some(trace_id.clone()));
                self.selected_span_id.set(None);
            }
            Route::TraceDetailSpan { trace_id, span_id } => {
                self.selected_trace_id.set(Some(trace_id.clone()));
                self.selected_span_id.set(Some(span_id.clone()));
            }
            Route::PicanteGraph { trace_id } => {
                self.selected_trace_id.set(Some(trace_id.clone()));
                self.selected_span_id.set(None);
            }
            Route::RapaceTopology | Route::Services => {
                self.selected_trace_id.set(None);
                self.selected_span_id.set(None);
            }
        }

        // Update active tab
        let tab = TabId::from_route(&route);
        self.active_tab.set(tab);

        // Update current route
        self.current_route.set(route.clone());

        // Update browser URL
        navigate_to_route(&route);
    }

    /// Navigate to a tab (uses the tab's default route)
    pub fn navigate_to_tab(&self, tab: TabId) {
        let route = tab.default_route();
        self.navigate_to(route);
    }

    /// Check if a tab is currently available
    pub fn is_tab_available(&self, tab: TabId) -> bool {
        self.available_tabs.with(|tabs| tabs.contains(&tab))
    }
}

impl Default for NavigationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-reference link that can navigate between different views
///
/// This enum represents all the different types of links that can appear
/// in the UI - clicking a span in a trace should link to that span,
/// clicking a Picante node should link to the trace/span that node represents, etc.
#[derive(Clone, Debug)]
pub enum HindsightLink {
    /// Link to a specific trace
    Trace { trace_id: TraceId },
    /// Link to a specific span within a trace
    TraceSpan { trace_id: TraceId, span_id: String },
    /// Link to Picante graph for a trace
    PicanteGraph { trace_id: TraceId },
    /// Link to a specific node in a Picante graph
    PicanteNode {
        trace_id: TraceId,
        node_span_id: String,
    },
    /// Link to Rapace topology view
    RapaceTopology,
    /// Link to a specific Rapace session (future)
    RapaceSession { session_id: u64 },
    /// Link to services view
    Services,
}

impl HindsightLink {
    /// Convert link to a URL hash
    pub fn to_hash(&self) -> String {
        let route = match self {
            HindsightLink::Trace { trace_id } => Route::TraceDetail {
                trace_id: trace_id.clone(),
            },
            HindsightLink::TraceSpan { trace_id, span_id } => Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
            },
            HindsightLink::PicanteGraph { trace_id } => Route::PicanteGraph {
                trace_id: trace_id.clone(),
            },
            HindsightLink::PicanteNode {
                trace_id,
                node_span_id,
            } => {
                // Navigate to trace detail and highlight the span
                Route::TraceDetailSpan {
                    trace_id: trace_id.clone(),
                    span_id: node_span_id.clone(),
                }
            }
            HindsightLink::RapaceTopology => Route::RapaceTopology,
            HindsightLink::RapaceSession { .. } => {
                // TODO: Once we have session detail view
                Route::RapaceTopology
            }
            HindsightLink::Services => Route::Services,
        };
        format!("#{}", route.to_hash())
    }

    /// Navigate to this link by updating navigation state
    pub fn navigate(&self, nav_state: &NavigationState) {
        let route = match self {
            HindsightLink::Trace { trace_id } => Route::TraceDetail {
                trace_id: trace_id.clone(),
            },
            HindsightLink::TraceSpan { trace_id, span_id } => Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
            },
            HindsightLink::PicanteGraph { trace_id } => Route::PicanteGraph {
                trace_id: trace_id.clone(),
            },
            HindsightLink::PicanteNode {
                trace_id,
                node_span_id,
            } => Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: node_span_id.clone(),
            },
            HindsightLink::RapaceTopology => Route::RapaceTopology,
            HindsightLink::RapaceSession { .. } => Route::RapaceTopology,
            HindsightLink::Services => Route::Services,
        };
        nav_state.navigate_to(route);
    }
}
