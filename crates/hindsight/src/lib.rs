//! Client library for sending spans to Hindsight tracing server.
//!
//! # Example
//!
//! ```no_run
//! use hindsight::Tracer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let tracer = Tracer::connect("http://localhost:9090").await?;
//!
//!     let span = tracer.span("my_operation")
//!         .with_attribute("key", "value")
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

pub use tracer::Tracer;
pub use span_builder::SpanBuilder;
pub use hindsight_protocol::*;
