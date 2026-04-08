//! Lower CIR IR to Cranelift CLIF and produce a native object file.
//!
//! Walks the CIR module operation, translates each `func.func` into a
//! Cranelift function, emits instructions for the Gate 1 op set
//! (cir.constant, cir.add, cir.sub, cir.mul, func.return, func.call),
//! and finishes with an object file suitable for linking.

#![cfg(feature = "codegen")]

use std::collections::HashMap;

use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

use crate::entity::{BlockId, OpId, TypeId, ValueId};
use crate::ir::attributes::Attribute;
use crate::ir::context::Context;
use crate::ir::types::TypeKind;

use super::types::{is_aggregate_type, to_cranelift_type, type_byte_size};

use super::construct_lowering::LoweringRegistry;
use super::lowering_ctx::LoweringCtx;

/// Lower an entire CIR module to a native object file.
///
/// The `module_op` must be a `builtin.module` operation (or any top-level
/// container whose single region holds `func.func` operations).
///
/// `registry` provides construct-specific lowering implementations.
/// If `None`, only framework ops (func.call, func.return) are lowered.
///
/// Returns the raw bytes of a relocatable object file suitable for linking.
pub fn lower_module(
    ctx: &Context,
    module_op: OpId,
    registry: Option<&LoweringRegistry>,
) -> Result<Vec<u8>, String> {
    // --- Set up Cranelift target ---
    let shared_builder = settings::builder();
    let shared_flags = settings::Flags::new(shared_builder);
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .map_err(|e| format!("ISA lookup failed: {}", e))?
        .finish(shared_flags)
        .map_err(|e| format!("ISA finish failed: {}", e))?;

    let mut object_module = ObjectModule::new(
        ObjectBuilder::new(
            isa,
            "cir_module",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| format!("ObjectBuilder failed: {}", e))?,
    );

    // --- Collect all func.func declarations first (needed for cross-references) ---
    let module_data = &ctx[module_op];
    if module_data.regions().is_empty() {
        return Err("module operation has no regions".into());
    }

    let body_region = module_data.region(0);
    let entry_block = ctx
        .region_entry_block(body_region)
        .ok_or("module region has no entry block")?;

    // First pass: declare all functions so cross-calls can resolve.
    // Tuple: (FuncId, Signature, OpId, sret_size)
    // sret_size is Some(bytes) if the function returns an aggregate via pointer.
    let mut func_declarations: HashMap<String, (cranelift_module::FuncId, Signature, OpId, Option<u32>)> =
        HashMap::new();
    let mut func_order: Vec<String> = Vec::new();

    for op in ctx.block_ops(entry_block) {
        if ctx[op].name() != "func.func" {
            continue;
        }

        let func_name = match ctx[op].get_attribute("sym_name") {
            Some(Attribute::String(name)) => name.clone(),
            _ => return Err("func.func missing sym_name attribute".into()),
        };

        let func_type_id = match ctx[op].get_attribute("function_type") {
            Some(Attribute::Type(ty)) => *ty,
            _ => return Err(format!(
                "func.func '{}' missing function_type attribute",
                func_name
            )),
        };

        let (sig, sret_size) = build_signature(ctx, func_type_id, &object_module)?;

        // Export `main`, keep everything else local.
        let linkage = if func_name == "main" {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let func_id = object_module
            .declare_function(&func_name, linkage, &sig)
            .map_err(|e| format!("declare_function '{}': {}", func_name, e))?;

        func_declarations.insert(func_name.clone(), (func_id, sig, op, sret_size));
        func_order.push(func_name);
    }

    // Second pass: lower each function body.
    for func_name in &func_order {
        let (func_id, ref sig, func_op, sret_size) = func_declarations[func_name];

        let mut cl_ctx = cranelift_codegen::Context::new();
        cl_ctx.func = Function::with_name_signature(
            UserFuncName::user(0, func_id.as_u32()),
            sig.clone(),
        );

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut cl_ctx.func, &mut builder_ctx);

        lower_function(
            ctx,
            func_op,
            &mut builder,
            &mut object_module,
            &func_declarations,
            registry,
            sret_size,
        )?;

        builder.finalize();

        object_module
            .define_function(func_id, &mut cl_ctx)
            .map_err(|e| format!("define_function '{}': {}", func_name, e))?;
    }

    // --- Finish and emit ---
    let product = object_module.finish();
    let bytes = product.emit().map_err(|e| format!("emit failed: {}", e))?;

    Ok(bytes)
}

