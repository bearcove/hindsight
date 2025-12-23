use hindsight_protocol::*;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

/// Builder for creating and starting spans
pub struct SpanBuilder {
    name: String,
    service_name: String,
    attributes: BTreeMap<String, AttributeValue>,
    parent: Option<TraceContext>,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl SpanBuilder {
    pub(crate) fn new(
        name: String,
        service_name: String,
        span_tx: mpsc::UnboundedSender<Span>,
    ) -> Self {
        Self {
            name,
            service_name,
            attributes: BTreeMap::new(),
            parent: None,
            span_tx,
        }
    }

    /// Set the parent trace context (for propagation)
    pub fn with_parent(mut self, parent: TraceContext) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Add an attribute to the span
    pub fn with_attribute(
        mut self,
        key: impl Into<String>,
        value: impl IntoAttributeValue,
    ) -> Self {
        self.attributes
            .insert(key.into(), value.into_attribute_value());
        self
    }

    /// Start the span
    pub fn start(self) -> ActiveSpan {
        let context = if let Some(parent) = self.parent {
            parent.child()
        } else {
            TraceContext::new_root()
        };

        let span = Span {
            trace_id: context.trace_id,
            span_id: context.span_id,
            parent_span_id: context.parent_span_id,
            name: self.name,
            start_time: Timestamp::now(),
            end_time: None,
            attributes: self.attributes,
            events: Vec::new(),
            status: SpanStatus::Ok,
            service_name: self.service_name,
        };

        ActiveSpan {
            span,
            context,
            span_tx: self.span_tx,
        }
    }
}

/// Active span (not yet finished)
pub struct ActiveSpan {
    span: Span,
    context: TraceContext,
    span_tx: mpsc::UnboundedSender<Span>,
}

impl ActiveSpan {
    /// Get the trace context (for propagation to downstream calls)
    pub fn context(&self) -> &TraceContext {
        &self.context
    }

    /// Add an event to the span
    pub fn add_event(&mut self, name: impl Into<String>) {
        self.span.events.push(SpanEvent {
            name: name.into(),
            timestamp: Timestamp::now(),
            attributes: BTreeMap::new(),
        });
    }

    /// Mark the span as errored
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.span.status = SpanStatus::Error {
            message: message.into(),
        };
    }

    /// End the span and send it to the server
    pub fn end(mut self) {
        self.span.end_time = Some(Timestamp::now());
        let _ = self.span_tx.send(self.span);
    }
}

// Helper trait for converting to AttributeValue
pub trait IntoAttributeValue {
    fn into_attribute_value(self) -> AttributeValue;
}

impl IntoAttributeValue for &str {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::String(self.to_string())
    }
}

impl IntoAttributeValue for String {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::String(self)
    }
}

impl IntoAttributeValue for i64 {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Int(self)
    }
}

impl IntoAttributeValue for i32 {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Int(self as i64)
    }
}

impl IntoAttributeValue for bool {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Bool(self)
    }
}

impl IntoAttributeValue for f64 {
    fn into_attribute_value(self) -> AttributeValue {
        AttributeValue::Float(self)
    }
}

impl IntoAttributeValue for AttributeValue {
    fn into_attribute_value(self) -> AttributeValue {
        self
    }
}
