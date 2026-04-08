use crate::entity::{OpId, RegionId, ValueId};

/// Data stored for each block in the arena.
///
/// Operations within a block form a doubly-linked list (via OpId prev/next
/// on OperationData). The block tracks the head and tail of this list.
pub struct BlockData {
    pub(crate) arguments: Vec<ValueId>,
    pub(crate) first_op: Option<OpId>,
    pub(crate) last_op: Option<OpId>,
    pub(crate) parent_region: Option<RegionId>,
}

impl BlockData {
    pub fn arguments(&self) -> &[ValueId] {
        &self.arguments
    }
    pub fn argument(&self, index: usize) -> ValueId {
        self.arguments[index]
    }
    pub fn num_arguments(&self) -> usize {
        self.arguments.len()
    }
    pub fn first_op(&self) -> Option<OpId> {
        self.first_op
    }
    pub fn last_op(&self) -> Option<OpId> {
        self.last_op
    }
    pub fn parent_region(&self) -> Option<RegionId> {
        self.parent_region
    }
    pub fn is_empty(&self) -> bool {
        self.first_op.is_none()
    }
}

/// Iterator over operations in a block, following the linked list.
pub struct BlockOpIter<'a> {
    pub(crate) ctx: &'a super::context::Context,
    pub(crate) current: Option<OpId>,
}

impl<'a> Iterator for BlockOpIter<'a> {
    type Item = OpId;
    fn next(&mut self) -> Option<OpId> {
        let current = self.current?;
        self.current = self.ctx[current].next_op();
        Some(current)
    }
}

/// Reverse iterator over operations in a block.
pub struct BlockOpIterRev<'a> {
    pub(crate) ctx: &'a super::context::Context,
    pub(crate) current: Option<OpId>,
}

impl<'a> Iterator for BlockOpIterRev<'a> {
    type Item = OpId;
    fn next(&mut self) -> Option<OpId> {
        let current = self.current?;
        self.current = self.ctx[current].prev_op();
        Some(current)
    }
}

