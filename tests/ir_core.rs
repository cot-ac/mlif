use mlif::*;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Type interner
// ---------------------------------------------------------------------------

#[test]
fn type_interning() {
    let mut ctx = Context::new();

    let i32a = ctx.integer_type(32);
    let i32b = ctx.integer_type(32);
    assert_eq!(i32a, i32b, "same integer type should intern to same handle");

    let i64_ty = ctx.integer_type(64);
    assert_ne!(i32a, i64_ty);

    let f32_ty = ctx.float_type(32);
    let f64_ty = ctx.float_type(64);
    assert_ne!(f32_ty, f64_ty);

    assert!(ctx.is_integer_type(i32a));
    assert!(!ctx.is_float_type(i32a));
    assert!(ctx.is_float_type(f64_ty));
    assert_eq!(ctx.integer_type_width(i32a), Some(32));
    assert_eq!(ctx.float_type_width(f64_ty), Some(64));
}

#[test]
fn function_type() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let func_ty = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);

    assert!(ctx.is_function_type(func_ty));
    assert_eq!(
        ctx.function_type_params(func_ty).unwrap(),
        &[i32_ty, i32_ty]
    );
    assert_eq!(ctx.function_type_results(func_ty).unwrap(), &[i32_ty]);

    // Same signature interns to same type
    let func_ty2 = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);
    assert_eq!(func_ty, func_ty2);
}

#[test]
fn none_and_index_types() {
    let mut ctx = Context::new();
    let none = ctx.none_type();
    assert!(ctx.is_none_type(none));

    let idx = ctx.index_type();
    assert!(ctx.is_index_type(idx));
    assert_ne!(none, idx);
}

#[test]
fn extension_type() {
    let mut ctx = Context::new();
    let f32_ty = ctx.float_type(32);

    let ptr_ty = ctx.extension_type(ExtensionType::new("cir", "ptr"));
    let ptr_ty2 = ctx.extension_type(ExtensionType::new("cir", "ptr"));
    assert_eq!(ptr_ty, ptr_ty2, "extension types intern by structural equality");

    let ref_ty = ctx.extension_type(
        ExtensionType::new("cir", "ref").with_type_params(vec![f32_ty]),
    );
    assert_ne!(ptr_ty, ref_ty);

    // Struct type with parametric data
    let struct_ty = ctx.extension_type(
        ExtensionType::new("cir", "struct")
            .with_string_params(vec!["Point".into(), "x".into(), "y".into()])
            .with_type_params(vec![f32_ty, f32_ty]),
    );
    assert!(matches!(ctx.type_kind(struct_ty), TypeKind::Extension(_)));
}

#[test]
fn type_display() {
    let mut ctx = Context::new();
    let none = ctx.none_type();
    let i32_ty = ctx.integer_type(32);
    let f64_ty = ctx.float_type(64);
    let idx = ctx.index_type();
    assert_eq!(ctx.format_type(none), "none");
    assert_eq!(ctx.format_type(i32_ty), "i32");
    assert_eq!(ctx.format_type(f64_ty), "f64");
    assert_eq!(ctx.format_type(idx), "index");
}

// ---------------------------------------------------------------------------
// Values — unique IDs, use-def tracking
// ---------------------------------------------------------------------------

#[test]
fn value_uniqueness() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    // Block arguments get unique ValueIds
    let block = ctx.create_block();
    let v0 = ctx.block_add_argument(block, i32_ty);
    let v1 = ctx.block_add_argument(block, i32_ty);
    assert_ne!(v0, v1);
    assert_eq!(ctx.value_type(v0), i32_ty);
}

#[test]
fn use_def_chains() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let block = ctx.create_block();
    let arg = ctx.block_add_argument(block, i32_ty);

    // Create an op that uses arg twice
    let op = ctx.create_operation("test.op", &[arg, arg], &[i32_ty], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op);

    // arg should have 2 uses
    assert_eq!(ctx.value_uses(arg).len(), 2);
    assert_eq!(ctx.value_uses(arg)[0].user, op);
    assert_eq!(ctx.value_uses(arg)[0].operand_index, 0);
    assert_eq!(ctx.value_uses(arg)[1].operand_index, 1);

    // The result should have 0 uses initially
    let result = ctx.op_result(op, 0);
    assert!(!ctx.value_has_uses(result));

    // Create another op that uses the result
    let op2 = ctx.create_operation("test.user", &[result], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op2);
    assert_eq!(ctx.value_uses(result).len(), 1);
    assert_eq!(ctx.value_uses(result)[0].user, op2);
}

