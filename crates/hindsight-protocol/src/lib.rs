//! Protocol definitions for Hindsight distributed tracing.
//!
//! This crate defines the core types for W3C Trace Context and span representation.

pub mod events;
pub mod service;
pub mod span;
pub mod trace_context;

pub use events::*;
pub use service::*;
pub use span::*;
pub use trace_context::*;
