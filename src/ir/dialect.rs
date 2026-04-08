use std::collections::HashMap;

/// A trait that operations can have, affecting verification and optimization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpTrait {
    /// Operation is a terminator (must be last in a block).
    Terminator,
    /// Operation has no side effects.
    Pure,
    /// Operation is commutative (operand order doesn't matter).
    Commutative,
    /// All operands and results have the same type.
    SameOperandsAndResultType,
}

/// Definition of an operation within a dialect.
#[derive(Clone, Debug)]
pub struct OpDefinition {
    pub name: String,
    pub traits: Vec<OpTrait>,
}

impl OpDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            traits: Vec::new(),
        }
    }

    pub fn with_trait(mut self, t: OpTrait) -> Self {
        self.traits.push(t);
        self
    }

    pub fn has_trait(&self, t: &OpTrait) -> bool {
        self.traits.contains(t)
    }

    pub fn is_terminator(&self) -> bool {
        self.has_trait(&OpTrait::Terminator)
    }
}

/// A dialect groups related operations and types under a namespace.
#[derive(Clone, Debug)]
pub struct Dialect {
    name: String,
    ops: HashMap<String, OpDefinition>,
}

impl Dialect {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ops: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn register_op(&mut self, def: OpDefinition) {
        self.ops.insert(def.name.clone(), def);
    }

    pub fn get_op(&self, name: &str) -> Option<&OpDefinition> {
        self.ops.get(name)
    }

    pub fn ops(&self) -> &HashMap<String, OpDefinition> {
        &self.ops
    }
}
