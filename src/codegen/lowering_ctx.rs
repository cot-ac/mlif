//! LoweringCtx — MLIR-like helpers for Cranelift construct lowering.
//!
//! Wraps the CIR context, Cranelift FunctionBuilder, value/block maps,
//! and ObjectModule into a single struct with convenience methods.
//! Construct lowerings become 2-5 line wrappers.
//!
//! Mirrors the C++ MLIR pattern: `OpAdaptor` (operand access) +
//! `ConversionPatternRewriter` (instruction emission) + `TypeConverter`
//! (type mapping) in a single unified context.

#![cfg(feature = "codegen")]

use std::collections::HashMap;

use cranelift_codegen::ir::{self as clir, types, AbiParam, InstBuilder, MemFlags, Signature,
                            StackSlotData, StackSlotKind};
use cranelift_frontend::{FuncInstBuilder, FunctionBuilder};
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;

use crate::entity::{BlockId, EntityRef, OpId, TypeId, ValueId};
use crate::ir::attributes::Attribute;
use crate::ir::context::Context;
use crate::ir::types::TypeKind;

use super::types::{to_cranelift_type, type_byte_size};

/// Context passed to construct lowerings during Cranelift code generation.
///
/// Provides MLIR-style helpers so that construct lowerings can be concise
/// wrappers rather than manual Cranelift instruction sequences.
pub struct LoweringCtx<'a, 'f> {
    /// Read-only access to the CIR IR.
    pub ir: &'a Context,
    builder: &'a mut FunctionBuilder<'f>,
    value_map: &'a mut HashMap<ValueId, clir::Value>,
    block_map: &'a HashMap<BlockId, clir::Block>,
    module: &'a mut ObjectModule,
    func_declarations: &'a HashMap<String, (cranelift_module::FuncId, Signature, OpId, Option<u32>)>,
    /// If this function uses sret convention, the pointer to caller-allocated return buffer.
    sret_ptr: Option<clir::Value>,
    /// Size of the sret aggregate in bytes (if sret is active).
    sret_size: Option<u32>,
}

impl<'a, 'f> LoweringCtx<'a, 'f> {
    /// Create a new LoweringCtx.
    pub fn new(
        ir: &'a Context,
        builder: &'a mut FunctionBuilder<'f>,
        value_map: &'a mut HashMap<ValueId, clir::Value>,
        block_map: &'a HashMap<BlockId, clir::Block>,
        module: &'a mut ObjectModule,
        func_declarations: &'a HashMap<String, (cranelift_module::FuncId, Signature, OpId, Option<u32>)>,
        sret_ptr: Option<clir::Value>,
        sret_size: Option<u32>,
    ) -> Self {
        Self { ir, builder, value_map, block_map, module, func_declarations, sret_ptr, sret_size }
    }

    // =====================================================================
    // Operand access (mirrors MLIR OpAdaptor)
    // =====================================================================

    /// Look up a single operand by index.
    pub fn operand(&self, op: OpId, idx: usize) -> Result<clir::Value, String> {
        let operands = self.ir[op].operands();
        let cir_val = operands.get(idx).ok_or_else(|| {
            format!("{}: operand {} out of range (has {})",
                    self.ir[op].name(), idx, operands.len())
        })?;
        self.value_map
            .get(cir_val)
            .copied()
            .ok_or_else(|| format!("{}: operand {} not in value map", self.ir[op].name(), idx))
    }

    /// Look up all operands as a Vec.
    pub fn all_operands(&self, op: OpId) -> Result<Vec<clir::Value>, String> {
        self.ir[op].operands().iter().enumerate().map(|(i, cir_val)| {
            self.value_map.get(cir_val).copied()
                .ok_or_else(|| format!("{}: operand {} not in value map", self.ir[op].name(), i))
        }).collect()
    }

    /// Number of operands on an op.
    pub fn num_operands(&self, op: OpId) -> usize {
        self.ir[op].operands().len()
    }

    /// Look up both operands of a binary op.
    pub fn binary_operands(&self, op: OpId) -> Result<(clir::Value, clir::Value), String> {
        let operands = self.ir[op].operands();
        if operands.len() != 2 {
            return Err(format!(
                "{}: expected 2 operands, got {}", self.ir[op].name(), operands.len()
            ));
        }
        let lhs = self.value_map.get(&operands[0]).copied()
            .ok_or_else(|| format!("{}: lhs not in value map", self.ir[op].name()))?;
        let rhs = self.value_map.get(&operands[1]).copied()
            .ok_or_else(|| format!("{}: rhs not in value map", self.ir[op].name()))?;
        Ok((lhs, rhs))
    }

