use std::collections::HashMap;
use std::ops::Index;

use crate::entity::{BlockId, EntityRef, OpId, PrimaryMap, RegionId, TypeId, ValueId};

use super::attributes::NamedAttribute;
use super::block::{BlockData, BlockOpIter, BlockOpIterRev};
use super::dialect::Dialect;
use super::location::Location;
use super::operation::OperationData;
use super::region::RegionData;
use super::types::{ExtensionType, TypeKind};
use super::value::{Use, ValueData, ValueKind};

use crate::transform::walk::{WalkOrder, WalkResult};

/// The central context for MLIF. Owns all IR entities in typed arenas.
///
/// Every operation, block, region, value, and type is stored here and
/// referenced by lightweight, Copy handles (OpId, BlockId, etc.).
/// This gives stable identity, O(1) lookup, and clean ownership.
pub struct Context {
    // Entity arenas
    pub(crate) ops: PrimaryMap<OpId, OperationData>,
    pub(crate) blocks: PrimaryMap<BlockId, BlockData>,
    pub(crate) regions: PrimaryMap<RegionId, RegionData>,
    pub(crate) values: PrimaryMap<ValueId, ValueData>,
    pub(crate) types: PrimaryMap<TypeId, TypeKind>,

    // Type interner — deduplicates types by structural equality
    type_intern: HashMap<TypeKind, TypeId>,

    // Registered dialects
    dialects: Vec<Dialect>,
}

// ---------------------------------------------------------------------------
// Read-only access via Index
// ---------------------------------------------------------------------------

impl Index<OpId> for Context {
    type Output = OperationData;
    fn index(&self, id: OpId) -> &OperationData {
        &self.ops[id]
    }
}

impl Index<BlockId> for Context {
    type Output = BlockData;
    fn index(&self, id: BlockId) -> &BlockData {
        &self.blocks[id]
    }
}

impl Index<RegionId> for Context {
    type Output = RegionData;
    fn index(&self, id: RegionId) -> &RegionData {
        &self.regions[id]
    }
}

impl Index<ValueId> for Context {
    type Output = ValueData;
    fn index(&self, id: ValueId) -> &ValueData {
        &self.values[id]
    }
}

