//! Protocol definitions for Hindsight distributed tracing.
//!
//! This crate defines the core types for W3C Trace Context and span representation.

mod span;
mod trace_context;

pub use span::*;
pub use trace_context::*;
