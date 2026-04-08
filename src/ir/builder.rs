use crate::entity::{BlockId, OpId, RegionId, TypeId, ValueId};

use super::attributes::{Attribute, NamedAttribute};
use super::context::Context;
use super::location::Location;
use super::types::TypeKind;

/// Where the builder inserts new operations.
#[derive(Clone, Copy, Debug)]
pub enum InsertionPoint {
    /// Operations are created but not inserted.
    Detached,
    /// Insert at the end of a block.
    BlockEnd(BlockId),
    /// Insert before this operation.
    Before(OpId),
    /// Insert after this operation (advances after each insert).
    After(OpId),
}

/// IR builder — tracks an insertion point and provides convenience methods
/// for creating and inserting operations.
///
/// Modeled after MLIR's OpBuilder: set an insertion point, then create
/// operations that are automatically placed at that point.
pub struct Builder<'a> {
    ctx: &'a mut Context,
    ip: InsertionPoint,
}

impl<'a> Builder<'a> {
    /// Create a builder with no insertion point.
    pub fn new(ctx: &'a mut Context) -> Self {
        Self {
            ctx,
            ip: InsertionPoint::Detached,
        }
    }

    /// Create a builder positioned at the end of a block.
    pub fn at_end(ctx: &'a mut Context, block: BlockId) -> Self {
        Self {
            ctx,
            ip: InsertionPoint::BlockEnd(block),
        }
    }

    /// Set insertion point to the end of a block.
    pub fn set_insertion_point_to_end(&mut self, block: BlockId) {
        self.ip = InsertionPoint::BlockEnd(block);
    }

    /// Set insertion point to before an existing operation.
    pub fn set_insertion_point_before(&mut self, op: OpId) {
        self.ip = InsertionPoint::Before(op);
    }

    /// Set insertion point to after an existing operation.
    pub fn set_insertion_point_after(&mut self, op: OpId) {
        self.ip = InsertionPoint::After(op);
    }

    /// Get the current insertion point.
    pub fn insertion_point(&self) -> InsertionPoint {
        self.ip
    }

    fn insert_op(&mut self, op: OpId) {
        match self.ip {
            InsertionPoint::Detached => {}
            InsertionPoint::BlockEnd(block) => {
                self.ctx.block_push_op(block, op);
            }
            InsertionPoint::Before(before) => {
                self.ctx.block_insert_op_before(before, op);
                // Next insert also goes before the same target, keeping
                // insertions in sequence: A B C <before>
                self.ip = InsertionPoint::After(op);
            }
            InsertionPoint::After(after) => {
                self.ctx.block_insert_op_after(after, op);
                // Advance: next insert goes after the newly inserted op.
                self.ip = InsertionPoint::After(op);
            }
        }
    }

    // ---- Query methods (delegate to Context for ergonomics) ----

    pub fn op_result(&self, op: OpId, index: usize) -> ValueId {
        self.ctx.op_result(op, index)
    }

    pub fn op_region(&self, op: OpId, index: usize) -> RegionId {
        self.ctx.op_region(op, index)
    }

    pub fn block_argument(&self, block: BlockId, index: usize) -> ValueId {
        self.ctx.block_argument(block, index)
    }

    pub fn region_entry_block(&self, region: RegionId) -> Option<BlockId> {
        self.ctx.region_entry_block(region)
    }

    pub fn value_type(&self, value: ValueId) -> TypeId {
        self.ctx.value_type(value)
    }

    pub fn format_type(&self, ty: TypeId) -> String {
        self.ctx.format_type(ty)
    }

    /// Get the entry block of a function operation.
    pub fn func_entry_block(&self, func_op: OpId) -> BlockId {
        let body = self.ctx.op_region(func_op, 0);
        self.ctx.region_entry_block(body).unwrap()
    }

    // ---- Mutation methods ----

    /// Create an operation and insert it at the current insertion point.
    pub fn create_op(
        &mut self,
        name: &str,
        operands: &[ValueId],
        result_types: &[TypeId],
        location: Location,
    ) -> OpId {
        let op =
            self.ctx
                .create_operation(name, operands, result_types, vec![], vec![], location);
        self.insert_op(op);
        op
    }

    /// Create an operation with attributes and regions, then insert it.
    pub fn create_op_full(
        &mut self,
        name: &str,
        operands: &[ValueId],
        result_types: &[TypeId],
        attributes: Vec<NamedAttribute>,
        regions: Vec<RegionId>,
        location: Location,
    ) -> OpId {
        let op = self.ctx.create_operation(
            name,
            operands,
            result_types,
            attributes,
            regions,
            location,
        );
        self.insert_op(op);
        op
    }

    /// Build a function operation. Creates the body region and entry block
    /// with arguments matching the function type's parameters.
    pub fn build_func(&mut self, name: &str, func_type: TypeId, location: Location) -> OpId {
        let param_types = match self.ctx.type_kind(func_type) {
            TypeKind::Function { params, .. } => params.clone(),
            _ => panic!("build_func requires a function type"),
        };

        let entry_block = self.ctx.create_block_with_args(&param_types);
        let body_region = self.ctx.create_region();
        self.ctx.region_push_block(body_region, entry_block);

        let op = self.ctx.create_operation(
            "func.func",
            &[],
            &[],
            vec![
                NamedAttribute::new("sym_name", Attribute::String(name.to_string())),
                NamedAttribute::new("function_type", Attribute::Type(func_type)),
            ],
            vec![body_region],
            location,
        );
        self.insert_op(op);
        op
    }

    /// Build a function return operation.
    pub fn build_return(&mut self, values: &[ValueId], location: Location) -> OpId {
        let op =
            self.ctx
                .create_operation("func.return", values, &[], vec![], vec![], location);
        self.insert_op(op);
        op
    }

    /// Build a function call operation.
    pub fn build_call(
        &mut self,
        callee: &str,
        args: &[ValueId],
        result_types: &[TypeId],
        location: Location,
    ) -> OpId {
        let op = self.ctx.create_operation(
            "func.call",
            args,
            result_types,
            vec![NamedAttribute::new(
                "callee",
                Attribute::SymbolRef(callee.to_string()),
            )],
            vec![],
            location,
        );
        self.insert_op(op);
        op
    }

    /// Create a block with the given argument types and append it to a region.
    pub fn create_block_in_region(
        &mut self,
        region: RegionId,
        arg_types: &[TypeId],
    ) -> BlockId {
        let block = self.ctx.create_block_with_args(arg_types);
        self.ctx.region_push_block(region, block);
        block
    }
}
