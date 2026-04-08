use crate::entity::TypeId;
use std::fmt;

/// Concrete type representation stored in the Context's type interner.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypeKind {
    /// Void / unit type.
    None,
    /// Integer type with a given bit width (1, 8, 16, 32, 64).
    Integer { width: u32 },
    /// Floating-point type (32 or 64 bit).
    Float { width: u32 },
    /// Target-dependent index type.
    Index,
    /// Function type: (params) -> (results).
    Function {
        params: Vec<TypeId>,
        results: Vec<TypeId>,
    },
    /// Parameterized type registered by a consumer dialect.
    /// Carries typed parameters so dialects can define rich types
    /// (e.g., !cir.struct<"Point", "x": f32, "y": f32>) without
    /// modifying MLIF itself.
    Extension(ExtensionType),
}

/// Data for a consumer-defined type. Interned by the Context alongside
/// built-in types.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ExtensionType {
    pub dialect: String,
    pub name: String,
    pub type_params: Vec<TypeId>,
    pub int_params: Vec<i64>,
    pub string_params: Vec<String>,
}

impl ExtensionType {
    pub fn new(dialect: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            dialect: dialect.into(),
            name: name.into(),
            type_params: Vec::new(),
            int_params: Vec::new(),
            string_params: Vec::new(),
        }
    }

    pub fn with_type_params(mut self, params: Vec<TypeId>) -> Self {
        self.type_params = params;
        self
    }

    pub fn with_int_params(mut self, params: Vec<i64>) -> Self {
        self.int_params = params;
        self
    }

    pub fn with_string_params(mut self, params: Vec<String>) -> Self {
        self.string_params = params;
        self
    }
}

impl fmt::Display for TypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeKind::None => write!(f, "none"),
            TypeKind::Integer { width } => write!(f, "i{}", width),
            TypeKind::Float { width } => write!(f, "f{}", width),
            TypeKind::Index => write!(f, "index"),
            TypeKind::Function { params, results } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> (")?;
                for (i, r) in results.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", r)?;
                }
                write!(f, ")")
            }
            TypeKind::Extension(ext) => {
                write!(f, "!{}.{}", ext.dialect, ext.name)?;
                let has_params = !ext.type_params.is_empty()
                    || !ext.int_params.is_empty()
                    || !ext.string_params.is_empty();
                if has_params {
                    write!(f, "<")?;
                    let mut first = true;
                    for s in &ext.string_params {
                        if !first {
                            write!(f, ", ")?;
                        }
                        write!(f, "\"{}\"", s)?;
                        first = false;
                    }
                    for &i in &ext.int_params {
                        if !first {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", i)?;
                        first = false;
                    }
                    for &t in &ext.type_params {
                        if !first {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", t)?;
                        first = false;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
        }
    }
}