    /// Look up the single operand of a unary op.
    pub fn unary_operand(&self, op: OpId) -> Result<clir::Value, String> {
        let operands = self.ir[op].operands();
        if operands.len() != 1 {
            return Err(format!(
                "{}: expected 1 operand, got {}", self.ir[op].name(), operands.len()
            ));
        }
        self.value_map.get(&operands[0]).copied()
            .ok_or_else(|| format!("{}: operand not in value map", self.ir[op].name()))
    }

    // =====================================================================
    // Result mapping (mirrors rewriter.replaceOp)
    // =====================================================================

    /// Map a CIR op's result(0) to a Cranelift value.
    pub fn set_result(&mut self, op: OpId, val: clir::Value) {
        self.value_map.insert(self.ir[op].result(0), val);
    }

    /// Map multiple CIR op results.
    pub fn set_results(&mut self, op: OpId, vals: &[clir::Value]) {
        let results = self.ir[op].results();
        for (cir_result, cl_val) in results.iter().zip(vals.iter()) {
            self.value_map.insert(*cir_result, *cl_val);
        }
    }

    // =====================================================================
    // Type helpers (mirrors TypeConverter)
    // =====================================================================

    /// Get the CIR TypeId of an op's result(0).
    pub fn result_type(&self, op: OpId) -> TypeId {
        self.ir.value_type(self.ir[op].result(0))
    }

    /// Get the Cranelift type for an op's result(0).
    pub fn result_cranelift_type(&self, op: OpId) -> Result<clir::Type, String> {
        to_cranelift_type(self.ir, self.result_type(op))
            .ok_or_else(|| format!("{}: unsupported result type", self.ir[op].name()))
    }

    /// Check if an op's result type is floating point.
    pub fn result_is_float(&self, op: OpId) -> bool {
        matches!(self.ir.type_kind(self.result_type(op)), TypeKind::Float { .. })
    }

    /// Compute the byte size of a CIR type.
    pub fn type_size(&self, ty: TypeId) -> u32 {
        type_byte_size(self.ir, ty)
    }

    /// Get a type parameter from an extension type.
    pub fn ext_type_param(&self, ty: TypeId, idx: usize) -> Option<TypeId> {
        match self.ir.type_kind(ty) {
            TypeKind::Extension(ext) => ext.type_params.get(idx).copied(),
            _ => None,
        }
    }

    /// Get an integer parameter from an extension type.
    pub fn ext_int_param(&self, ty: TypeId, idx: usize) -> Option<i64> {
        match self.ir.type_kind(ty) {
            TypeKind::Extension(ext) => ext.int_params.get(idx).copied(),
            _ => None,
        }
    }

    /// Get a string parameter from an extension type.
    pub fn ext_string_param(&self, ty: TypeId, idx: usize) -> Option<String> {
        match self.ir.type_kind(ty) {
            TypeKind::Extension(ext) => ext.string_params.get(idx).cloned(),
            _ => None,
        }
    }

    /// Number of type parameters on an extension type.
    pub fn ext_type_param_count(&self, ty: TypeId) -> usize {
        match self.ir.type_kind(ty) {
            TypeKind::Extension(ext) => ext.type_params.len(),
            _ => 0,
        }
    }

    /// Number of string parameters on an extension type.
    pub fn ext_string_param_count(&self, ty: TypeId) -> usize {
        match self.ir.type_kind(ty) {
            TypeKind::Extension(ext) => ext.string_params.len(),
            _ => 0,
        }
    }

    /// Map a CIR type to a Cranelift type.
    pub fn cranelift_type(&self, ty: TypeId) -> Result<clir::Type, String> {
        to_cranelift_type(self.ir, ty)
            .ok_or_else(|| format!("unsupported type: {:?}", self.ir.type_kind(ty)))
    }

    /// Get the CIR type of a CIR value.
    pub fn value_type(&self, val: ValueId) -> TypeId {
        self.ir.value_type(val)
    }

    // =====================================================================
    // Aggregate layout helpers (mirrors LLVM::InsertValueOp / ExtractValueOp)
    // =====================================================================

