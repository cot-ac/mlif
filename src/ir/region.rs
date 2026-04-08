use crate::entity::{BlockId, OpId};

/// Data stored for each region in the arena.
///
/// A region is an ordered list of blocks, owned by a parent operation.
pub struct RegionData {
    pub(crate) blocks: Vec<BlockId>,
    pub(crate) parent_op: Option<OpId>,
}

impl RegionData {
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }
    pub fn entry_block(&self) -> Option<BlockId> {
        self.blocks.first().copied()
    }
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    pub fn parent_op(&self) -> Option<OpId> {
        self.parent_op
    }
    pub fn block(&self, index: usize) -> BlockId {
        self.blocks[index]
    }
}

