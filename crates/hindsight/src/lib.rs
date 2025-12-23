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

mod span_builder;
mod tracer;

pub use hindsight_protocol::*;
pub use span_builder::{ActiveSpan, IntoAttributeValue, SpanBuilder};
pub use tracer::{Tracer, TracerError};