#[test]
fn replace_all_uses() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let block = ctx.create_block();
    let old_val = ctx.block_add_argument(block, i32_ty);
    let new_val = ctx.block_add_argument(block, i32_ty);

    let op1 = ctx.create_operation("test.a", &[old_val], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op1);
    let op2 = ctx.create_operation("test.b", &[old_val], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op2);

    assert_eq!(ctx.value_uses(old_val).len(), 2);
    assert_eq!(ctx.value_uses(new_val).len(), 0);

    ctx.replace_all_uses(old_val, new_val);

    // old_val has 0 uses, new_val has 2
    assert_eq!(ctx.value_uses(old_val).len(), 0);
    assert_eq!(ctx.value_uses(new_val).len(), 2);

    // Operands were updated
    assert_eq!(ctx[op1].operands()[0], new_val);
    assert_eq!(ctx[op2].operands()[0], new_val);
}

// ---------------------------------------------------------------------------
// IR construction — the spec validation test
// ---------------------------------------------------------------------------

/// Build and verify:
/// ```
/// module {
///   func @add(%a: i32, %b: i32) -> i32 {
///   ^entry(%a: i32, %b: i32):
///     %sum = cir.add %a, %b : i32
///     func.return %sum
///   }
/// }
/// ```
#[test]
fn build_add_function() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let func_ty = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);

    let module = Module::new(&mut ctx, Location::unknown());
    let module_block = module.body_block(&ctx);

    // Build function body
    let entry_block = ctx.create_block_with_args(&[i32_ty, i32_ty]);
    let body_region = ctx.create_region();
    ctx.region_push_block(body_region, entry_block);

    let func_op = ctx.create_operation(
        "func.func",
        &[],
        &[],
        vec![
            NamedAttribute::new("sym_name", Attribute::String("add".into())),
            NamedAttribute::new("function_type", Attribute::Type(func_ty)),
        ],
        vec![body_region],
        Location::unknown(),
    );
    ctx.block_push_op(module_block, func_op);

    // Get block arguments
    let arg_a = ctx.block_argument(entry_block, 0);
    let arg_b = ctx.block_argument(entry_block, 1);

    // cir.add
    let add_op = ctx.create_operation(
        "cir.add",
        &[arg_a, arg_b],
        &[i32_ty],
        vec![],
        vec![],
        Location::unknown(),
    );
    ctx.block_push_op(entry_block, add_op);
    let sum = ctx.op_result(add_op, 0);

    // func.return
    let ret_op = ctx.create_operation(
        "func.return",
        &[sum],
        &[],
        vec![],
        vec![],
        Location::unknown(),
    );
    ctx.block_push_op(entry_block, ret_op);

    // --- Verify structure ---
    assert_eq!(ctx[module.body(&ctx)].num_blocks(), 1);

    let ops: Vec<OpId> = ctx.block_ops(module_block).collect();
    assert_eq!(ops.len(), 1);
    assert!(ctx[ops[0]].is_a("func.func"));

    // Check sym_name
    match ctx[func_op].get_attribute("sym_name") {
        Some(Attribute::String(name)) => assert_eq!(name, "add"),
        _ => panic!("expected sym_name attribute"),
    }

    // Check function body
    let func_body = ctx.op_region(func_op, 0);
    assert_eq!(ctx[func_body].num_blocks(), 1);

    let entry = ctx[func_body].entry_block().unwrap();
    assert_eq!(ctx[entry].num_arguments(), 2);

    let entry_ops: Vec<OpId> = ctx.block_ops(entry).collect();
    assert_eq!(entry_ops.len(), 2);
    assert!(ctx[entry_ops[0]].is_a("cir.add"));
    assert!(ctx[entry_ops[1]].is_a("func.return"));

    // Use-def: add result is return's operand
    let add_result = ctx[entry_ops[0]].result(0);
    let ret_operand = ctx[entry_ops[1]].operands()[0];
    assert_eq!(add_result, ret_operand);

    // Use-def chain: sum has exactly one use (the return)
    assert_eq!(ctx.value_uses(sum).len(), 1);
    assert_eq!(ctx.value_uses(sum)[0].user, ret_op);

    // Parent navigation: add_op is in entry_block
    assert_eq!(ctx[add_op].parent_block(), Some(entry_block));

    // Block's parent region
    assert_eq!(ctx[entry_block].parent_region(), Some(func_body));

    // Region's parent op
    assert_eq!(ctx[func_body].parent_op(), Some(func_op));

    // Verification passes
    assert!(verify(&ctx, module.op()).is_ok());
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

