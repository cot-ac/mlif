use crate::diagnostic::diagnostic::DiagnosticError;
use crate::entity::OpId;
use crate::ir::context::Context;

/// Verify that an operation's operands and results all have the same type.
pub fn verify_same_operands_and_result_type(
    ctx: &Context,
    op: OpId,
) -> Result<(), DiagnosticError> {
    let data = &ctx[op];
    let mut all_types = Vec::new();
    for &operand in data.operands() {
        all_types.push(ctx.value_type(operand));
    }
    for &result in data.results() {
        all_types.push(ctx.value_type(result));
    }
    if all_types.len() > 1 {
        let first = all_types[0];
        for (i, &ty) in all_types.iter().enumerate().skip(1) {
            if ty != first {
                return Err(DiagnosticError::single(
                    data.location().clone(),
                    format!(
                        "type mismatch in '{}': value {} has {}, expected {}",
                        data.name(),
                        i,
                        ctx.format_type(ty),
                        ctx.format_type(first),
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Verify that an operation has exactly the expected number of operands.
pub fn verify_num_operands(
    ctx: &Context,
    op: OpId,
    expected: usize,
) -> Result<(), DiagnosticError> {
    let data = &ctx[op];
    if data.num_operands() != expected {
        return Err(DiagnosticError::single(
            data.location().clone(),
            format!(
                "'{}' expects {} operands, got {}",
                data.name(),
                expected,
                data.num_operands()
            ),
        ));
    }
    Ok(())
}

/// Verify that an operation has exactly the expected number of results.
pub fn verify_num_results(
    ctx: &Context,
    op: OpId,
    expected: usize,
) -> Result<(), DiagnosticError> {
    let data = &ctx[op];
    if data.num_results() != expected {
        return Err(DiagnosticError::single(
            data.location().clone(),
            format!(
                "'{}' expects {} results, got {}",
                data.name(),
                expected,
                data.num_results()
            ),
        ));
    }
    Ok(())
}
