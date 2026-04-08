use std::ffi::c_void;

use super::pass::Pass;
use crate::diagnostic::diagnostic::DiagnosticError;
use crate::entity::OpId;
use crate::ir::context::Context;

/// C ABI callbacks for an external pass (non-Rust passes).
#[repr(C)]
pub struct ExternalPassCallbacks {
    pub run: extern "C" fn(ctx: *mut Context, op: OpId, user_data: *mut c_void),
    pub user_data: *mut c_void,
}

unsafe impl Send for ExternalPassCallbacks {}
unsafe impl Sync for ExternalPassCallbacks {}

/// A pass implemented via C ABI callbacks. Allows Zig, Go, C++ to provide
/// pass implementations that integrate with MLIF's pass manager.
pub struct ExternalPass {
    name: String,
    callbacks: ExternalPassCallbacks,
}

impl ExternalPass {
    pub fn new(name: impl Into<String>, callbacks: ExternalPassCallbacks) -> Self {
        Self {
            name: name.into(),
            callbacks,
        }
    }
}

impl Pass for ExternalPass {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, op: OpId, ctx: &mut Context) -> Result<(), DiagnosticError> {
        (self.callbacks.run)(ctx as *mut Context, op, self.callbacks.user_data);
        Ok(())
    }
}

/// Create an external pass from C ABI callbacks.
pub fn create_external_pass(name: &str, callbacks: ExternalPassCallbacks) -> Box<dyn Pass> {
    Box::new(ExternalPass::new(name, callbacks))
}
