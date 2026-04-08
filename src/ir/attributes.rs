use crate::entity::TypeId;

/// Named attribute: a (name, value) pair attached to an operation.
#[derive(Clone, Debug)]
pub struct NamedAttribute {
    pub name: String,
    pub value: Attribute,
}

impl NamedAttribute {
    pub fn new(name: impl Into<String>, value: Attribute) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// Compile-time metadata attached to operations.
#[derive(Clone, Debug)]
pub enum Attribute {
    /// Integer value with associated type.
    Integer { value: i64, ty: TypeId },
    /// Floating-point value with associated type.
    Float { value: f64, ty: TypeId },
    /// String value.
    String(String),
    /// Boolean value.
    Bool(bool),
    /// Type reference.
    Type(TypeId),
    /// Symbol reference (e.g., @function_name).
    SymbolRef(String),
    /// Array of attributes.
    Array(Vec<Attribute>),
    /// Unit attribute (marker, no value).
    Unit,
}