impl Index<TypeId> for Context {
    type Output = TypeKind;
    fn index(&self, id: TypeId) -> &TypeKind {
        &self.types[id]
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl Context {
    pub fn new() -> Self {
        let mut ctx = Self {
            ops: PrimaryMap::new(),
            blocks: PrimaryMap::new(),
            regions: PrimaryMap::new(),
            values: PrimaryMap::new(),
            types: PrimaryMap::new(),
            type_intern: HashMap::new(),
            dialects: Vec::new(),
        };
        // Pre-intern None type as TypeId(0).
        ctx.intern_type(TypeKind::None);
        ctx
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Check if an operation ID refers to a valid (allocated) operation.
    pub fn op_exists(&self, op: OpId) -> bool {
        self.ops.get(op).is_some()
    }
}

// ---------------------------------------------------------------------------
// Types — interned, structural equality
// ---------------------------------------------------------------------------

impl Context {
    fn intern_type(&mut self, kind: TypeKind) -> TypeId {
        if let Some(&id) = self.type_intern.get(&kind) {
            return id;
        }
        let id = self.types.push(kind.clone());
        self.type_intern.insert(kind, id);
        id
    }

    pub fn none_type(&self) -> TypeId {
        TypeId::new(0)
    }

    pub fn integer_type(&mut self, width: u32) -> TypeId {
        self.intern_type(TypeKind::Integer { width })
    }

    pub fn float_type(&mut self, width: u32) -> TypeId {
        self.intern_type(TypeKind::Float { width })
    }

    pub fn index_type(&mut self) -> TypeId {
        self.intern_type(TypeKind::Index)
    }

    pub fn function_type(&mut self, params: &[TypeId], results: &[TypeId]) -> TypeId {
        self.intern_type(TypeKind::Function {
            params: params.to_vec(),
            results: results.to_vec(),
        })
    }

    pub fn extension_type(&mut self, data: ExtensionType) -> TypeId {
        self.intern_type(TypeKind::Extension(data))
    }

    pub fn type_kind(&self, ty: TypeId) -> &TypeKind {
        &self.types[ty]
    }

    pub fn is_integer_type(&self, ty: TypeId) -> bool {
        matches!(self.types[ty], TypeKind::Integer { .. })
    }

    pub fn is_float_type(&self, ty: TypeId) -> bool {
        matches!(self.types[ty], TypeKind::Float { .. })
    }

    pub fn is_function_type(&self, ty: TypeId) -> bool {
        matches!(self.types[ty], TypeKind::Function { .. })
    }

    pub fn is_none_type(&self, ty: TypeId) -> bool {
        matches!(self.types[ty], TypeKind::None)
    }

    pub fn is_index_type(&self, ty: TypeId) -> bool {
        matches!(self.types[ty], TypeKind::Index)
    }

    pub fn integer_type_width(&self, ty: TypeId) -> Option<u32> {
        match &self.types[ty] {
            TypeKind::Integer { width } => Some(*width),
            _ => None,
        }
    }

    pub fn float_type_width(&self, ty: TypeId) -> Option<u32> {
        match &self.types[ty] {
            TypeKind::Float { width } => Some(*width),
            _ => None,
        }
    }

    pub fn function_type_params(&self, ty: TypeId) -> Option<&[TypeId]> {
        match &self.types[ty] {
            TypeKind::Function { params, .. } => Some(params),
            _ => None,
        }
    }

    pub fn function_type_results(&self, ty: TypeId) -> Option<&[TypeId]> {
        match &self.types[ty] {
            TypeKind::Function { results, .. } => Some(results),
            _ => None,
        }
    }

    /// Format a type for display, resolving TypeIds through the context.
    pub fn format_type(&self, ty: TypeId) -> String {
        match &self.types[ty] {
            TypeKind::None => "none".into(),
            TypeKind::Integer { width } => format!("i{}", width),
            TypeKind::Float { width } => format!("f{}", width),
            TypeKind::Index => "index".into(),
            TypeKind::Function { params, results } => {
                let p: Vec<String> = params.iter().map(|&t| self.format_type(t)).collect();
                let r: Vec<String> = results.iter().map(|&t| self.format_type(t)).collect();
                format!("({}) -> ({})", p.join(", "), r.join(", "))
            }
            TypeKind::Extension(ext) => {
                let mut s = format!("!{}.{}", ext.dialect, ext.name);
                let has_params = !ext.type_params.is_empty()
                    || !ext.int_params.is_empty()
                    || !ext.string_params.is_empty();
                if has_params {
                    s.push('<');
                    let mut parts = Vec::new();
                    for sp in &ext.string_params {
                        parts.push(format!("\"{}\"", sp));
                    }
                    for &ip in &ext.int_params {
                        parts.push(format!("{}", ip));
                    }
                    for &tp in &ext.type_params {
                        parts.push(self.format_type(tp));
                    }
                    s.push_str(&parts.join(", "));
                    s.push('>');
                }
                s
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

impl Context {
    /// Convenience: get the type of a value.
    pub fn value_type(&self, value: ValueId) -> TypeId {
        self.values[value].ty
    }

    /// Get all uses of a value.
    pub fn value_uses(&self, value: ValueId) -> &[Use] {
        &self.values[value].uses
    }

    /// Check if a value has any uses.
    pub fn value_has_uses(&self, value: ValueId) -> bool {
        !self.values[value].uses.is_empty()
    }

    /// Replace all uses of `old` with `new`. O(uses of old).
    pub fn replace_all_uses(&mut self, old: ValueId, new: ValueId) {
        let uses = std::mem::take(&mut self.values[old].uses);
        for u in &uses {
            self.ops[u.user].operands[u.operand_index as usize] = new;
        }
        self.values[new].uses.extend_from_slice(&uses);
    }
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

impl Context {
    /// Create an operation (not yet inserted into any block).
    ///
    /// Result values are automatically allocated from `result_types`.
    /// Use-def chains for operands are updated immediately.
    pub fn create_operation(
        &mut self,
        name: &str,
        operands: &[ValueId],
        result_types: &[TypeId],
        attributes: Vec<NamedAttribute>,
        regions: Vec<RegionId>,
        location: Location,
    ) -> OpId {
        // Allocate the operation first (with empty results).
        let op_id = self.ops.push(OperationData {
            name: name.to_string(),
            operands: operands.to_vec(),
            results: Vec::new(),
            attributes,
            regions: regions.clone(),
            location,
            parent_block: None,
            prev_op: None,
            next_op: None,
        });

        // Create result values referencing this operation.
        let results: Vec<ValueId> = result_types
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                self.values.push(ValueData {
                    ty,
                    kind: ValueKind::OpResult {
                        op: op_id,
                        index: i as u32,
                    },
                    uses: Vec::new(),
                })
            })
            .collect();
        self.ops[op_id].results = results;

        // Update use-def chains: register this op as a user of each operand.
        for (i, &operand) in operands.iter().enumerate() {
            self.values[operand].uses.push(Use {
                user: op_id,
                operand_index: i as u32,
            });
        }

        // Set parent_op on each region.
        for &region in &regions {
            self.regions[region].parent_op = Some(op_id);
        }

        op_id
    }

    /// Convenience: get result #index of an operation.
    pub fn op_result(&self, op: OpId, index: usize) -> ValueId {
        self.ops[op].results[index]
    }

    /// Convenience: get the first region of an operation.
    pub fn op_region(&self, op: OpId, index: usize) -> RegionId {
        self.ops[op].regions[index]
    }

    /// Set an attribute on an operation (safe — does not affect use-def chains).
    pub fn op_set_attribute(&mut self, op: OpId, name: impl Into<String>, value: super::attributes::Attribute) {
        self.ops[op].set_attribute(name, value);
    }

    /// Remove an attribute from an operation by name.
    pub fn op_remove_attribute(&mut self, op: OpId, name: &str) -> bool {
        self.ops[op].remove_attribute(name)
    }

    /// Detach an operation from its parent block and remove its use-def entries.
    /// The operation's arena slot remains allocated but unreachable.
    pub fn detach_op(&mut self, op: OpId) {
        // Remove from use-def chains.
        let operands = self.ops[op].operands.clone();
        for (i, operand) in operands.iter().enumerate() {
            self.values[*operand]
                .uses
                .retain(|u| !(u.user == op && u.operand_index == i as u32));
        }

        // Unlink from parent block.
        if self.ops[op].parent_block.is_some() {
            self.unlink_op(op);
        }
    }

    /// Erase an operation: detach it and recursively erase its contents.
    pub fn erase_op(&mut self, op: OpId) {
        // Recursively erase nested regions.
        let regions = self.ops[op].regions.clone();
        for region in regions {
            self.erase_region(region);
        }
        self.detach_op(op);
    }

    fn erase_region(&mut self, region: RegionId) {
        let blocks = self.regions[region].blocks.clone();
        for block in blocks {
            self.erase_block(block);
        }
    }

    fn erase_block(&mut self, block: BlockId) {
        // Erase all operations in the block.
        let mut current = self.blocks[block].first_op;
        while let Some(op) = current {
            let next = self.ops[op].next_op;
            self.erase_op(op);
            current = next;
        }
    }
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

impl Context {
    /// Create an empty block (no arguments, no operations).
    pub fn create_block(&mut self) -> BlockId {
        self.blocks.push(BlockData {
            arguments: Vec::new(),
            first_op: None,
            last_op: None,
            parent_region: None,
        })
    }

    /// Create a block with arguments of the given types.
    pub fn create_block_with_args(&mut self, arg_types: &[TypeId]) -> BlockId {
        let block = self.create_block();
        for &ty in arg_types {
            self.block_add_argument(block, ty);
        }
        block
    }

    /// Add an argument to a block. Returns the new ValueId.
    pub fn block_add_argument(&mut self, block: BlockId, ty: TypeId) -> ValueId {
        let index = self.blocks[block].arguments.len() as u32;
        let value = self.values.push(ValueData {
            ty,
            kind: ValueKind::BlockArg { block, index },
            uses: Vec::new(),
        });
        self.blocks[block].arguments.push(value);
        value
    }

    /// Append an operation to the end of a block (O(1)).
    pub fn block_push_op(&mut self, block: BlockId, op: OpId) {
        self.link_op_at_end(block, op);
    }

    /// Insert an operation before another operation in its block (O(1)).
    pub fn block_insert_op_before(&mut self, before: OpId, op: OpId) {
        let block = self.ops[before]
            .parent_block
            .expect("'before' op must be in a block");
        let prev = self.ops[before].prev_op;

        self.ops[op].next_op = Some(before);
        self.ops[op].prev_op = prev;
        self.ops[before].prev_op = Some(op);

        if let Some(prev_id) = prev {
            self.ops[prev_id].next_op = Some(op);
        } else {
            self.blocks[block].first_op = Some(op);
        }
        self.ops[op].parent_block = Some(block);
    }

    /// Insert an operation after another operation in its block (O(1)).
    pub fn block_insert_op_after(&mut self, after: OpId, op: OpId) {
        let block = self.ops[after]
            .parent_block
            .expect("'after' op must be in a block");
        let next = self.ops[after].next_op;

        self.ops[op].prev_op = Some(after);
        self.ops[op].next_op = next;
        self.ops[after].next_op = Some(op);

        if let Some(next_id) = next {
            self.ops[next_id].prev_op = Some(op);
        } else {
            self.blocks[block].last_op = Some(op);
        }
        self.ops[op].parent_block = Some(block);
    }

    /// Iterate operations in a block (forward, following the linked list).
    pub fn block_ops(&self, block: BlockId) -> BlockOpIter<'_> {
        BlockOpIter {
            ctx: self,
            current: self.blocks[block].first_op,
        }
    }

    /// Iterate operations in a block in reverse.
    pub fn block_ops_rev(&self, block: BlockId) -> BlockOpIterRev<'_> {
        BlockOpIterRev {
            ctx: self,
            current: self.blocks[block].last_op,
        }
    }

    /// Convenience: get block argument by index.
    pub fn block_argument(&self, block: BlockId, index: usize) -> ValueId {
        self.blocks[block].arguments[index]
    }

    // ---- Internal linked-list operations ----

    fn link_op_at_end(&mut self, block: BlockId, op: OpId) {
        let last = self.blocks[block].last_op;
        if let Some(last_id) = last {
            self.ops[last_id].next_op = Some(op);
            self.ops[op].prev_op = Some(last_id);
        } else {
            self.blocks[block].first_op = Some(op);
        }
        self.ops[op].next_op = None;
        self.blocks[block].last_op = Some(op);
        self.ops[op].parent_block = Some(block);
    }

    fn unlink_op(&mut self, op: OpId) {
        let block = self.ops[op]
            .parent_block
            .expect("op must be in a block to unlink");
        let prev = self.ops[op].prev_op;
        let next = self.ops[op].next_op;

        if let Some(prev_id) = prev {
            self.ops[prev_id].next_op = next;
        } else {
            self.blocks[block].first_op = next;
        }

        if let Some(next_id) = next {
            self.ops[next_id].prev_op = prev;
        } else {
            self.blocks[block].last_op = prev;
        }

        self.ops[op].parent_block = None;
        self.ops[op].prev_op = None;
        self.ops[op].next_op = None;
    }
}

// ---------------------------------------------------------------------------
// Regions
// ---------------------------------------------------------------------------

impl Context {
    /// Create an empty region.
    pub fn create_region(&mut self) -> RegionId {
        self.regions.push(RegionData {
            blocks: Vec::new(),
            parent_op: None,
        })
    }

    /// Append a block to a region. Sets the block's parent.
    pub fn region_push_block(&mut self, region: RegionId, block: BlockId) {
        self.regions[region].blocks.push(block);
        self.blocks[block].parent_region = Some(region);
    }

    /// Convenience: get the entry block of a region.
    pub fn region_entry_block(&self, region: RegionId) -> Option<BlockId> {
        self.regions[region].blocks.first().copied()
    }
}

// ---------------------------------------------------------------------------
// Walk — immutable traversal over the operation tree
// ---------------------------------------------------------------------------

impl Context {
    /// Walk the operation tree rooted at `op` in the given order.
    /// The callback receives each OpId and a reference to this Context.
    pub fn walk(
        &self,
        op: OpId,
        order: WalkOrder,
        cb: &mut dyn FnMut(OpId, &Context) -> WalkResult,
    ) -> WalkResult {
        match order {
            WalkOrder::PreOrder => {
                match cb(op, self) {
                    WalkResult::Skip => return WalkResult::Advance,
                    WalkResult::Interrupt => return WalkResult::Interrupt,
                    WalkResult::Advance => {}
                }
                self.walk_children(op, order, cb)
            }
            WalkOrder::PostOrder => {
                match self.walk_children(op, order, cb) {
                    WalkResult::Interrupt => return WalkResult::Interrupt,
                    _ => {}
                }
                cb(op, self)
            }
        }
    }

    fn walk_children(
        &self,
        op: OpId,
        order: WalkOrder,
        cb: &mut dyn FnMut(OpId, &Context) -> WalkResult,
    ) -> WalkResult {
        // Clone region IDs to avoid holding a borrow during recursion.
        let regions: Vec<RegionId> = self.ops[op].regions.clone();
        for region in regions {
            let blocks: Vec<BlockId> = self.regions[region].blocks.clone();
            for block in blocks {
                let mut current = self.blocks[block].first_op;
                while let Some(inner_op) = current {
                    // Save next before callback (callback might erase inner_op).
                    current = self.ops[inner_op].next_op;
                    if self.walk(inner_op, order, cb) == WalkResult::Interrupt {
                        return WalkResult::Interrupt;
                    }
                }
            }
        }
        WalkResult::Advance
    }
}

// ---------------------------------------------------------------------------
// Dialects
// ---------------------------------------------------------------------------

impl Context {
    pub fn register_dialect(&mut self, dialect: Dialect) {
        self.dialects.push(dialect);
    }

    pub fn get_dialect(&self, name: &str) -> Option<&Dialect> {
        self.dialects.iter().find(|d| d.name() == name)
    }

    pub fn dialects(&self) -> &[Dialect] {
        &self.dialects
    }
}

// ---------------------------------------------------------------------------
// IR printing
// ---------------------------------------------------------------------------

impl Context {
    /// Print an operation and its nested IR to a string.
    pub fn print_op(&self, op: OpId) -> String {
        let mut out = String::new();
        self.print_op_inner(op, &mut out, 0);
        out
    }

    fn print_op_inner(&self, op: OpId, out: &mut String, indent: usize) {
        let data = &self.ops[op];
        let pad = "  ".repeat(indent);

        // Print results
        if !data.results.is_empty() {
            let results: Vec<String> = data
                .results
                .iter()
                .map(|&v| format!("%{}", v.index()))
                .collect();
            out.push_str(&format!("{}{} = ", pad, results.join(", ")));
        } else {
            out.push_str(&pad);
        }

        // Op name
        out.push_str(&format!("\"{}\"", data.name));

        // Operands
        if !data.operands.is_empty() {
            let operands: Vec<String> = data
                .operands
                .iter()
                .map(|&v| format!("%{}", v.index()))
                .collect();
            out.push_str(&format!("({})", operands.join(", ")));
        }

        // Attributes
        if !data.attributes.is_empty() {
            let attrs: Vec<String> = data
                .attributes
                .iter()
                .map(|a| format!("{} = {}", a.name, self.format_attribute(&a.value)))
                .collect();
            out.push_str(&format!(" {{{}}}", attrs.join(", ")));
        }

        // Result types
        if !data.results.is_empty() {
            let types: Vec<String> = data
                .results
                .iter()
                .map(|&v| self.format_type(self.values[v].ty))
                .collect();
            out.push_str(&format!(" : {}", types.join(", ")));
        }

        // Regions
        if data.regions.is_empty() {
            out.push('\n');
        } else {
            for &region in &data.regions {
                out.push_str(" {\n");
                for &block in &self.regions[region].blocks {
                    self.print_block(block, out, indent + 1);
                }
                out.push_str(&format!("{}}}\n", pad));
            }
        }
    }

    fn print_block(&self, block: BlockId, out: &mut String, indent: usize) {
        let data = &self.blocks[block];
        let pad = "  ".repeat(indent);

        // Block header with arguments
        if data.arguments.is_empty() {
            out.push_str(&format!("{}{}:\n", pad, block));
        } else {
            let args: Vec<String> = data
                .arguments
                .iter()
                .map(|&v| format!("%{}: {}", v.index(), self.format_type(self.values[v].ty)))
                .collect();
            out.push_str(&format!("{}{}({}):\n", pad, block, args.join(", ")));
        }

        // Operations
        let mut current = data.first_op;
        while let Some(op) = current {
            self.print_op_inner(op, out, indent + 1);
            current = self.ops[op].next_op;
        }
    }

    fn format_attribute(&self, attr: &super::attributes::Attribute) -> String {
        use super::attributes::Attribute;
        match attr {
            Attribute::Integer { value, ty } => {
                format!("{} : {}", value, self.format_type(*ty))
            }
            Attribute::Float { value, ty } => {
                format!("{} : {}", value, self.format_type(*ty))
            }
            Attribute::String(s) => format!("\"{}\"", s),
            Attribute::Bool(b) => format!("{}", b),
            Attribute::Type(ty) => self.format_type(*ty),
            Attribute::SymbolRef(s) => format!("@{}", s),
            Attribute::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|a| self.format_attribute(a)).collect();
                format!("[{}]", items.join(", "))
            }
            Attribute::Unit => "unit".into(),
        }
    }
}
