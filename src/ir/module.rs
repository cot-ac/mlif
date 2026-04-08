use crate::entity::{BlockId, OpId, RegionId};

use super::context::Context;
use super::location::Location;

/// Top-level module container. A module is a "builtin.module" operation
/// with a single region containing one block (the module body).
///
/// Module is a thin handle — the actual data lives in the Context arena.
pub struct Module {
    op: OpId,
}

impl Module {
    /// Create a new empty module. Allocates the operation, region, and
    /// body block in the given Context.
    pub fn new(ctx: &mut Context, location: Location) -> Self {
        let block = ctx.create_block();
        let region = ctx.create_region();
        ctx.region_push_block(region, block);
        let op = ctx.create_operation(
            "builtin.module",
            &[],
            &[],
            vec![],
            vec![region],
            location,
        );
        Self { op }
    }

    /// The module's root operation.
    pub fn op(&self) -> OpId {
        self.op
    }

    /// The module's body region.
    pub fn body(&self, ctx: &Context) -> RegionId {
        ctx[self.op].region(0)
    }

    /// The module's body block (the single block inside the body region).
    pub fn body_block(&self, ctx: &Context) -> BlockId {
        ctx[self.body(ctx)].entry_block().unwrap()
    }
}