    /// Compute the byte offset of field `idx` in a struct type.
    /// Fields are packed sequentially (no alignment padding).
    pub fn field_offset(&self, struct_ty: TypeId, idx: usize) -> u32 {
        let mut offset = 0u32;
        for i in 0..idx {
            if let Some(field_ty) = self.ext_type_param(struct_ty, i) {
                offset += type_byte_size(self.ir, field_ty);
            }
        }
        offset
    }

    /// Number of fields in a struct type (= number of type params).
    pub fn field_count(&self, struct_ty: TypeId) -> usize {
        self.ext_type_param_count(struct_ty)
    }

    /// Compute the byte offset of element `idx_val` in an array type.
    /// Returns a dynamic Cranelift value: idx_val * element_size.
    pub fn elem_byte_offset(&mut self, array_ty: TypeId, idx_val: clir::Value) -> clir::Value {
        let elem_ty = self.ext_type_param(array_ty, 0).unwrap();
        let elem_size = type_byte_size(self.ir, elem_ty);
        let size_val = self.builder.ins().iconst(types::I64, elem_size as i64);
        self.builder.ins().imul(idx_val, size_val)
    }

    /// Compute a pointer to ptr + dynamic byte offset.
    pub fn ptr_add(&mut self, ptr: clir::Value, offset: clir::Value) -> clir::Value {
        self.builder.ins().iadd(ptr, offset)
    }

    /// Compute a pointer to ptr + static byte offset.
    pub fn addr_at_offset(&mut self, ptr: clir::Value, offset: u32) -> clir::Value {
        if offset == 0 {
            ptr
        } else {
            // Use iadd for address computation — this produces a proper pointer value
            // that can be stored/loaded through, unlike a Cranelift Offset32 which
            // is only usable as a store/load operand.
            let off = self.builder.ins().iconst(types::I64, offset as i64);
            self.builder.ins().iadd(ptr, off)
        }
    }

    // =====================================================================
    // Instruction emission (mirrors ConversionPatternRewriter)
    // =====================================================================

    /// Access the Cranelift instruction builder.
    pub fn ins(&mut self) -> FuncInstBuilder<'_, 'f> {
        self.builder.ins()
    }

    // =====================================================================
    // Memory helpers (stack allocation + load/store)
    // =====================================================================

