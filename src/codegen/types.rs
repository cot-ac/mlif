//! CIR type -> Cranelift type mapping.
//!
//! Maps MLIF's TypeKind enum to Cranelift scalar types. Aggregate types
//! (struct, array, slice) return None — they need stack slots, not registers.

#![cfg(feature = "codegen")]

use cranelift_codegen::ir::types;

use crate::entity::TypeId;
use crate::ir::context::Context;
use crate::ir::types::TypeKind;

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
        _ => None, // struct, array, slice, etc. -> stack slot
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
