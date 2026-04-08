//! Trait for construct-specific Cranelift lowering.
//!
//! Each construct crate implements `ConstructLowering` to provide
//! Cranelift code generation for its ops. The framework dispatches
//! to registered construct lowerings during `lower_module()`.
//!
//! This mirrors the C++ `Construct::populateLoweringPatterns()` method.

#![cfg(feature = "codegen")]

use cranelift_codegen::ir as clir;

use crate::entity::{OpId, TypeId};
use crate::ir::context::Context;

use super::lowering_ctx::LoweringCtx;

/// Trait implemented by each construct crate to provide Cranelift lowering.
///
/// Each construct handles ops it owns and returns `Ok(true)` if handled,
/// `Ok(false)` if the op doesn't belong to this construct.
pub trait ConstructLowering: Send + Sync {
    /// The construct name (e.g., "arith", "memory", "flow").
    fn name(&self) -> &str;

    /// Lower a single CIR op to Cranelift instructions.
    ///
    /// # Arguments
    /// * `op` — The CIR operation to lower
    /// * `lctx` — Lowering context with IR, builder, value/block maps, and helpers
    ///
    /// # Returns
    /// * `Ok(true)` — This construct handled the op
    /// * `Ok(false)` — This construct doesn't own this op (pass to next)
    /// * `Err(msg)` — Lowering failed
    fn lower_op(&self, op: OpId, lctx: &mut LoweringCtx) -> Result<bool, String>;

    /// Map a CIR type owned by this construct to a Cranelift type.
    ///
    /// Called by the framework when `to_cranelift_type()` encounters
    /// an ExtensionType. Return `None` if this construct doesn't own
    /// the type.
    fn map_type(&self, _ctx: &Context, _ty: TypeId) -> Option<clir::Type> {
        None
    }
}

/// Collects construct lowering implementations.
///
/// The compiler registers all construct lowerings before calling
/// `lower_module()`. The framework iterates registered lowerings
/// to dispatch each CIR op.
pub struct LoweringRegistry {
    constructs: Vec<Box<dyn ConstructLowering>>,
}

impl LoweringRegistry {
    pub fn new() -> Self {
        Self {
            constructs: Vec::new(),
        }
    }

    /// Register a construct lowering implementation.
    pub fn register(&mut self, lowering: Box<dyn ConstructLowering>) {
        self.constructs.push(lowering);
    }

    /// Get all registered construct lowerings.
    pub fn constructs(&self) -> &[Box<dyn ConstructLowering>] {
        &self.constructs
    }
}

impl Default for LoweringRegistry {
    fn default() -> Self {
        Self::new()
    }
}