/// Build a Cranelift Signature from a CIR function type.
///
/// Returns `(signature, sret_size)` where `sret_size` is `Some(bytes)` if the
/// function returns an aggregate that needs the sret calling convention
/// (caller allocates, callee writes via pointer parameter).
fn build_signature(
    ctx: &Context,
    func_type: TypeId,
    object_module: &ObjectModule,
) -> Result<(Signature, Option<u32>), String> {
    let (param_types, result_types) = match ctx.type_kind(func_type) {
        TypeKind::Function { params, results } => (params.clone(), results.clone()),
        _ => return Err("expected function type".into()),
    };

    let mut sig = object_module.make_signature();
    let mut sret_size: Option<u32> = None;

    // Check if return type is aggregate — needs sret convention.
    for &result_ty in &result_types {
        if ctx.is_none_type(result_ty) {
            continue;
        }
        if is_aggregate_type(ctx, result_ty) {
            // Aggregate return: add implicit sret pointer as first param.
            // Don't add to sig.returns — callee writes to sret pointer instead.
            let size = type_byte_size(ctx, result_ty);
            sret_size = Some(size);
            sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
        } else {
            let cl_ty = to_cranelift_type(ctx, result_ty)
                .ok_or_else(|| format!("unsupported result type: {:?}", ctx.type_kind(result_ty)))?;
            sig.returns.push(AbiParam::new(cl_ty));
        }
    }

    // Regular parameters (after sret if present).
    for &param_ty in &param_types {
        let cl_ty = to_cranelift_type(ctx, param_ty)
            .ok_or_else(|| format!("unsupported parameter type: {:?}", ctx.type_kind(param_ty)))?;
        sig.params.push(AbiParam::new(cl_ty));
    }

    Ok((sig, sret_size))
}

/// Lower a single func.func operation into the Cranelift FunctionBuilder.
fn lower_function(
    ctx: &Context,
    func_op: OpId,
    builder: &mut FunctionBuilder,
    object_module: &mut ObjectModule,
    func_declarations: &HashMap<String, (cranelift_module::FuncId, Signature, OpId, Option<u32>)>,
    registry: Option<&LoweringRegistry>,
    sret_size: Option<u32>,
) -> Result<(), String> {
    // CIR ValueId -> Cranelift Value mapping.
    let mut value_map: HashMap<ValueId, cranelift_codegen::ir::Value> = HashMap::new();

    // CIR BlockId -> Cranelift Block mapping.
    let mut block_map: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();

    let body_region = ctx[func_op].region(0);
    let cir_blocks = ctx[body_region].blocks();

    // Create all Cranelift blocks first (needed for forward branches).
    for &cir_block in cir_blocks {
        let cl_block = builder.create_block();
        block_map.insert(cir_block, cl_block);
    }

    // Process the entry block: wire up function parameters.
    let entry_cir_block = cir_blocks[0];
    let entry_cl_block = block_map[&entry_cir_block];
    builder.append_block_params_for_function_params(entry_cl_block);
    builder.switch_to_block(entry_cl_block);

    // Map CIR block arguments to Cranelift block params for the entry block.
    // When sret is active, the first Cranelift param is the sret pointer (not a CIR arg).
    let cir_args = ctx[entry_cir_block].arguments().to_vec();
    let cl_params = builder.block_params(entry_cl_block).to_vec();

    let sret_ptr = if sret_size.is_some() {
        // First param is the sret pointer — skip it when mapping CIR args.
        let sret = cl_params[0];
        for (cir_arg, cl_param) in cir_args.iter().zip(cl_params[1..].iter()) {
            value_map.insert(*cir_arg, *cl_param);
        }
        Some(sret)
    } else {
        for (cir_arg, cl_param) in cir_args.iter().zip(cl_params.iter()) {
            value_map.insert(*cir_arg, *cl_param);
        }
        None
    };

    // Seal the entry block if it's the only block (no predecessors).
    // For multi-block functions, we seal after processing all blocks.
    if cir_blocks.len() == 1 {
        builder.seal_block(entry_cl_block);
    }

    // Lower operations in each block.
    for (block_idx, &cir_block) in cir_blocks.iter().enumerate() {
        let cl_block = block_map[&cir_block];

        if block_idx > 0 {
            builder.switch_to_block(cl_block);

            // Map block arguments for non-entry blocks.
            let block_args = ctx[cir_block].arguments().to_vec();
            for (i, &cir_arg) in block_args.iter().enumerate() {
                let cl_ty = to_cranelift_type(ctx, ctx.value_type(cir_arg))
                    .ok_or("unsupported block argument type")?;
                builder.append_block_param(cl_block, cl_ty);
                let cl_param = builder.block_params(cl_block)[i];
                value_map.insert(cir_arg, cl_param);
            }
        }

        // Lower each operation in this block.
        for op in ctx.block_ops(cir_block) {
            lower_op(
                ctx,
                op,
                builder,
                object_module,
                &mut value_map,
                &block_map,
                func_declarations,
                registry,
                sret_ptr,
                sret_size,
            )?;
        }
    }

    // Seal all blocks.
    builder.seal_all_blocks();

    Ok(())
}