#[test]
fn builder_builds_function() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let func_ty = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);

    let module = Module::new(&mut ctx, Location::unknown());
    let module_block = module.body_block(&ctx);

    let mut b = Builder::at_end(&mut ctx, module_block);

    let func_op = b.build_func("add", func_ty, Location::unknown());
    let entry = b.func_entry_block(func_op);
    let arg_a = b.block_argument(entry, 0);
    let arg_b = b.block_argument(entry, 1);

    b.set_insertion_point_to_end(entry);
    let add_op = b.create_op("cir.add", &[arg_a, arg_b], &[i32_ty], Location::unknown());
    let sum = b.op_result(add_op, 0);
    b.build_return(&[sum], Location::unknown());

    drop(b);

    // Verify
    assert!(verify(&ctx, module.op()).is_ok());

    // Check structure
    let func_body = ctx.op_region(func_op, 0);
    let entry_block = ctx.region_entry_block(func_body).unwrap();
    let ops: Vec<OpId> = ctx.block_ops(entry_block).collect();
    assert_eq!(ops.len(), 2);
    assert!(ctx[ops[0]].is_a("cir.add"));
    assert!(ctx[ops[1]].is_a("func.return"));
}

#[test]
fn builder_call() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let block = ctx.create_block();
    let arg = ctx.block_add_argument(block, i32_ty);

    let mut b = Builder::at_end(&mut ctx, block);
    let call = b.build_call("foo", &[arg], &[i32_ty], Location::unknown());

    drop(b);

    assert!(ctx[call].is_a("func.call"));
    assert_eq!(ctx[call].num_results(), 1);
    assert_eq!(ctx[call].num_operands(), 1);
    match ctx[call].get_attribute("callee") {
        Some(Attribute::SymbolRef(name)) => assert_eq!(name, "foo"),
        _ => panic!("expected callee"),
    }
}

#[test]
fn builder_insertion_point_after() {
    let mut ctx = Context::new();

    let block = ctx.create_block();

    let mut b = Builder::at_end(&mut ctx, block);
    let _op_a = b.create_op("op.a", &[], &[], Location::unknown());
    let _op_c = b.create_op("op.c", &[], &[], Location::unknown());

    // Insert between A and C
    b.set_insertion_point_after(_op_a);
    let _op_b = b.create_op("op.b", &[], &[], Location::unknown());

    drop(b);

    let ops: Vec<OpId> = ctx.block_ops(block).collect();
    assert_eq!(ops.len(), 3);
    assert!(ctx[ops[0]].is_a("op.a"));
    assert!(ctx[ops[1]].is_a("op.b"));
    assert!(ctx[ops[2]].is_a("op.c"));
}

#[test]
fn builder_insertion_point_before() {
    let mut ctx = Context::new();

    let block = ctx.create_block();
    let op_a = ctx.create_operation("op.a", &[], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op_a);
    let op_c = ctx.create_operation("op.c", &[], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op_c);

    let mut b = Builder::new(&mut ctx);
    b.set_insertion_point_before(op_c);
    let _op_b = b.create_op("op.b", &[], &[], Location::unknown());

    drop(b);

    let ops: Vec<OpId> = ctx.block_ops(block).collect();
    assert_eq!(ops.len(), 3);
    assert!(ctx[ops[0]].is_a("op.a"));
    assert!(ctx[ops[1]].is_a("op.b"));
    assert!(ctx[ops[2]].is_a("op.c"));
}

