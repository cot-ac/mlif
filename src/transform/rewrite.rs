// Rewrite utilities are methods on Context:
//   ctx.replace_all_uses(old, new)  — O(uses) via use-def chains
//   ctx.detach_op(op)               — unlink + remove uses
//   ctx.erase_op(op)                — detach + recursive erase
//
// These live on Context because they require mutable access to the arenas.
// This module re-exports them for discoverability; users should call them
// via `ctx.replace_all_uses(...)` etc.