/// Lower a single CIR operation to Cranelift instructions.
///
/// Dispatch order:
/// 1. Framework ops (func.call, func.return) — always handled here
/// 2. Registered construct lowerings — each gets a chance to handle the op
/// 3. Built-in fallback (cir.constant, cir.add, etc.) — for backwards compat
///    until all ops move to construct crates
fn lower_op(
    ctx: &Context,
    op: OpId,
    builder: &mut FunctionBuilder,
    object_module: &mut ObjectModule,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    block_map: &HashMap<BlockId, cranelift_codegen::ir::Block>,
    func_declarations: &HashMap<String, (cranelift_module::FuncId, Signature, OpId, Option<u32>)>,
    registry: Option<&LoweringRegistry>,
    sret_ptr: Option<cranelift_codegen::ir::Value>,
    sret_size: Option<u32>,
) -> Result<(), String> {
    let op_name = ctx[op].name();

    // Create unified LoweringCtx for all dispatch paths.
    let mut lctx = LoweringCtx::new(ctx, builder, value_map, block_map, object_module, func_declarations, sret_ptr, sret_size);

    // 1. Framework-level ops (func dialect) — always handled here
    match op_name {
        "func.return" => return lctx.lower_return(op),
        "func.call" => return lctx.lower_call(op),
        _ => {}
    }

    // 2. Dispatch to registered construct lowerings
    if let Some(reg) = registry {
        for construct in reg.constructs() {
            if construct.lower_op(op, &mut lctx)? {
                return Ok(());
            }
        }
    }

    // 3. Built-in fallback — only used when registry is None (e.g., mlif unit tests).
    // When ArithLowering is registered, these ops are handled by the construct crate
    // and this fallback is never reached. Remove once mlif tests use a registry.
    lower_op_fallback(ctx, op, &mut lctx)
}

/// Built-in fallback lowering for basic ops (used by mlif unit tests
/// that don't register construct lowerings).
fn lower_op_fallback(
    ctx: &Context,
    op: OpId,
    lctx: &mut LoweringCtx,
) -> Result<(), String> {
    match ctx[op].name() {
        "cir.constant" => lower_constant(ctx, op, lctx),
        "cir.add" => lower_binary_int(ctx, op, lctx, BinaryIntOp::Add),
        "cir.sub" => lower_binary_int(ctx, op, lctx, BinaryIntOp::Sub),
        "cir.mul" => lower_binary_int(ctx, op, lctx, BinaryIntOp::Mul),
        _ => Err(format!("unsupported operation: {}", ctx[op].name())),
    }
}

