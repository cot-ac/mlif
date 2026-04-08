//! Semantic analysis step infrastructure.
//!
//! Provides the `SemaStep` trait that construct crates implement, and
//! `CIRSema` — the single-walk pass that dispatches to registered steps.
//!
//! This lives in MLIF (not in a COT-specific crate) so construct crates
//! can depend on it without circular dependencies.

pub mod step;

pub use step::{CIRSema, SemaState, SemaStep, StepPosition};
