/// Result of a walk callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkResult {
    /// Continue walking.
    Advance,
    /// Skip this operation's children (PreOrder only).
    Skip,
    /// Stop walking entirely.
    Interrupt,
}

/// Walk order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkOrder {
    PreOrder,
    PostOrder,
}

// Walk implementation lives on Context (see context.rs) because the walker
// needs to traverse the arena. Re-exported here for the public API.
