//! Hash-based routing for Hindsight UI
//!
//! This module provides lightweight client-side routing using URL hash fragments.
//! No external router dependencies - just simple string parsing and browser APIs.

use hindsight_protocol::TraceId;

/// Application routes
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// Main trace list view
    TraceList,
    /// Single trace detail with waterfall
    TraceDetail { trace_id: TraceId },
    /// Single trace detail focused on a specific span
    TraceDetailSpan { trace_id: TraceId, span_id: String },
    /// Picante query graph visualization
    PicanteGraph { trace_id: TraceId },
    /// Rapace topology view
    RapaceTopology,
    /// Services overview
    Services,
}

impl Route {
    /// Convert route to URL hash fragment (without leading #)
    pub fn to_hash(&self) -> String {
        match self {
            Route::TraceList => String::new(),
            Route::TraceDetail { trace_id } => {
                format!("traces/{}", trace_id.to_hex())
            }
            Route::TraceDetailSpan { trace_id, span_id } => {
                format!("traces/{}/spans/{}", trace_id.to_hex(), span_id)
            }
            Route::PicanteGraph { trace_id } => {
                format!("picante/{}", trace_id.to_hex())
            }
            Route::RapaceTopology => "rapace".to_string(),
            Route::Services => "services".to_string(),
        }
    }

    /// Parse URL hash fragment into route
    /// Hash should not include the leading # character
    pub fn from_hash(hash: &str) -> Self {
        // Remove leading # if present (shouldn't happen but defensive)
        let hash = hash.strip_prefix('#').unwrap_or(hash);

        // Empty hash = trace list
        if hash.is_empty() {
            return Route::TraceList;
        }

        // Split into segments
        let segments: Vec<&str> = hash.split('/').collect();

        match segments.as_slice() {
            ["traces"] => Route::TraceList,
            ["traces", trace_id] => {
                if let Ok(trace_id) = TraceId::from_hex(trace_id) {
                    Route::TraceDetail { trace_id }
                } else {
                    // Invalid trace ID, fall back to list
                    Route::TraceList
                }
            }
            ["traces", trace_id, "spans", span_id] => {
                if let Ok(trace_id) = TraceId::from_hex(trace_id) {
                    Route::TraceDetailSpan {
                        trace_id,
                        span_id: span_id.to_string(),
                    }
                } else {
                    Route::TraceList
                }
            }
            ["picante", trace_id] => {
                if let Ok(trace_id) = TraceId::from_hex(trace_id) {
                    Route::PicanteGraph { trace_id }
                } else {
                    Route::TraceList
                }
            }
            ["rapace"] => Route::RapaceTopology,
            ["services"] => Route::Services,
            _ => {
                // Unknown route, default to trace list
                Route::TraceList
            }
        }
    }
}

/// Get current route from browser location
pub fn get_current_route() -> Route {
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let hash = location.hash().unwrap_or_default();
    Route::from_hash(&hash)
}

/// Navigate to a route by updating the URL hash
pub fn navigate_to_route(route: &Route) {
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let hash = route.to_hash();

    // Set hash (browser adds # automatically)
    if hash.is_empty() {
        // For empty hash (trace list), use replace_hash to avoid adding #
        let _ = location.set_hash("");
    } else {
        let _ = location.set_hash(&hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_to_hash() {
        let trace_id = TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap();

        assert_eq!(Route::TraceList.to_hash(), "");
        assert_eq!(
            Route::TraceDetail {
                trace_id: trace_id.clone()
            }
            .to_hash(),
            "traces/0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: "span123".to_string()
            }
            .to_hash(),
            "traces/0123456789abcdef0123456789abcdef/spans/span123"
        );
        assert_eq!(
            Route::PicanteGraph {
                trace_id: trace_id.clone()
            }
            .to_hash(),
            "picante/0123456789abcdef0123456789abcdef"
        );
        assert_eq!(Route::RapaceTopology.to_hash(), "rapace");
        assert_eq!(Route::Services.to_hash(), "services");
    }

    #[test]
    fn test_route_from_hash() {
        let trace_id = TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap();

        assert_eq!(Route::from_hash(""), Route::TraceList);
        assert_eq!(Route::from_hash("#"), Route::TraceList);
        assert_eq!(Route::from_hash("traces"), Route::TraceList);
        assert_eq!(
            Route::from_hash("traces/0123456789abcdef0123456789abcdef"),
            Route::TraceDetail {
                trace_id: trace_id.clone()
            }
        );
        assert_eq!(
            Route::from_hash("traces/0123456789abcdef0123456789abcdef/spans/span123"),
            Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: "span123".to_string()
            }
        );
        assert_eq!(
            Route::from_hash("picante/0123456789abcdef0123456789abcdef"),
            Route::PicanteGraph {
                trace_id: trace_id.clone()
            }
        );
        assert_eq!(Route::from_hash("rapace"), Route::RapaceTopology);
        assert_eq!(Route::from_hash("services"), Route::Services);

        // Invalid cases should fall back to TraceList
        assert_eq!(Route::from_hash("invalid"), Route::TraceList);
        assert_eq!(Route::from_hash("traces/invalid-id"), Route::TraceList);
    }

    #[test]
    fn test_roundtrip() {
        let trace_id = TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap();

        let routes = vec![
            Route::TraceList,
            Route::TraceDetail {
                trace_id: trace_id.clone(),
            },
            Route::TraceDetailSpan {
                trace_id: trace_id.clone(),
                span_id: "span123".to_string(),
            },
            Route::PicanteGraph {
                trace_id: trace_id.clone(),
            },
            Route::RapaceTopology,
            Route::Services,
        ];

        for route in routes {
            let hash = route.to_hash();
            let parsed = Route::from_hash(&hash);
            assert_eq!(route, parsed, "Roundtrip failed for route: {:?}", route);
        }
    }
}