// ---------------------------------------------------------------------------
// Walk traversal
// ---------------------------------------------------------------------------

#[test]
fn walk_preorder() {
    let (ctx, module) = build_two_op_module();
    let mut names = Vec::new();

    ctx.walk(module.op(), WalkOrder::PreOrder, &mut |op_id, ctx| {
        names.push(ctx[op_id].name().to_string());
        WalkResult::Advance
    });

    assert_eq!(
        names,
        vec!["builtin.module", "func.func", "cir.add", "func.return"]
    );
}

#[test]
fn walk_postorder() {
    let (ctx, module) = build_two_op_module();
    let mut names = Vec::new();

    ctx.walk(module.op(), WalkOrder::PostOrder, &mut |op_id, ctx| {
        names.push(ctx[op_id].name().to_string());
        WalkResult::Advance
    });

    assert_eq!(
        names,
        vec!["cir.add", "func.return", "func.func", "builtin.module"]
    );
}

#[test]
fn walk_skip() {
    let (ctx, module) = build_two_op_module();
    let mut names = Vec::new();

    ctx.walk(module.op(), WalkOrder::PreOrder, &mut |op_id, ctx| {
        names.push(ctx[op_id].name().to_string());
        if ctx[op_id].is_a("func.func") {
            WalkResult::Skip
        } else {
            WalkResult::Advance
        }
    });

    assert_eq!(names, vec!["builtin.module", "func.func"]);
}

#[test]
fn walk_interrupt() {
    let (ctx, module) = build_two_op_module();
    let mut names = Vec::new();

    ctx.walk(module.op(), WalkOrder::PreOrder, &mut |op_id, ctx| {
        names.push(ctx[op_id].name().to_string());
        if ctx[op_id].is_a("cir.add") {
            WalkResult::Interrupt
        } else {
            WalkResult::Advance
        }
    });

    assert_eq!(names, vec!["builtin.module", "func.func", "cir.add"]);
}

// ---------------------------------------------------------------------------
// Linked list operations
// ---------------------------------------------------------------------------

#[test]
fn block_linked_list() {
    let mut ctx = Context::new();
    let block = ctx.create_block();

    let op_a = ctx.create_operation("op.a", &[], &[], vec![], vec![], Location::unknown());
    let op_b = ctx.create_operation("op.b", &[], &[], vec![], vec![], Location::unknown());
    let op_c = ctx.create_operation("op.c", &[], &[], vec![], vec![], Location::unknown());

    ctx.block_push_op(block, op_a);
    ctx.block_push_op(block, op_c);
    ctx.block_insert_op_before(op_c, op_b); // Insert B before C

    let fwd: Vec<OpId> = ctx.block_ops(block).collect();
    assert_eq!(fwd, vec![op_a, op_b, op_c]);

    let rev: Vec<OpId> = ctx.block_ops_rev(block).collect();
    assert_eq!(rev, vec![op_c, op_b, op_a]);

    // Detach the middle op
    ctx.detach_op(op_b);

    let after: Vec<OpId> = ctx.block_ops(block).collect();
    assert_eq!(after, vec![op_a, op_c]);

    // Parent check
    assert_eq!(ctx[op_a].parent_block(), Some(block));
    assert_eq!(ctx[op_b].parent_block(), None); // detached
    assert_eq!(ctx[op_c].parent_block(), Some(block));
}

#[test]
fn block_insert_after() {
    let mut ctx = Context::new();
    let block = ctx.create_block();

    let op_a = ctx.create_operation("op.a", &[], &[], vec![], vec![], Location::unknown());
    let op_c = ctx.create_operation("op.c", &[], &[], vec![], vec![], Location::unknown());
    let op_b = ctx.create_operation("op.b", &[], &[], vec![], vec![], Location::unknown());

    ctx.block_push_op(block, op_a);
    ctx.block_push_op(block, op_c);
    ctx.block_insert_op_after(op_a, op_b);

    let ops: Vec<OpId> = ctx.block_ops(block).collect();
    assert_eq!(ops, vec![op_a, op_b, op_c]);
}