/// Lower `cir.constant` to `iconst` or `f32const`/`f64const`.
fn lower_constant(
    ctx: &Context,
    op: OpId,
    lctx: &mut LoweringCtx,
) -> Result<(), String> {
    match ctx[op].get_attribute("value") {
        Some(Attribute::Integer { value, .. }) => {
            let cl_type = lctx.result_cranelift_type(op)?;
            let r = lctx.ins().iconst(cl_type, *value);
            lctx.set_result(op, r);
            Ok(())
        }
        Some(Attribute::Float { value, .. }) => {
            let rt = lctx.result_type(op);
            match ctx.type_kind(rt) {
                TypeKind::Float { width: 32 } => {
                    let r = lctx.ins().f32const(*value as f32);
                    lctx.set_result(op, r);
                }
                TypeKind::Float { width: 64 } => {
                    let r = lctx.ins().f64const(*value);
                    lctx.set_result(op, r);
                }
                _ => return Err("cir.constant: unsupported float width".into()),
            }
            Ok(())
        }
        _ => Err("cir.constant: missing or unsupported 'value' attribute".into()),
    }
}

/// Binary integer operations supported by the fallback lowering.
enum BinaryIntOp {
    Add,
    Sub,
    Mul,
}

/// Lower a binary integer op (cir.add, cir.sub, cir.mul) to iadd/isub/imul.
fn lower_binary_int(
    _ctx: &Context,
    op: OpId,
    lctx: &mut LoweringCtx,
    bin_op: BinaryIntOp,
) -> Result<(), String> {
    let (lhs, rhs) = lctx.binary_operands(op)?;

    let r = match bin_op {
        BinaryIntOp::Add => lctx.ins().iadd(lhs, rhs),
        BinaryIntOp::Sub => lctx.ins().isub(lhs, rhs),
        BinaryIntOp::Mul => lctx.ins().imul(lhs, rhs),
    };

    lctx.set_result(op, r);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::attributes::{Attribute, NamedAttribute};
    use crate::ir::builder::Builder;
    use crate::ir::location::Location;

    /// Helper: build a CIR module containing a single `main` function
    /// that returns integer constant 42.
    fn build_return_42_module() -> (Context, OpId) {
        let mut ctx = Context::new();
        let i32_ty = ctx.integer_type(32);
        let fn_ty = ctx.function_type(&[], &[i32_ty]);

        // builtin.module { func.func @main() -> i32 { return 42 } }
        let module_block = ctx.create_block();
        let module_region = ctx.create_region();
        ctx.region_push_block(module_region, module_block);

        let module_op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![module_region],
            Location::unknown(),
        );

        // Build func.func @main
        let mut b = Builder::at_end(&mut ctx, module_block);
        let func_op = b.build_func("main", fn_ty, Location::unknown());

        // Build the body: cir.constant 42, func.return
        let entry = b.func_entry_block(func_op);
        b.set_insertion_point_to_end(entry);

        let const_op = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 42,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let const_val = b.op_result(const_op, 0);
        b.build_return(&[const_val], Location::unknown());

        (ctx, module_op)
    }

    /// Helper: build a CIR module with `add(i32, i32) -> i32` and
    /// `main() -> i32` that calls `add(20, 22)`.
    fn build_add_call_module() -> (Context, OpId) {
        let mut ctx = Context::new();
        let i32_ty = ctx.integer_type(32);
        let add_fn_ty = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);
        let main_fn_ty = ctx.function_type(&[], &[i32_ty]);

        // Module shell.
        let module_block = ctx.create_block();
        let module_region = ctx.create_region();
        ctx.region_push_block(module_region, module_block);

        let module_op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![module_region],
            Location::unknown(),
        );

        // --- func @add(i32, i32) -> i32 ---
        let mut b = Builder::at_end(&mut ctx, module_block);
        let add_func = b.build_func("add", add_fn_ty, Location::unknown());
        let add_entry = b.func_entry_block(add_func);
        b.set_insertion_point_to_end(add_entry);

        let arg0 = b.block_argument(add_entry, 0);
        let arg1 = b.block_argument(add_entry, 1);

        let add_op = b.create_op("cir.add", &[arg0, arg1], &[i32_ty], Location::unknown());
        let add_result = b.op_result(add_op, 0);
        b.build_return(&[add_result], Location::unknown());

        // --- func @main() -> i32 ---
        b.set_insertion_point_to_end(module_block);
        let main_func = b.build_func("main", main_fn_ty, Location::unknown());
        let main_entry = b.func_entry_block(main_func);
        b.set_insertion_point_to_end(main_entry);

        let c20 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 20,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let c22 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 22,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let v20 = b.op_result(c20, 0);
        let v22 = b.op_result(c22, 0);

        let call_op = b.build_call("add", &[v20, v22], &[i32_ty], Location::unknown());
        let call_result = b.op_result(call_op, 0);
        b.build_return(&[call_result], Location::unknown());

        (ctx, module_op)
    }

    /// Helper: build a CIR module with `main() -> i32` that computes
    /// 10 - 3 = 7 using cir.sub.
    fn build_sub_module() -> (Context, OpId) {
        let mut ctx = Context::new();
        let i32_ty = ctx.integer_type(32);
        let main_fn_ty = ctx.function_type(&[], &[i32_ty]);

        let module_block = ctx.create_block();
        let module_region = ctx.create_region();
        ctx.region_push_block(module_region, module_block);

        let module_op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![module_region],
            Location::unknown(),
        );

        let mut b = Builder::at_end(&mut ctx, module_block);
        let main_func = b.build_func("main", main_fn_ty, Location::unknown());
        let entry = b.func_entry_block(main_func);
        b.set_insertion_point_to_end(entry);

        let c10 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 10,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let c3 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 3,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let v10 = b.op_result(c10, 0);
        let v3 = b.op_result(c3, 0);

        let sub_op = b.create_op("cir.sub", &[v10, v3], &[i32_ty], Location::unknown());
        let sub_result = b.op_result(sub_op, 0);
        b.build_return(&[sub_result], Location::unknown());

        (ctx, module_op)
    }

    /// Helper: build a CIR module with `main() -> i32` that computes
    /// 6 * 7 = 42 using cir.mul.
    fn build_mul_module() -> (Context, OpId) {
        let mut ctx = Context::new();
        let i32_ty = ctx.integer_type(32);
        let main_fn_ty = ctx.function_type(&[], &[i32_ty]);

        let module_block = ctx.create_block();
        let module_region = ctx.create_region();
        ctx.region_push_block(module_region, module_block);

        let module_op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![module_region],
            Location::unknown(),
        );

        let mut b = Builder::at_end(&mut ctx, module_block);
        let main_func = b.build_func("main", main_fn_ty, Location::unknown());
        let entry = b.func_entry_block(main_func);
        b.set_insertion_point_to_end(entry);

        let c6 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 6,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let c7 = b.create_op_full(
            "cir.constant",
            &[],
            &[i32_ty],
            vec![NamedAttribute::new(
                "value",
                Attribute::Integer {
                    value: 7,
                    ty: i32_ty,
                },
            )],
            vec![],
            Location::unknown(),
        );
        let v6 = b.op_result(c6, 0);
        let v7 = b.op_result(c7, 0);

        let mul_op = b.create_op("cir.mul", &[v6, v7], &[i32_ty], Location::unknown());
        let mul_result = b.op_result(mul_op, 0);
        b.build_return(&[mul_result], Location::unknown());

        (ctx, module_op)
    }

    #[test]
    fn test_lower_return_42() {
        let (ctx, module_op) = build_return_42_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn test_lower_add_call() {
        let (ctx, module_op) = build_add_call_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn test_lower_sub() {
        let (ctx, module_op) = build_sub_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn test_lower_mul() {
        let (ctx, module_op) = build_mul_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn test_lower_empty_module_errors() {
        let mut ctx = Context::new();
        // Module with empty region (no blocks).
        let module_region = ctx.create_region();
        let module_op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![module_region],
            Location::unknown(),
        );
        let result = lower_module(&ctx, module_op, None);
        assert!(result.is_err());
    }

    /// End-to-end test: lower, write, link, execute, check exit code.
    #[test]
    fn test_end_to_end_return_42() {
        let (ctx, module_op) = build_return_42_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering failed");

        let tmp_dir = std::env::temp_dir();
        let obj_path = tmp_dir.join("mlif_test_return42.o");
        let exe_path = tmp_dir.join("mlif_test_return42");

        super::super::emit::write_object_file(
            &bytes,
            obj_path.to_str().unwrap(),
        )
        .expect("write_object_file failed");

        super::super::link::link_executable(
            obj_path.to_str().unwrap(),
            exe_path.to_str().unwrap(),
        )
        .expect("link_executable failed");

        let status = std::process::Command::new(exe_path.to_str().unwrap())
            .status()
            .expect("failed to run executable");

        assert_eq!(status.code(), Some(42));

        // Clean up.
        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&exe_path);
    }

    /// End-to-end test: add(20, 22) = 42.
    #[test]
    fn test_end_to_end_add_call() {
        let (ctx, module_op) = build_add_call_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering failed");

        let tmp_dir = std::env::temp_dir();
        let obj_path = tmp_dir.join("mlif_test_add_call.o");
        let exe_path = tmp_dir.join("mlif_test_add_call");

        super::super::emit::write_object_file(
            &bytes,
            obj_path.to_str().unwrap(),
        )
        .expect("write_object_file failed");

        super::super::link::link_executable(
            obj_path.to_str().unwrap(),
            exe_path.to_str().unwrap(),
        )
        .expect("link_executable failed");

        let status = std::process::Command::new(exe_path.to_str().unwrap())
            .status()
            .expect("failed to run executable");

        assert_eq!(status.code(), Some(42));

        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&exe_path);
    }

    /// End-to-end test: 10 - 3 = 7.
    #[test]
    fn test_end_to_end_sub() {
        let (ctx, module_op) = build_sub_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering failed");

        let tmp_dir = std::env::temp_dir();
        let obj_path = tmp_dir.join("mlif_test_sub.o");
        let exe_path = tmp_dir.join("mlif_test_sub");

        super::super::emit::write_object_file(
            &bytes,
            obj_path.to_str().unwrap(),
        )
        .expect("write_object_file failed");

        super::super::link::link_executable(
            obj_path.to_str().unwrap(),
            exe_path.to_str().unwrap(),
        )
        .expect("link_executable failed");

        let status = std::process::Command::new(exe_path.to_str().unwrap())
            .status()
            .expect("failed to run executable");

        assert_eq!(status.code(), Some(7));

        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&exe_path);
    }

    /// End-to-end test: 6 * 7 = 42.
    #[test]
    fn test_end_to_end_mul() {
        let (ctx, module_op) = build_mul_module();
        let bytes = lower_module(&ctx, module_op, None).expect("lowering failed");

        let tmp_dir = std::env::temp_dir();
        let obj_path = tmp_dir.join("mlif_test_mul.o");
        let exe_path = tmp_dir.join("mlif_test_mul");

        super::super::emit::write_object_file(
            &bytes,
            obj_path.to_str().unwrap(),
        )
        .expect("write_object_file failed");

        super::super::link::link_executable(
            obj_path.to_str().unwrap(),
            exe_path.to_str().unwrap(),
        )
        .expect("link_executable failed");

        let status = std::process::Command::new(exe_path.to_str().unwrap())
            .status()
            .expect("failed to run executable");

        assert_eq!(status.code(), Some(42));

        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&exe_path);
    }
}
