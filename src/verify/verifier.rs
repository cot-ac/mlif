use std::collections::HashSet;

use crate::diagnostic::diagnostic::{Diagnostic, DiagnosticError};
use crate::entity::{OpId, ValueId};
use crate::ir::context::Context;
use crate::transform::walk::WalkOrder;

/// Verify the structural integrity of an operation tree rooted at `op`.
///
/// Checks:
/// - No SSA value is defined more than once
/// - All operands reference values that are in scope (defined in a
///   dominating position within the same region or an enclosing region)
/// - Nested regions and blocks are structurally sound
pub fn verify(ctx: &Context, op: OpId) -> Result<(), DiagnosticError> {
    let mut errors = Vec::new();

    // Collect all defined values in the tree to detect duplicates.
    let mut all_defs: HashSet<ValueId> = HashSet::new();

    ctx.walk(op, WalkOrder::PreOrder, &mut |op_id, ctx| {
        // Check results: no duplicate ValueIds.
        for &result in ctx[op_id].results() {
            if !all_defs.insert(result) {
                errors.push(Diagnostic::error(
                    ctx[op_id].location().clone(),
                    format!("SSA violation: value {} defined more than once", result),
                ));
            }
        }

        // Check nested blocks' arguments.
        for &region in ctx[op_id].regions() {
            for &block in ctx[region].blocks() {
                for &arg in ctx[block].arguments() {
                    if !all_defs.insert(arg) {
                        errors.push(Diagnostic::error(
                            ctx[op_id].location().clone(),
                            format!(
                                "SSA violation: block argument {} defined more than once",
                                arg
                            ),
                        ));
                    }
                }
            }
        }

        crate::transform::walk::WalkResult::Advance
    });

    // Second pass: verify all operands reference defined values.
    ctx.walk(op, WalkOrder::PreOrder, &mut |op_id, ctx| {
        for &operand in ctx[op_id].operands() {
            if !all_defs.contains(&operand) {
                errors.push(Diagnostic::error(
                    ctx[op_id].location().clone(),
                    format!(
                        "use of undefined value {} in '{}'",
                        operand,
                        ctx[op_id].name()
                    ),
                ));
            }
        }
        crate::transform::walk::WalkResult::Advance
    });

    // Third pass: verify use-def chain consistency.
    ctx.walk(op, WalkOrder::PreOrder, &mut |op_id, ctx| {
        for (i, &operand) in ctx[op_id].operands().iter().enumerate() {
            let has_use = ctx[operand]
                .uses()
                .iter()
                .any(|u| u.user == op_id && u.operand_index == i as u32);
            if !has_use {
                errors.push(Diagnostic::error(
                    ctx[op_id].location().clone(),
                    format!(
                        "use-def chain inconsistency: op {} uses {} at index {} but value's use list doesn't contain it",
                        op_id, operand, i
                    ),
                ));
            }
        }
        crate::transform::walk::WalkResult::Advance
    });

    if errors.is_empty() {
        Ok(())
    } else {
        Err(DiagnosticError {
            diagnostics: errors,
        })
    }
}

/// Verify that blocks within function bodies end with a terminator.
pub fn verify_terminators(
    ctx: &Context,
    op: OpId,
    terminator_names: &HashSet<String>,
) -> Result<(), DiagnosticError> {
    let mut errors = Vec::new();
    check_terminators(ctx, op, terminator_names, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DiagnosticError {
            diagnostics: errors,
        })
    }
}

fn check_terminators(
    ctx: &Context,
    op: OpId,
    terminator_names: &HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    let data = &ctx[op];

    for &region in data.regions() {
        for &block in ctx[region].blocks() {
            // Only check non-empty blocks inside non-module operations.
            if !ctx[block].is_empty() && !data.is_a("builtin.module") {
                if let Some(last_op) = ctx[block].last_op() {
                    if !terminator_names.contains(ctx[last_op].name()) {
                        errors.push(Diagnostic::error(
                            ctx[last_op].location().clone(),
                            format!(
                                "block does not end with a terminator: last op is '{}'",
                                ctx[last_op].name()
                            ),
                        ));
                    }
                }
            }

            // Recurse into nested operations.
            for inner_op in ctx.block_ops(block) {
                check_terminators(ctx, inner_op, terminator_names, errors);
            }
        }
    }
}
