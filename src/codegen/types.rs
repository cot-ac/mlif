//! CIR type -> Cranelift type mapping.
//!
//! Maps MLIF's TypeKind enum to Cranelift scalar types. Aggregate types
//! (struct, array, slice) return None — they need stack slots, not registers.

#![cfg(feature = "codegen")]

use cranelift_codegen::ir::types;

use crate::entity::TypeId;
use crate::ir::context::Context;
use crate::ir::types::TypeKind;

/// Check if a CIR type is an aggregate that gets passed as a pointer to stack data.
/// These types need sret (struct return) convention when returned from functions.
pub fn is_aggregate_type(ctx: &Context, ty: TypeId) -> bool {
    matches!(ctx.type_kind(ty), TypeKind::Extension(ext)
        if ext.dialect == "cir" && matches!(ext.name.as_str(),
            "optional" | "error_union" | "struct" | "array" | "slice" | "tagged_union"))
}

/// Map a CIR type to a Cranelift scalar type.
///
/// Returns `None` for aggregate types that must be lowered via stack slots
/// (structs, arrays, slices, etc.).
pub fn to_cranelift_type(ctx: &Context, ty: TypeId) -> Option<types::Type> {
    match ctx.type_kind(ty) {
        // i1 is promoted to i8 — Cranelift has no native i1.
        TypeKind::Integer { width: 1 } => Some(types::I8),
        TypeKind::Integer { width: 8 } => Some(types::I8),
        TypeKind::Integer { width: 16 } => Some(types::I16),
        TypeKind::Integer { width: 32 } => Some(types::I32),
        TypeKind::Integer { width: 64 } => Some(types::I64),
        TypeKind::Float { width: 32 } => Some(types::F32),
        TypeKind::Float { width: 64 } => Some(types::F64),
        TypeKind::Index => Some(types::I64),
        // Pointers and references are lowered as 64-bit integers.
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "ptr" => Some(types::I64),
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "ref" => Some(types::I64),
        // Aggregates passed as pointers to stack slots.
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "optional" => {
            Some(types::I64)
        }
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "error_union" => {
            Some(types::I64)
        }
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "slice" => {
            Some(types::I64)
        }
        // Enums are integer tags.
        TypeKind::Extension(ext) if ext.dialect == "cir" && ext.name == "enum" => {
            Some(types::I32)
        }
        _ => None, // struct, array, etc. -> stack slot
    }
}

/// Compute the byte size of a CIR type for Cranelift stack allocation.
///
/// Returns the size in bytes for scalar types and pointers. Aggregate types
/// (struct, array, slice) will need layout computation from their construct
/// crate — this function returns a pointer-sized default for unknown types.
pub fn type_byte_size(ctx: &Context, ty: TypeId) -> u32 {
    // Extract extension type info to avoid holding borrow during recursion.
    enum Info {
        Scalar(u32),
        CirExt { name: String, payload: Option<TypeId> },
    }

    let info = match ctx.type_kind(ty) {
        TypeKind::Integer { width } => Info::Scalar(((*width + 7) / 8) as u32),
        TypeKind::Float { width } => Info::Scalar((*width / 8) as u32),
        TypeKind::Index => Info::Scalar(8),
        TypeKind::Extension(ext) if ext.dialect == "cir" => Info::CirExt {
            name: ext.name.clone(),
            payload: ext.type_params.first().copied(),
        },
        _ => Info::Scalar(8),
    };

    match info {
        Info::Scalar(s) => s,
        Info::CirExt { ref name, payload } => match name.as_str() {
            "ptr" | "ref" => 8,
            "optional" => payload.map(|p| type_byte_size(ctx, p)).unwrap_or(8) + 1,
            "error_union" => payload.map(|p| type_byte_size(ctx, p)).unwrap_or(8) + 2,
            "slice" => 16, // {ptr: i64, len: i64}
            "enum" => 4,   // i32 tag
            _ => 8,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_types() {
        let mut ctx = Context::new();
        let i1 = ctx.integer_type(1);
        let i8 = ctx.integer_type(8);
        let i16 = ctx.integer_type(16);
        let i32 = ctx.integer_type(32);
        let i64 = ctx.integer_type(64);

        assert_eq!(to_cranelift_type(&ctx, i1), Some(types::I8));
        assert_eq!(to_cranelift_type(&ctx, i8), Some(types::I8));
        assert_eq!(to_cranelift_type(&ctx, i16), Some(types::I16));
        assert_eq!(to_cranelift_type(&ctx, i32), Some(types::I32));
        assert_eq!(to_cranelift_type(&ctx, i64), Some(types::I64));
    }

    #[test]
    fn test_float_types() {
        let mut ctx = Context::new();
        let f32_ty = ctx.float_type(32);
        let f64_ty = ctx.float_type(64);

        assert_eq!(to_cranelift_type(&ctx, f32_ty), Some(types::F32));
        assert_eq!(to_cranelift_type(&ctx, f64_ty), Some(types::F64));
    }

    #[test]
    fn test_index_type() {
        let mut ctx = Context::new();
        let idx = ctx.index_type();
        assert_eq!(to_cranelift_type(&ctx, idx), Some(types::I64));
    }

    #[test]
    fn test_none_type_returns_none() {
        let ctx = Context::new();
        let none = ctx.none_type();
        assert_eq!(to_cranelift_type(&ctx, none), None);
    }

    #[test]
    fn test_function_type_returns_none() {
        let mut ctx = Context::new();
        let i32_ty = ctx.integer_type(32);
        let fn_ty = ctx.function_type(&[i32_ty], &[i32_ty]);
        assert_eq!(to_cranelift_type(&ctx, fn_ty), None);
    }

    #[test]
    fn test_ptr_type() {
        use crate::ir::types::ExtensionType;
        let mut ctx = Context::new();
        let ptr_ty = ctx.extension_type(ExtensionType::new("cir", "ptr"));
        assert_eq!(to_cranelift_type(&ctx, ptr_ty), Some(types::I64));
    }

    #[test]
    fn test_ref_type() {
        use crate::ir::types::ExtensionType;
        let mut ctx = Context::new();
        let ref_ty = ctx.extension_type(ExtensionType::new("cir", "ref"));
        assert_eq!(to_cranelift_type(&ctx, ref_ty), Some(types::I64));
    }
}