    /// Allocate a stack slot and return its address as I64.
    pub fn stack_alloc(&mut self, size: u32) -> clir::Value {
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot, size, 0,
        ));
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    /// Store a value at ptr + offset.
    pub fn store_at_offset(&mut self, val: clir::Value, ptr: clir::Value, offset: u32) {
        if offset == 0 {
            self.builder.ins().store(MemFlags::new(), val, ptr, 0);
        } else {
            let off = self.builder.ins().iconst(types::I64, offset as i64);
            let addr = self.builder.ins().iadd(ptr, off);
            self.builder.ins().store(MemFlags::new(), val, addr, 0);
        }
    }

    /// Load a value of the given type from ptr + offset.
    pub fn load_at_offset(&mut self, ty: clir::Type, ptr: clir::Value, offset: u32) -> clir::Value {
        if offset == 0 {
            self.builder.ins().load(ty, MemFlags::new(), ptr, 0)
        } else {
            let off = self.builder.ins().iconst(types::I64, offset as i64);
            let addr = self.builder.ins().iadd(ptr, off);
            self.builder.ins().load(ty, MemFlags::new(), addr, 0)
        }
    }

    /// Store a value at ptr + dynamic offset.
    pub fn store_dynamic(&mut self, val: clir::Value, ptr: clir::Value, offset: clir::Value) {
        let addr = self.builder.ins().iadd(ptr, offset);
        self.builder.ins().store(MemFlags::new(), val, addr, 0);
    }

    /// Load a value at ptr + dynamic offset.
    pub fn load_dynamic(&mut self, ty: clir::Type, ptr: clir::Value, offset: clir::Value) -> clir::Value {
        let addr = self.builder.ins().iadd(ptr, offset);
        self.builder.ins().load(ty, MemFlags::new(), addr, 0)
    }

    // =====================================================================
    // Module-level operations (mirrors LLVM::GlobalOp, LLVM::AddressOfOp)
    // =====================================================================

    /// Declare and define a read-only data section (e.g., string constant).
    /// Returns the address of the data as a Cranelift Value (I64).
    pub fn define_data(&mut self, name: &str, bytes: &[u8]) -> Result<clir::Value, String> {
        let data_id = self.module
            .declare_data(name, Linkage::Local, false, false)
            .map_err(|e| format!("declare_data '{}': {}", name, e))?;

        let mut data_desc = cranelift_module::DataDescription::new();
        data_desc.define(bytes.to_vec().into_boxed_slice());
        self.module
            .define_data(data_id, &data_desc)
            .map_err(|e| format!("define_data '{}': {}", name, e))?;

        let global = self.module.declare_data_in_func(data_id, self.builder.func);
        let addr = self.builder.ins().global_value(types::I64, global);
        Ok(addr)
    }

    /// Declare an imported (external) function and return a callable FuncRef.
    pub fn declare_import_func(
        &mut self,
        name: &str,
        params: &[clir::Type],
        returns: &[clir::Type],
    ) -> Result<clir::FuncRef, String> {
        let mut sig = self.module.make_signature();
        for &p in params {
            sig.params.push(AbiParam::new(p));
        }
        for &r in returns {
            sig.returns.push(AbiParam::new(r));
        }
        let func_id = self.module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| format!("declare_function '{}': {}", name, e))?;
        Ok(self.module.declare_func_in_func(func_id, self.builder.func))
    }

    /// Call a function declared in the CIR module (from func_declarations).
    pub fn call_func(&mut self, name: &str, args: &[clir::Value]) -> Result<clir::Inst, String> {
        let (func_id, _, _, _) = self.func_declarations.get(name)
            .ok_or_else(|| format!("unknown function '{}'", name))?;
        let func_ref = self.module.declare_func_in_func(*func_id, self.builder.func);
        Ok(self.builder.ins().call(func_ref, args))
    }

    /// Get the result values from a call instruction.
    pub fn inst_results(&self, inst: clir::Inst) -> Vec<clir::Value> {
        self.builder.inst_results(inst).to_vec()
    }

    // =====================================================================
    // Attribute access
    // =====================================================================

    /// Read an integer attribute value.
    pub fn int_attr(&self, op: OpId, name: &str) -> Result<i64, String> {
        match self.ir[op].get_attribute(name) {
            Some(Attribute::Integer { value, .. }) => Ok(*value),
            _ => Err(format!("{}: missing integer attribute '{}'",
                             self.ir[op].name(), name)),
        }
    }

    /// Read a string attribute value.
    pub fn string_attr(&self, op: OpId, name: &str) -> Result<String, String> {
        match self.ir[op].get_attribute(name) {
            Some(Attribute::String(s)) => Ok(s.clone()),
            _ => Err(format!("{}: missing string attribute '{}'",
                             self.ir[op].name(), name)),
        }
    }

    /// Read a type attribute value.
    pub fn type_attr(&self, op: OpId, name: &str) -> Result<TypeId, String> {
        match self.ir[op].get_attribute(name) {
            Some(Attribute::Type(ty)) => Ok(*ty),
            _ => Err(format!("{}: missing type attribute '{}'",
                             self.ir[op].name(), name)),
        }
    }

    /// Read a block destination from an integer attribute and resolve via block_map.
    pub fn block_dest(&self, op: OpId, name: &str) -> Result<clir::Block, String> {
        let block_idx = self.int_attr(op, name)?;
        let cir_block = BlockId::new(block_idx as usize);
        self.block_map.get(&cir_block).copied()
            .ok_or_else(|| format!("{}: block {} not in block_map",
                                   self.ir[op].name(), block_idx))
    }

    /// Resolve a CIR BlockId to a Cranelift Block directly.
    pub fn block_dest_raw(&self, cir_block: BlockId) -> Result<clir::Block, String> {
        self.block_map.get(&cir_block).copied()
            .ok_or_else(|| format!("block {:?} not in block_map", cir_block))
    }

    /// Read an array attribute.
    pub fn array_attr(&self, op: OpId, name: &str) -> Result<Vec<Attribute>, String> {
        match self.ir[op].get_attribute(name) {
            Some(Attribute::Array(vals)) => Ok(vals.clone()),
            _ => Err(format!("{}: missing array attribute '{}'",
                             self.ir[op].name(), name)),
        }
    }

    // =====================================================================
    // Block helpers
    // =====================================================================

    /// Create a new Cranelift block.
    pub fn create_block(&mut self) -> clir::Block {
        self.builder.create_block()
    }

    /// Switch instruction emission to a different block.
    pub fn switch_to_block(&mut self, block: clir::Block) {
        self.builder.switch_to_block(block);
    }

    /// Seal a block (declare all predecessors known).
    pub fn seal_block(&mut self, block: clir::Block) {
        self.builder.seal_block(block);
    }

    // =====================================================================
    // Framework op lowering (func.return, func.call)
    // =====================================================================

    /// Lower `func.return` to Cranelift `return_`.
    ///
    /// When sret is active, copies aggregate data from the local pointer to
    /// the caller-allocated sret buffer, then returns void.
    pub fn lower_return(&mut self, op: OpId) -> Result<(), String> {
        let operands = self.ir[op].operands();

        if let (Some(sret_ptr), Some(size)) = (self.sret_ptr, self.sret_size) {
            // sret: copy aggregate from local stack to caller's buffer.
            if !operands.is_empty() {
                let src = self.value_map.get(&operands[0]).copied()
                    .ok_or("func.return: sret operand not found")?;
                self.copy_memory(src, sret_ptr, size);
            }
            self.builder.ins().return_(&[]);
        } else {
            let cl_values: Vec<clir::Value> = operands.iter().map(|&v| {
                self.value_map.get(&v).copied()
                    .ok_or_else(|| "func.return: operand not found in value map".to_string())
            }).collect::<Result<_, _>>()?;
            self.builder.ins().return_(&cl_values);
        }
        Ok(())
    }

    /// Lower `func.call` to Cranelift `call`.
    ///
    /// When the callee uses sret, allocates a buffer in the caller's frame,
    /// passes it as the first argument, and maps the result to that buffer.
    pub fn lower_call(&mut self, op: OpId) -> Result<(), String> {
        let callee_name = match self.ir[op].get_attribute("callee") {
            Some(Attribute::SymbolRef(name)) => name.clone(),
            _ => return Err("func.call: missing callee attribute".into()),
        };

        let (callee_func_id, _, _, callee_sret_size) = self.func_declarations.get(&callee_name)
            .ok_or_else(|| format!("func.call: unknown callee '{}'", callee_name))?;

        let callee_ref = self.module.declare_func_in_func(*callee_func_id, self.builder.func);

        let operands = self.ir[op].operands();
        let user_args: Vec<clir::Value> = operands.iter().map(|&v| {
            self.value_map.get(&v).copied()
                .ok_or_else(|| "func.call: argument not found in value map".to_string())
        }).collect::<Result<_, _>>()?;

        if let Some(size) = callee_sret_size {
            // Callee uses sret: allocate return buffer in caller, pass as first arg.
            let ret_buf = self.stack_alloc(*size);
            let mut cl_args = vec![ret_buf];
            cl_args.extend_from_slice(&user_args);
            self.builder.ins().call(callee_ref, &cl_args);

            // Map CIR result to the caller's buffer.
            let cir_results = self.ir[op].results();
            if !cir_results.is_empty() {
                self.value_map.insert(cir_results[0], ret_buf);
            }
        } else {
            // Normal call — no sret.
            let call_inst = self.builder.ins().call(callee_ref, &user_args);
            let cl_results = self.builder.inst_results(call_inst).to_vec();
            let cir_results = self.ir[op].results();
            for (cir_result, cl_result) in cir_results.iter().zip(cl_results.iter()) {
                self.value_map.insert(*cir_result, *cl_result);
            }
        }
        Ok(())
    }

    /// Copy `size` bytes from src to dst using word-sized loads/stores.
    fn copy_memory(&mut self, src: clir::Value, dst: clir::Value, size: u32) {
        let mut offset = 0u32;
        while offset + 8 <= size {
            let val = self.load_at_offset(types::I64, src, offset);
            self.store_at_offset(val, dst, offset);
            offset += 8;
        }
        while offset + 4 <= size {
            let val = self.load_at_offset(types::I32, src, offset);
            self.store_at_offset(val, dst, offset);
            offset += 4;
        }
        while offset + 2 <= size {
            let val = self.load_at_offset(types::I16, src, offset);
            self.store_at_offset(val, dst, offset);
            offset += 2;
        }
        while offset < size {
            let val = self.load_at_offset(types::I8, src, offset);
            self.store_at_offset(val, dst, offset);
            offset += 1;
        }
    }
}
