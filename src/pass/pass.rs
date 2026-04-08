use crate::diagnostic::diagnostic::DiagnosticError;
use crate::entity::OpId;
use crate::ir::context::Context;

/// A compiler pass that transforms or analyzes IR.
pub trait Pass {
    /// The name of this pass (for logging and debugging).
    fn name(&self) -> &str;

    /// Run this pass on the given top-level operation (usually a module).
    fn run(&mut self, op: OpId, ctx: &mut Context) -> Result<(), DiagnosticError>;
}
