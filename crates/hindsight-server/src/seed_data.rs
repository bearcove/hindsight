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
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;

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
            events: vec![
                SpanEvent {
                    name: "Waiting for lock".to_string(),
                    timestamp: Timestamp(start.0 + 100_000_000),
                    attributes: BTreeMap::from([
                        attr_str("lock.type", "ROW EXCLUSIVE"),
                    ]),
                },
            ],
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
            events: vec![
                SpanEvent {
                    name: "exception".to_string(),
                    timestamp: Timestamp(start.0 + 5_000_000),
                    attributes: BTreeMap::from([
                        attr_str("exception.type", "UserNotFoundException"),
                        attr_str("exception.message", "No user with ID 999"),
                    ]),
                },
            ],
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
            attributes: BTreeMap::from([
                attr_int("items.checked", 3),
            ]),
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
            attributes: BTreeMap::from([
                attr_str("order.id", "ORD-12345"),
            ]),
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
            attributes: BTreeMap::from([
                attr_str("http.url", "https://external-api.example.com"),
            ]),
            events: vec![
                SpanEvent {
                    name: "timeout".to_string(),
                    timestamp: Timestamp(start.0 + 5_000_000_000),
                    attributes: BTreeMap::from([
                        attr_str("timeout.duration", "5s"),
                    ]),
                },
            ],
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
            events: vec![
                SpanEvent {
                    name: "checkpoint".to_string(),
                    timestamp: Timestamp(start.0 + 400_000_000),
                    attributes: BTreeMap::from([
                        attr_int("processed", 750),
                    ]),
                },
            ],
            status: SpanStatus::Ok,
            service_name: "batch-processor".to_string(),
        };

        if let Some(trace) = Trace::from_spans(vec![span]) {
            traces.push(trace);
        }
    }

    traces
}