// ---------------------------------------------------------------------------
// Pass manager
// ---------------------------------------------------------------------------

struct CountingPass {
    name: String,
}

impl Pass for CountingPass {
    fn name(&self) -> &str {
        &self.name
    }
    fn run(&mut self, _op: OpId, _ctx: &mut Context) -> Result<(), DiagnosticError> {
        Ok(())
    }
}

#[test]
fn pass_manager_runs_passes() {
    let mut ctx = Context::new();
    let module = Module::new(&mut ctx, Location::unknown());

    let mut pm = PassManager::new();
    pm.add_pass(Box::new(CountingPass {
        name: "p1".into(),
    }));
    pm.add_pass(Box::new(CountingPass {
        name: "p2".into(),
    }));
    assert_eq!(pm.num_passes(), 2);

    pm.run(module.op(), &mut ctx).unwrap();
}

struct FailingPass;

impl Pass for FailingPass {
    fn name(&self) -> &str {
        "failing"
    }
    fn run(&mut self, _op: OpId, _ctx: &mut Context) -> Result<(), DiagnosticError> {
        Err(DiagnosticError::single(
            Location::unknown(),
            "intentional failure",
        ))
    }
}

#[test]
fn pass_manager_stops_on_error() {
    let mut ctx = Context::new();
    let module = Module::new(&mut ctx, Location::unknown());

    let mut pm = PassManager::new();
    pm.add_pass(Box::new(FailingPass));
    pm.add_pass(Box::new(CountingPass {
        name: "p2".into(),
    }));

    assert!(pm.run(module.op(), &mut ctx).is_err());
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

#[test]
fn verify_empty_module() {
    let mut ctx = Context::new();
    let module = Module::new(&mut ctx, Location::unknown());
    assert!(verify(&ctx, module.op()).is_ok());
}

#[test]
fn verify_well_formed_module() {
    let (ctx, module) = build_two_op_module();
    assert!(verify(&ctx, module.op()).is_ok());
}

#[test]
fn verify_use_def_consistency() {
    let (ctx, module) = build_two_op_module();
    // The verifier checks that every operand reference has a corresponding
    // entry in the value's use list. This should pass for well-formed IR.
    assert!(verify(&ctx, module.op()).is_ok());
}

#[test]
fn verify_terminators_pass() {
    let (ctx, module) = build_two_op_module();
    let mut terminators = HashSet::new();
    terminators.insert("func.return".to_string());
    assert!(mlif::verify::verifier::verify_terminators(&ctx, module.op(), &terminators).is_ok());
}

#[test]
fn verify_missing_terminator() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let module = Module::new(&mut ctx, Location::unknown());
    let module_block = module.body_block(&ctx);

    // Function with a block that doesn't end with a terminator
    let entry = ctx.create_block_with_args(&[i32_ty]);
    let body = ctx.create_region();
    ctx.region_push_block(body, entry);

    let func = ctx.create_operation(
        "func.func",
        &[],
        &[],
        vec![NamedAttribute::new(
            "sym_name",
            Attribute::String("bad".into()),
        )],
        vec![body],
        Location::unknown(),
    );
    ctx.block_push_op(module_block, func);

    let arg = ctx.block_argument(entry, 0);
    let add = ctx.create_operation("cir.add", &[arg, arg], &[i32_ty], vec![], vec![], Location::unknown());
    ctx.block_push_op(entry, add);
    // No return!

    let mut terminators = HashSet::new();
    terminators.insert("func.return".to_string());

    let result = mlif::verify::verifier::verify_terminators(&ctx, module.op(), &terminators);
    assert!(result.is_err());
    assert!(result.unwrap_err().diagnostics[0]
        .message
        .contains("terminator"));
}

// ---------------------------------------------------------------------------
// Trait verification
// ---------------------------------------------------------------------------

