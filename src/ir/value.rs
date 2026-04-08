use crate::entity::{BlockId, OpId, TypeId};

/// Data stored for each SSA value in the arena.
pub struct ValueData {
    pub(crate) ty: TypeId,
    pub(crate) kind: ValueKind,
    /// All uses of this value (operations that reference it as an operand).
    pub(crate) uses: Vec<Use>,
}

impl ValueData {
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    pub fn kind(&self) -> &ValueKind {
        &self.kind
    }
    pub fn uses(&self) -> &[Use] {
        &self.uses
    }
    pub fn num_uses(&self) -> usize {
        self.uses.len()
    }
    pub fn has_uses(&self) -> bool {
        !self.uses.is_empty()
    }
}

/// Where a value is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// Result #index of the given operation.
    OpResult { op: OpId, index: u32 },
    /// Argument #index of the given block.
    BlockArg { block: BlockId, index: u32 },
}

impl ValueKind {
    /// If this is an op result, return the defining operation.
    pub fn defining_op(&self) -> Option<OpId> {
        match self {
            ValueKind::OpResult { op, .. } => Some(*op),
            ValueKind::BlockArg { .. } => None,
        }
    }

    /// If this is a block argument, return the owning block.
    pub fn defining_block(&self) -> Option<BlockId> {
        match self {
            ValueKind::BlockArg { block, .. } => Some(*block),
            ValueKind::OpResult { .. } => None,
        }
    }
}

/// A single use of a value: "operand #operand_index of operation `user`".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Use {
    pub user: OpId,
    pub operand_index: u32,
}

/// Iterator over the users (operations) of a value.
pub struct UserIter<'a> {
    inner: std::slice::Iter<'a, Use>,
}

impl<'a> Iterator for UserIter<'a> {
    type Item = &'a Use;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl ValueData {
    pub fn users(&self) -> UserIter<'_> {
        UserIter {
            inner: self.uses.iter(),
        }
    }
}

// ValueId Display is provided by the entity macro ("%N" format).
