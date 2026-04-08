use std::collections::HashMap;

use crate::entity::OpId;

use super::attributes::Attribute;
use super::context::Context;

/// Lookup table for named operations (functions, globals).
/// Built by scanning operations with a "sym_name" string attribute.
pub struct SymbolTable {
    symbols: HashMap<String, OpId>,
}

impl SymbolTable {
    /// Build a symbol table by scanning all operations in a block.
    pub fn build(ctx: &Context, ops: impl Iterator<Item = OpId>) -> Self {
        let mut symbols = HashMap::new();
        for op_id in ops {
            if let Some(Attribute::String(name)) = ctx[op_id].get_attribute("sym_name") {
                symbols.insert(name.clone(), op_id);
            }
        }
        Self { symbols }
    }

    /// Look up a symbol by name, returning its OpId.
    pub fn lookup(&self, name: &str) -> Option<OpId> {
        self.symbols.get(name).copied()
    }

    /// Check if a symbol exists.
    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Iterate over all symbol names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.symbols.keys()
    }
}
