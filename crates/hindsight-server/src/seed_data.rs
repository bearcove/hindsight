//! Seed data for UI development
//!
//! Generates realistic trace data with various characteristics to aid in
//! designing and testing the UI without needing a running client.

use hindsight_protocol::*;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::TraceStore;

/// Load seed traces into the store
pub fn load_seed_data(store: &TraceStore) {
    let traces = generate_seed_traces();

    for trace in traces {
        // Ingest all spans from this trace
        store.ingest(trace.spans);
    }
}

/// Helper to create string attributes
fn attr_str(key: &str, value: &str) -> (String, AttributeValue) {
    (key.to_string(), AttributeValue::String(value.to_string()))
}

/// Helper to create int attributes
fn attr_int(key: &str, value: i64) -> (String, AttributeValue) {
    (key.to_string(), AttributeValue::Int(value))
}

/// Helper to create bool attributes
fn attr_bool(key: &str, value: bool) -> (String, AttributeValue) {
    (key.to_string(), AttributeValue::Bool(value))
}

/// Generate a variety of realistic traces
fn generate_seed_traces() -> Vec<Trace> {
    let mut traces = Vec::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    // 1. Fast successful HTTP request (2 spans)
    {
        let trace_id = TraceId::from_hex("a1b2c3d4e5f6789012345678901234ab").unwrap();
        let start = Timestamp(now - 50_000_000);
        let mut spans = vec![];

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1234567890abcdef").unwrap(),
            parent_span_id: None,
            name: "GET /api/users".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 12_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_str("http.url", "/api/users"),
                attr_int("http.status_code", 200),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("abcdef1234567890").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1234567890abcdef").unwrap()),
            name: "db.query users".to_string(),
            start_time: Timestamp(start.0 + 2_000_000),
            end_time: Some(Timestamp(start.0 + 10_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "postgresql"),
                attr_str("db.statement", "SELECT * FROM users LIMIT 10"),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 2. Slow request with database lock (2 spans, one with event)
    {
        let trace_id = TraceId::from_hex("deadbeef12345678901234567890abcd").unwrap();
        let start = Timestamp(now - 2_500_000_000);
        let mut spans = vec![];

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("fedcba9876543210").unwrap(),
            parent_span_id: None,
            name: "POST /api/orders".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 2_345_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "POST"),
                attr_str("http.url", "/api/orders"),
                attr_int("http.status_code", 200),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "order-service".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1111222233334444").unwrap(),
            parent_span_id: Some(SpanId::from_hex("fedcba9876543210").unwrap()),
            name: "db.transaction".to_string(),
            start_time: Timestamp(start.0 + 50_000_000),
            end_time: Some(Timestamp(start.0 + 2_340_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "postgresql"),
                attr_str("db.operation", "INSERT"),
            ]),
            events: vec![SpanEvent {
                name: "Waiting for lock".to_string(),
                timestamp: Timestamp(start.0 + 100_000_000),
                attributes: BTreeMap::from([attr_str("lock.type", "ROW EXCLUSIVE")]),
            }],
            status: SpanStatus::Ok,
            service_name: "order-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 3. Failed request with error
    {
        let trace_id = TraceId::from_hex("e440e404e440e404e440e404e440e404").unwrap();
        let start = Timestamp(now - 15_000_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("5555666677778888").unwrap(),
            parent_span_id: None,
            name: "GET /api/user/999".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 8_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_str("http.url", "/api/user/999"),
                attr_int("http.status_code", 404),
                attr_bool("error", true),
                attr_str("error.message", "User not found"),
            ]),
            events: vec![SpanEvent {
                name: "exception".to_string(),
                timestamp: Timestamp(start.0 + 5_000_000),
                attributes: BTreeMap::from([
                    attr_str("exception.type", "UserNotFoundException"),
                    attr_str("exception.message", "No user with ID 999"),
                ]),
            }],
            status: SpanStatus::Error {
                message: "User not found".to_string(),
            },
            service_name: "user-service".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 4. Complex nested trace with multiple services (5 spans)
    {
        let trace_id = TraceId::from_hex("c0a10000c0a10000c0a10000c0a10000").unwrap();
        let start = Timestamp(now - 500_000_000);
        let mut spans = vec![];

        // Root span
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1111000000000001").unwrap(),
            parent_span_id: None,
            name: "POST /api/checkout".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 485_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "POST"),
                attr_str("http.url", "/api/checkout"),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        });

        // Child spans
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("2222000000000002").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1111000000000001").unwrap()),
            name: "validate_cart".to_string(),
            start_time: Timestamp(start.0 + 5_000_000),
            end_time: Some(Timestamp(start.0 + 50_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "cart-service".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("3333000000000003").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1111000000000001").unwrap()),
            name: "check_inventory".to_string(),
            start_time: Timestamp(start.0 + 55_000_000),
            end_time: Some(Timestamp(start.0 + 175_000_000)),
            attributes: BTreeMap::from([attr_int("items.checked", 3)]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "inventory-service".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("4444000000000004").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1111000000000001").unwrap()),
            name: "process_payment".to_string(),
            start_time: Timestamp(start.0 + 180_000_000),
            end_time: Some(Timestamp(start.0 + 460_000_000)),
            attributes: BTreeMap::from([
                attr_str("payment.provider", "stripe"),
                attr_str("payment.amount", "99.99"),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "payment-service".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("5555000000000005").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1111000000000001").unwrap()),
            name: "create_order".to_string(),
            start_time: Timestamp(start.0 + 455_000_000),
            end_time: Some(Timestamp(start.0 + 485_000_000)),
            attributes: BTreeMap::from([attr_str("order.id", "ORD-12345")]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "order-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 5. Very fast cache hit
    {
        let trace_id = TraceId::from_hex("cac0e000cac0e000cac0e000cac0e000").unwrap();
        let start = Timestamp(now - 2_000_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("cafebabe12345678").unwrap(),
            parent_span_id: None,
            name: "GET /api/config".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 800_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_bool("cache.hit", true),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "config-service".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 6. Medium complexity query
    {
        let trace_id = TraceId::from_hex("000e0000000e0000000e0000000e0000").unwrap();
        let start = Timestamp(now - 180_000_000);
        let mut spans = vec![];

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("aaaa111122223333").unwrap(),
            parent_span_id: None,
            name: "GET /api/search".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 175_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_str("search.query", "laptop"),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "search-service".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("bbbb111122223333").unwrap(),
            parent_span_id: Some(SpanId::from_hex("aaaa111122223333").unwrap()),
            name: "db.query products".to_string(),
            start_time: Timestamp(start.0 + 5_000_000),
            end_time: Some(Timestamp(start.0 + 170_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "elasticsearch"),
                attr_int("results.count", 342),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "search-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 7. Another error case - timeout
    {
        let trace_id = TraceId::from_hex("00e0000000e0000000e0000000e00000").unwrap();
        let start = Timestamp(now - 5_100_000_000);
        let mut spans = vec![];

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a2b3c4d5e6f7890").unwrap(),
            parent_span_id: None,
            name: "GET /api/external".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 5_050_000_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_int("http.status_code", 504),
            ]),
            events: vec![],
            status: SpanStatus::Error {
                message: "Gateway timeout".to_string(),
            },
            service_name: "api-gateway".to_string(),
        });

        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("2b3c4d5e6f7890a1").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1a2b3c4d5e6f7890").unwrap()),
            name: "http.call external-api".to_string(),
            start_time: Timestamp(start.0 + 10_000_000),
            end_time: Some(Timestamp(start.0 + 5_040_000_000)),
            attributes: BTreeMap::from([attr_str("http.url", "https://external-api.example.com")]),
            events: vec![SpanEvent {
                name: "timeout".to_string(),
                timestamp: Timestamp(start.0 + 5_000_000_000),
                attributes: BTreeMap::from([attr_str("timeout.duration", "5s")]),
            }],
            status: SpanStatus::Error {
                message: "Request timeout after 5s".to_string(),
            },
            service_name: "api-gateway".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 8. Batch processing trace
    {
        let trace_id = TraceId::from_hex("ba0c0000ba0c0000ba0c0000ba0c0000").unwrap();
        let start = Timestamp(now - 850_000_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("9999aaaabbbbcccc").unwrap(),
            parent_span_id: None,
            name: "process_batch".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 820_000_000)),
            attributes: BTreeMap::from([
                attr_str("batch.type", "email"),
                attr_int("batch.size", 1500),
                attr_int("batch.processed", 1500),
            ]),
            events: vec![SpanEvent {
                name: "checkpoint".to_string(),
                timestamp: Timestamp(start.0 + 400_000_000),
                attributes: BTreeMap::from([attr_int("processed", 750)]),
            }],
            status: SpanStatus::Ok,
            service_name: "batch-processor".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 9. Deep nesting with error at bottom (8 levels)
    {
        let trace_id = TraceId::from_hex("de400000de400000de400000de400000").unwrap();
        let start = Timestamp(now - 1_200_000_000);
        let mut spans = vec![];

        // Root: api-gateway
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d111111111111111").unwrap(),
            parent_span_id: None,
            name: "GET /api/report".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 1_180_000_000)),
            attributes: BTreeMap::from([attr_str("http.method", "GET")]),
            events: vec![],
            status: SpanStatus::Error {
                message: "Child operation failed".to_string(),
            },
            service_name: "api-gateway".to_string(),
        });

        // Level 1: report-service
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d222222222222222").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d111111111111111").unwrap()),
            name: "generate_report".to_string(),
            start_time: Timestamp(start.0 + 10_000_000),
            end_time: Some(Timestamp(start.0 + 1_170_000_000)),
            attributes: BTreeMap::from([attr_str("report.type", "sales")]),
            events: vec![],
            status: SpanStatus::Error {
                message: "Data fetch failed".to_string(),
            },
            service_name: "report-service".to_string(),
        });

        // Level 2: data-aggregator
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d333333333333333").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d222222222222222").unwrap()),
            name: "aggregate_data".to_string(),
            start_time: Timestamp(start.0 + 50_000_000),
            end_time: Some(Timestamp(start.0 + 1_160_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Error {
                message: "Query failed".to_string(),
            },
            service_name: "data-aggregator".to_string(),
        });

        // Level 3: query-engine
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d444444444444444").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d333333333333333").unwrap()),
            name: "execute_query".to_string(),
            start_time: Timestamp(start.0 + 100_000_000),
            end_time: Some(Timestamp(start.0 + 1_150_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Error {
                message: "Connection failed".to_string(),
            },
            service_name: "query-engine".to_string(),
        });

        // Level 4: connection-pool
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d555555555555555").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d444444444444444").unwrap()),
            name: "get_connection".to_string(),
            start_time: Timestamp(start.0 + 120_000_000),
            end_time: Some(Timestamp(start.0 + 1_140_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Error {
                message: "Pool exhausted".to_string(),
            },
            service_name: "query-engine".to_string(),
        });

        // Level 5: db-driver
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d666666666666666").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d555555555555555").unwrap()),
            name: "db.connect".to_string(),
            start_time: Timestamp(start.0 + 150_000_000),
            end_time: Some(Timestamp(start.0 + 1_130_000_000)),
            attributes: BTreeMap::from([attr_str("db.system", "postgresql")]),
            events: vec![],
            status: SpanStatus::Error {
                message: "Timeout establishing connection".to_string(),
            },
            service_name: "query-engine".to_string(),
        });

        // Level 6: tcp-stack
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d777777777777777").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d666666666666666").unwrap()),
            name: "tcp.connect".to_string(),
            start_time: Timestamp(start.0 + 180_000_000),
            end_time: Some(Timestamp(start.0 + 1_120_000_000)),
            attributes: BTreeMap::from([attr_str("peer.address", "10.0.1.5:5432")]),
            events: vec![],
            status: SpanStatus::Error {
                message: "Connection refused".to_string(),
            },
            service_name: "query-engine".to_string(),
        });

        // Level 7: network-layer (deepest)
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("d888888888888888").unwrap(),
            parent_span_id: Some(SpanId::from_hex("d777777777777777").unwrap()),
            name: "socket.connect".to_string(),
            start_time: Timestamp(start.0 + 200_000_000),
            end_time: Some(Timestamp(start.0 + 1_100_000_000)),
            attributes: BTreeMap::new(),
            events: vec![SpanEvent {
                name: "connection_refused".to_string(),
                timestamp: Timestamp(start.0 + 1_000_000_000),
                attributes: BTreeMap::from([attr_str("errno", "ECONNREFUSED")]),
            }],
            status: SpanStatus::Error {
                message: "ECONNREFUSED".to_string(),
            },
            service_name: "query-engine".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 10. Parallel operations (fan-out pattern)
    {
        let trace_id = TraceId::from_hex("fa00fa00fa00fa00fa00fa00fa00fa00").unwrap();
        let start = Timestamp(now - 650_000_000);
        let mut spans = vec![];

        // Root span
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("f000000000000001").unwrap(),
            parent_span_id: None,
            name: "GET /api/dashboard".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 645_000_000)),
            attributes: BTreeMap::from([attr_str("http.method", "GET")]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        });

        // Parallel fetches - all start around the same time
        let parallel_start = start.0 + 5_000_000;

        // Fetch 1: user info (fast)
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("f100000000000001").unwrap(),
            parent_span_id: Some(SpanId::from_hex("f000000000000001").unwrap()),
            name: "fetch_user_info".to_string(),
            start_time: Timestamp(parallel_start),
            end_time: Some(Timestamp(parallel_start + 45_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "user-service".to_string(),
        });

        // Fetch 2: recent orders (medium)
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("f200000000000002").unwrap(),
            parent_span_id: Some(SpanId::from_hex("f000000000000001").unwrap()),
            name: "fetch_recent_orders".to_string(),
            start_time: Timestamp(parallel_start + 2_000_000),
            end_time: Some(Timestamp(parallel_start + 320_000_000)),
            attributes: BTreeMap::from([attr_int("limit", 20)]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "order-service".to_string(),
        });

        // Fetch 3: recommendations (slow - this is the critical path)
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("f300000000000003").unwrap(),
            parent_span_id: Some(SpanId::from_hex("f000000000000001").unwrap()),
            name: "fetch_recommendations".to_string(),
            start_time: Timestamp(parallel_start + 1_000_000),
            end_time: Some(Timestamp(parallel_start + 635_000_000)),
            attributes: BTreeMap::from([
                attr_str("algo", "collaborative_filtering"),
                attr_int("candidates", 1000),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "recommendation-service".to_string(),
        });

        // Fetch 4: notifications (fast)
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("f400000000000004").unwrap(),
            parent_span_id: Some(SpanId::from_hex("f000000000000001").unwrap()),
            name: "fetch_notifications".to_string(),
            start_time: Timestamp(parallel_start + 3_000_000),
            end_time: Some(Timestamp(parallel_start + 28_000_000)),
            attributes: BTreeMap::from([attr_bool("unread_only", true)]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "notification-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 11. Authentication failure
    {
        let trace_id = TraceId::from_hex("a00000000000000000000000000000a0").unwrap();
        let start = Timestamp(now - 8_000_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("a111111111111111").unwrap(),
            parent_span_id: None,
            name: "POST /api/admin".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 3_500_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "POST"),
                attr_int("http.status_code", 403),
                attr_bool("error", true),
            ]),
            events: vec![SpanEvent {
                name: "auth_failed".to_string(),
                timestamp: Timestamp(start.0 + 2_000_000),
                attributes: BTreeMap::from([
                    attr_str("reason", "insufficient_permissions"),
                    attr_str("required_role", "admin"),
                ]),
            }],
            status: SpanStatus::Error {
                message: "Forbidden: insufficient permissions".to_string(),
            },
            service_name: "api-gateway".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 12. Very fast cache hit (sub-millisecond)
    {
        let trace_id = TraceId::from_hex("ffffff00ffffff00ffffff00ffffff00").unwrap();
        let start = Timestamp(now - 500_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("fff0000000000fff").unwrap(),
            parent_span_id: None,
            name: "GET /api/health".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 250_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "GET"),
                attr_int("http.status_code", 200),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 13. Validation error (fast failure)
    {
        let trace_id = TraceId::from_hex("ba0ba0ba0ba0ba0ba0ba0ba0ba0ba0ba").unwrap();
        let start = Timestamp(now - 12_000_000);

        let span = Span {
            trace_id,
            span_id: SpanId::from_hex("b000000000000bad").unwrap(),
            parent_span_id: None,
            name: "POST /api/user".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 5_500_000)),
            attributes: BTreeMap::from([
                attr_str("http.method", "POST"),
                attr_int("http.status_code", 400),
                attr_bool("error", true),
                attr_str("error.type", "ValidationError"),
            ]),
            events: vec![SpanEvent {
                name: "validation_failed".to_string(),
                timestamp: Timestamp(start.0 + 2_000_000),
                attributes: BTreeMap::from([
                    attr_str("field", "email"),
                    attr_str("message", "invalid email format"),
                ]),
            }],
            status: SpanStatus::Error {
                message: "Invalid request: email format invalid".to_string(),
            },
            service_name: "user-service".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    // 14. Database query with retries
    {
        let trace_id = TraceId::from_hex("4e44e444e444e444e444e444e444e444").unwrap();
        let start = Timestamp(now - 3_800_000_000);
        let mut spans = vec![];

        // Root
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("4000000000000001").unwrap(),
            parent_span_id: None,
            name: "GET /api/analytics".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 3_750_000_000)),
            attributes: BTreeMap::from([attr_str("http.method", "GET")]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "analytics-service".to_string(),
        });

        // Retry 1 - fast fail
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("4111111111111111").unwrap(),
            parent_span_id: Some(SpanId::from_hex("4000000000000001").unwrap()),
            name: "db.query".to_string(),
            start_time: Timestamp(start.0 + 10_000_000),
            end_time: Some(Timestamp(start.0 + 1_200_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "postgresql"),
                attr_int("retry.attempt", 1),
            ]),
            events: vec![SpanEvent {
                name: "deadlock_detected".to_string(),
                timestamp: Timestamp(start.0 + 1_100_000_000),
                attributes: BTreeMap::new(),
            }],
            status: SpanStatus::Error {
                message: "Deadlock detected".to_string(),
            },
            service_name: "analytics-service".to_string(),
        });

        // Retry 2 - slow fail
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("4222222222222222").unwrap(),
            parent_span_id: Some(SpanId::from_hex("4000000000000001").unwrap()),
            name: "db.query".to_string(),
            start_time: Timestamp(start.0 + 1_250_000_000),
            end_time: Some(Timestamp(start.0 + 2_450_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "postgresql"),
                attr_int("retry.attempt", 2),
            ]),
            events: vec![SpanEvent {
                name: "timeout".to_string(),
                timestamp: Timestamp(start.0 + 2_400_000_000),
                attributes: BTreeMap::new(),
            }],
            status: SpanStatus::Error {
                message: "Query timeout".to_string(),
            },
            service_name: "analytics-service".to_string(),
        });

        // Retry 3 - success
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("4333333333333333").unwrap(),
            parent_span_id: Some(SpanId::from_hex("4000000000000001").unwrap()),
            name: "db.query".to_string(),
            start_time: Timestamp(start.0 + 2_500_000_000),
            end_time: Some(Timestamp(start.0 + 3_740_000_000)),
            attributes: BTreeMap::from([
                attr_str("db.system", "postgresql"),
                attr_int("retry.attempt", 3),
                attr_int("rows.returned", 15420),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "analytics-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    // 15. Mixed services with moderate complexity
    {
        let trace_id = TraceId::from_hex("111a111a111a111a111a111a111a111a").unwrap();
        let start = Timestamp(now - 420_000_000);
        let mut spans = vec![];

        // Root
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a00000000000001").unwrap(),
            parent_span_id: None,
            name: "POST /api/cart/add".to_string(),
            start_time: start,
            end_time: Some(Timestamp(start.0 + 415_000_000)),
            attributes: BTreeMap::from([attr_str("http.method", "POST")]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "api-gateway".to_string(),
        });

        // Validate user
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a11111111111111").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1a00000000000001").unwrap()),
            name: "validate_session".to_string(),
            start_time: Timestamp(start.0 + 5_000_000),
            end_time: Some(Timestamp(start.0 + 35_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "auth-service".to_string(),
        });

        // Check product availability
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a22222222222222").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1a00000000000001").unwrap()),
            name: "check_stock".to_string(),
            start_time: Timestamp(start.0 + 40_000_000),
            end_time: Some(Timestamp(start.0 + 220_000_000)),
            attributes: BTreeMap::from([
                attr_str("product_id", "SKU-12345"),
                attr_int("quantity", 2),
            ]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "inventory-service".to_string(),
        });

        // Update cart
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a33333333333333").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1a00000000000001").unwrap()),
            name: "cart.add_item".to_string(),
            start_time: Timestamp(start.0 + 225_000_000),
            end_time: Some(Timestamp(start.0 + 410_000_000)),
            attributes: BTreeMap::new(),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "cart-service".to_string(),
        });

        // Nested: save to database
        spans.push(Span {
            trace_id,
            span_id: SpanId::from_hex("1a44444444444444").unwrap(),
            parent_span_id: Some(SpanId::from_hex("1a33333333333333").unwrap()),
            name: "db.update cart_items".to_string(),
            start_time: Timestamp(start.0 + 230_000_000),
            end_time: Some(Timestamp(start.0 + 405_000_000)),
            attributes: BTreeMap::from([attr_str("db.system", "redis")]),
            events: vec![],
            status: SpanStatus::Ok,
            service_name: "cart-service".to_string(),
        });

        if let Some(trace) = Trace::from_spans(spans) {
            traces.push(trace);
        }
    }

    traces
}
