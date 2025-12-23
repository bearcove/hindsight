use facet::Facet;
use std::fmt;

/// 16-byte trace ID (128 bits)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Facet)]
pub struct TraceId(pub [u8; 16]);

impl TraceId {
    /// Generate a new random trace ID
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).expect("failed to generate random trace ID");
        Self(bytes)
    }

    /// Parse from hex string (W3C format: 32 hex chars)
    pub fn from_hex(s: &str) -> Result<Self, TraceContextError> {
        if s.len() != 32 {
            return Err(TraceContextError::InvalidLength);
        }
        let bytes = hex::decode(s).map_err(|_| TraceContextError::InvalidHex)?;
        Ok(Self(bytes.try_into().unwrap()))
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceId({})", self.to_hex())
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

/// 8-byte span ID (64 bits)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Facet)]
pub struct SpanId(pub [u8; 8]);

impl SpanId {
    /// Generate a new random span ID
    pub fn new() -> Self {
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).expect("failed to generate random span ID");
        Self(bytes)
    }

    /// Parse from hex string (W3C format: 16 hex chars)
    pub fn from_hex(s: &str) -> Result<Self, TraceContextError> {
        if s.len() != 16 {
            return Err(TraceContextError::InvalidLength);
        }
        let bytes = hex::decode(s).map_err(|_| TraceContextError::InvalidHex)?;
        Ok(Self(bytes.try_into().unwrap()))
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpanId({})", self.to_hex())
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

/// W3C traceparent header: "00-{trace_id}-{span_id}-{flags}"
#[derive(Clone, Debug, Facet)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub flags: u8,
}

impl TraceContext {
    /// Create a new root trace context
    pub fn new_root() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            parent_span_id: None,
            flags: 0x01, // Sampled
        }
    }

    /// Create a child span in the same trace
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: SpanId::new(),
            parent_span_id: Some(self.span_id),
            flags: self.flags,
        }
    }

    /// Parse from W3C traceparent header
    pub fn from_traceparent(header: &str) -> Result<Self, TraceContextError> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return Err(TraceContextError::InvalidFormat);
        }

        if parts[0] != "00" {
            return Err(TraceContextError::UnsupportedVersion);
        }

        let trace_id = TraceId::from_hex(parts[1])?;
        let span_id = SpanId::from_hex(parts[2])?;
        let flags = u8::from_str_radix(parts[3], 16).map_err(|_| TraceContextError::InvalidHex)?;

        Ok(Self {
            trace_id,
            span_id,
            parent_span_id: None,
            flags,
        })
    }

    /// Format as W3C traceparent header
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.span_id.to_hex(),
            self.flags
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TraceContextError {
    #[error("invalid traceparent format")]
    InvalidFormat,
    #[error("unsupported trace context version")]
    UnsupportedVersion,
    #[error("invalid hex encoding")]
    InvalidHex,
    #[error("invalid length")]
    InvalidLength,
}