#[test]
fn verify_same_operands_and_result_type_pass() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let block = ctx.create_block();
    let a = ctx.block_add_argument(block, i32_ty);
    let b = ctx.block_add_argument(block, i32_ty);

    let op = ctx.create_operation("cir.add", &[a, b], &[i32_ty], vec![], vec![], Location::unknown());

    assert!(mlif::verify::traits::verify_same_operands_and_result_type(&ctx, op).is_ok());
}

#[test]
fn verify_same_operands_and_result_type_fail() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let i64_ty = ctx.integer_type(64);

    let block = ctx.create_block();
    let a = ctx.block_add_argument(block, i32_ty);
    let b = ctx.block_add_argument(block, i64_ty); // mismatch

    let op = ctx.create_operation("cir.add", &[a, b], &[i32_ty], vec![], vec![], Location::unknown());

    assert!(mlif::verify::traits::verify_same_operands_and_result_type(&ctx, op).is_err());
}

// ---------------------------------------------------------------------------
// Symbol table
// ---------------------------------------------------------------------------

#[test]
fn symbol_table_lookup() {
    let (ctx, module) = build_two_op_module();
    let module_block = module.body_block(&ctx);

    let table = SymbolTable::build(&ctx, ctx.block_ops(module_block));
    assert!(table.contains("add"));
    assert_eq!(table.lookup("add").map(|op| ctx[op].name()), Some("func.func"));
    assert!(!table.contains("nonexistent"));
}

// ---------------------------------------------------------------------------
// Dialect registration
// ---------------------------------------------------------------------------

#[test]
fn dialect_registration() {
    let mut ctx = Context::new();

    let mut func_dialect = Dialect::new("func");
    func_dialect
        .register_op(OpDefinition::new("func.return").with_trait(OpTrait::Terminator));
    func_dialect.register_op(OpDefinition::new("func.func"));

    ctx.register_dialect(func_dialect);

    let d = ctx.get_dialect("func").unwrap();
    assert!(d.get_op("func.return").unwrap().is_terminator());
    assert!(!d.get_op("func.func").unwrap().is_terminator());
}

// ---------------------------------------------------------------------------
// Operation attributes
// ---------------------------------------------------------------------------

#[test]
fn operation_attributes() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let op = ctx.create_operation(
        "test.op",
        &[],
        &[],
        vec![
            NamedAttribute::new("name", Attribute::String("hello".into())),
            NamedAttribute::new("value", Attribute::Integer { value: 42, ty: i32_ty }),
        ],
        vec![],
        Location::unknown(),
    );

    match ctx[op].get_attribute("name") {
        Some(Attribute::String(s)) => assert_eq!(s, "hello"),
        _ => panic!("expected string attribute"),
    }

    match ctx[op].get_attribute("value") {
        Some(Attribute::Integer { value, .. }) => assert_eq!(*value, 42),
        _ => panic!("expected integer attribute"),
    }

    assert!(ctx[op].get_attribute("missing").is_none());

    // Mutate via Context method
    ctx.op_set_attribute(op, "name", Attribute::String("world".into()));
    match ctx[op].get_attribute("name") {
        Some(Attribute::String(s)) => assert_eq!(s, "world"),
        _ => panic!("expected updated string"),
    }
}

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

