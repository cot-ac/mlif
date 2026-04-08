use super::pass::Pass;
use crate::diagnostic::diagnostic::DiagnosticError;
use crate::entity::OpId;
use crate::ir::context::Context;
use crate::verify::verifier;

/// Manages an ordered sequence of passes. Runs verification after each pass.
pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
    verify_after_each: bool,
}

impl PassManager {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            verify_after_each: true,
        }
    }

    /// Add a pass to the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn Pass>) {
        self.passes.push(pass);
    }

    /// Enable or disable verification after each pass.
    pub fn set_verify_after_each(&mut self, verify: bool) {
        self.verify_after_each = verify;
    }

    /// Run all passes on the given top-level operation in order.
    pub fn run(&mut self, op: OpId, ctx: &mut Context) -> Result<(), DiagnosticError> {
        for pass in &mut self.passes {
            pass.run(op, ctx)?;
            if self.verify_after_each {
                verifier::verify(ctx, op)?;
            }
        }
        Ok(())
    }

    pub fn num_passes(&self) -> usize {
        self.passes.len()
    }
}

impl Default for PassManager {
    fn default() -> Self {
        Self::new()
    }
}
