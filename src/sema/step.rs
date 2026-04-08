//! CIRSema — the single-walk semantic analysis pass.
//!
//! Instead of N separate passes walking the module N times, CIRSema walks
//! once and dispatches to construct-registered steps at fixed positions:
//!
//!   Comptime → Generics → Types → Ownership
//!
//! Each construct registers a step at the appropriate position. Steps share
//! state (symbol table, comptime values, etc.) so downstream steps see
//! upstream results immediately.
//!
//! Design: `claude/CIRSEMA.md` + `claude/CIRSEMA_PLAN.md`
//! References: Zig Sema (single walk), Swift SILGen (ownership insertion)

use crate::diagnostic::diagnostic::DiagnosticError;
use crate::entity::OpId;
use crate::ir::context::Context;
use crate::ir::symbol_table::SymbolTable;
use crate::pass::pass::Pass;
use crate::transform::walk::{WalkOrder, WalkResult};

/// Fixed step positions. Steps run in this order for every op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StepPosition {
    /// Evaluate comptime-known ops, fold constants, memoize comptime calls.
    Comptime = 0,
    /// Specialize generic_apply, resolve trait_call.
    Generics = 1,
    /// Type-check ops, insert casts at call boundaries.
    Types = 2,
    /// Insert copy_value/destroy_value for ARC-managed values.
    Ownership = 3,
}

const NUM_POSITIONS: usize = 4;

/// Shared state available to all sema steps during the walk.
pub struct SemaState {
    /// Module-level symbol lookup (functions, globals).
    pub symbol_table: SymbolTable,
    // Future: comptime_values, memoized_calls, branch_quota, ownership tracking
}

/// A step in the CIRSema walk. Construct crates implement this trait.
pub trait SemaStep {
    /// Name for logging and debugging.
    fn name(&self) -> &str;

    /// Which position this step runs at.
    fn position(&self) -> StepPosition;

    /// Called for each op during the walk.
    /// Return `true` if this step handled the op (skip remaining positions).
    /// Return `false` to pass to the next step.
    fn visit_op(
        &mut self,
        op: OpId,
        ctx: &mut Context,
        state: &SemaState,
    ) -> Result<bool, DiagnosticError>;

    /// Called once after the walk completes. For steps that generate new
    /// functions (test runner, witness thunks).
    fn finalize(
        &mut self,
        _module: OpId,
        _ctx: &mut Context,
        _state: &SemaState,
    ) -> Result<(), DiagnosticError> {
        Ok(())
    }
}

/// CIRSema — the single semantic analysis pass.
///
/// Implements `mlif::Pass`. The pass manager runs it like any other pass,
/// but internally it walks once and dispatches to construct steps.
pub struct CIRSema {
    steps: [Vec<Box<dyn SemaStep>>; NUM_POSITIONS],
}

impl CIRSema {
    pub fn new() -> Self {
        Self {
            steps: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        }
    }

    /// Register a sema step at its declared position.
    pub fn add_step(&mut self, step: Box<dyn SemaStep>) {
        let pos = step.position() as usize;
        self.steps[pos].push(step);
    }

    fn run_sema(&mut self, module_op: OpId, ctx: &mut Context) -> Result<(), DiagnosticError> {
        // Build symbol table from module body.
        let module_body = ctx[module_op].region(0);
        let module_block = ctx[module_body].entry_block().unwrap();
        let state = SemaState {
            symbol_table: SymbolTable::build(ctx, ctx.block_ops(module_block)),
        };

        // Snapshot all op IDs (pre-order) so steps can safely modify the IR.
        let all_ops = collect_ops(ctx, module_op);

        for op_id in all_ops {
            if !ctx.op_exists(op_id) {
                continue;
            }

            for pos in 0..NUM_POSITIONS {
                let mut handled = false;
                for step in &mut self.steps[pos] {
                    if step.visit_op(op_id, ctx, &state)? {
                        handled = true;
                        break;
                    }
                }
                if handled {
                    break;
                }
            }
        }

        // Finalize: call finalize() on every step.
        for pos in 0..NUM_POSITIONS {
            for step in &mut self.steps[pos] {
                step.finalize(module_op, ctx, &state)?;
            }
        }

        Ok(())
    }
}

impl Default for CIRSema {
    fn default() -> Self {
        Self::new()
    }
}

impl Pass for CIRSema {
    fn name(&self) -> &str {
        "cir-sema"
    }

    fn run(&mut self, module: OpId, ctx: &mut Context) -> Result<(), DiagnosticError> {
        self.run_sema(module, ctx)
    }
}

fn collect_ops(ctx: &Context, root: OpId) -> Vec<OpId> {
    let mut ops = Vec::new();
    ctx.walk(root, WalkOrder::PreOrder, &mut |op_id, _ctx| {
        ops.push(op_id);
        WalkResult::Advance
    });
    ops
}