#[test]
fn location_display() {
    let loc = Location::file_line_col("test.ac", 10, 5);
    assert_eq!(format!("{}", loc), "test.ac:10:5");

    let unknown = Location::unknown();
    assert_eq!(format!("{}", unknown), "<unknown>");
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_display() {
    let d = Diagnostic::error(
        Location::file_line_col("test.ac", 1, 1),
        "something went wrong",
    );
    let s = format!("{}", d);
    assert!(s.contains("error"));
    assert!(s.contains("something went wrong"));
    assert!(s.contains("test.ac:1:1"));
}

#[test]
fn diagnostic_handler() {
    let mut handler = DiagnosticHandler::new();
    assert!(!handler.has_errors());

    handler.emit(Diagnostic::warning(Location::unknown(), "just a warning"));
    assert!(!handler.has_errors());

    handler.emit(Diagnostic::error(Location::unknown(), "real error"));
    assert!(handler.has_errors());
    assert_eq!(handler.diagnostics().len(), 2);

    handler.clear();
    assert!(!handler.has_errors());
}

// ---------------------------------------------------------------------------
// IR printing
// ---------------------------------------------------------------------------

#[test]
fn print_module() {
    let (ctx, module) = build_two_op_module();
    let ir = ctx.print_op(module.op());
    assert!(ir.contains("builtin.module"));
    assert!(ir.contains("func.func"));
    assert!(ir.contains("cir.add"));
    assert!(ir.contains("func.return"));
    assert!(ir.contains("\"add\""));
}

// ---------------------------------------------------------------------------
// Nested regions
// ---------------------------------------------------------------------------

#[test]
fn nested_regions() {
    let mut ctx = Context::new();

    // Op with a nested region
    let inner_block = ctx.create_block();
    let inner_op = ctx.create_operation("inner.op", &[], &[], vec![], vec![], Location::unknown());
    ctx.block_push_op(inner_block, inner_op);

    let inner_region = ctx.create_region();
    ctx.region_push_block(inner_region, inner_block);

    let outer_block = ctx.create_block();
    let outer_op = ctx.create_operation(
        "outer.op",
        &[],
        &[],
        vec![],
        vec![inner_region],
        Location::unknown(),
    );
    ctx.block_push_op(outer_block, outer_op);

    let module_region = ctx.create_region();
    ctx.region_push_block(module_region, outer_block);
    let root = ctx.create_operation(
        "builtin.module",
        &[],
        &[],
        vec![],
        vec![module_region],
        Location::unknown(),
    );

    // Walk and count
    let mut count = 0;
    ctx.walk(root, WalkOrder::PreOrder, &mut |_, _| {
        count += 1;
        WalkResult::Advance
    });
    assert_eq!(count, 3); // root, outer, inner
}

// ---------------------------------------------------------------------------
// Erase operation
// ---------------------------------------------------------------------------

#[test]
fn erase_operation() {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);

    let block = ctx.create_block();
    let arg = ctx.block_add_argument(block, i32_ty);

    let op = ctx.create_operation("test.op", &[arg], &[i32_ty], vec![], vec![], Location::unknown());
    ctx.block_push_op(block, op);

    assert_eq!(ctx.value_uses(arg).len(), 1);

    ctx.erase_op(op);

    // Uses cleaned up
    assert_eq!(ctx.value_uses(arg).len(), 0);

    // Block is now empty
    assert!(ctx[block].is_empty());
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_two_op_module() -> (Context, Module) {
    let mut ctx = Context::new();
    let i32_ty = ctx.integer_type(32);
    let func_ty = ctx.function_type(&[i32_ty, i32_ty], &[i32_ty]);

    let module = Module::new(&mut ctx, Location::unknown());
    let module_block = module.body_block(&ctx);

    let entry_block = ctx.create_block_with_args(&[i32_ty, i32_ty]);
    let body_region = ctx.create_region();
    ctx.region_push_block(body_region, entry_block);

    let func_op = ctx.create_operation(
        "func.func",
        &[],
        &[],
        vec![
            NamedAttribute::new("sym_name", Attribute::String("add".into())),
            NamedAttribute::new("function_type", Attribute::Type(func_ty)),
        ],
        vec![body_region],
        Location::unknown(),
    );
    ctx.block_push_op(module_block, func_op);

    let arg_a = ctx.block_argument(entry_block, 0);
    let arg_b = ctx.block_argument(entry_block, 1);

    let add_op = ctx.create_operation(
        "cir.add",
        &[arg_a, arg_b],
        &[i32_ty],
        vec![],
        vec![],
        Location::unknown(),
    );
    ctx.block_push_op(entry_block, add_op);
    let sum = ctx.op_result(add_op, 0);

    let ret_op = ctx.create_operation(
        "func.return",
        &[sum],
        &[],
        vec![],
        vec![],
        Location::unknown(),
    );
    ctx.block_push_op(entry_block, ret_op);

    (ctx, module)
}
