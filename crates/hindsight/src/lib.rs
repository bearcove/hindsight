//! Client library for sending spans to Hindsight tracing server.
//!
//! # Example
//!
//! ```no_run
//! use hindsight::Tracer;
//! use rapace::transport::StreamTransport;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Connect via TCP
//!     let transport = StreamTransport::connect("localhost:9090").await?;
//!     let tracer = Tracer::new(transport).await?;
//!
//!     let span = tracer.span("my_operation")
//!         .with_attribute("user_id", 123)
//!         .with_attribute("endpoint", "/api/users")
//!         .start();
//!
//!     // Do work...
//!
//!     span.end();
//!     Ok(())
//! }
//! ```

mod tracer;
mod span_builder;

pub use tracer::{Tracer, TracerError};
pub use span_builder::{SpanBuilder, ActiveSpan, IntoAttributeValue};
pub use hindsight_protocol::*;
