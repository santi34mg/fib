//! LLVM type mapping (extracted from `llvm_lower.rs`, phase 4 split).
//!
//! `map_type_to_llvm` is intentionally pure: `(&Ty, &Context, SymbolTable)`
//! with no builder state, so it can live here without lifetime
//! complications. Unsigned Fib types map to the same `iN` LLVM types as
//! their signed counterparts — LLVM integers carry no signedness; it lives
//! in the opcode (`UDiv` vs `SDiv`, `ULT` vs `SLT`, `zext` vs `sext`,
//! logical vs arithmetic shift). See `crate::ir::{ty_is_unsigned,
//! ty_is_float}` for the single source of truth used at use sites.

use std::error::Error;

use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::{BasicType, BasicTypeEnum};

use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{SymbolTable, Ty, TypedEnumVariant, TypedSymbol};

pub(crate) fn round_up(n: usize, align: usize) -> usize {
    if align <= 1 {
        n
    } else {
        n.div_ceil(align) * align
    }
}

/// Size and alignment of a Typed type in bytes, mirroring LLVM's struct layout
/// rules (fields aligned to their natural alignment, structs padded to the
/// largest field alignment). Used to size the payload region of tagged
/// unions — undersizing it would let payload stores write out of bounds.
pub(crate) fn typed_type_size_align(ty: &Ty, scope: &SymbolTable) -> (usize, usize) {
    match ty {
        Ty::Builtin(b) => {
            let s = match b {
                BuiltinType::Boolean | BuiltinType::Char => 1,
                BuiltinType::UInt1 | BuiltinType::Int1 | BuiltinType::Never => 1,
                BuiltinType::UInt2 | BuiltinType::Int2 | BuiltinType::Float2 => 2,
                BuiltinType::UInt4 | BuiltinType::Int4 | BuiltinType::Float4 => 4,
                BuiltinType::UInt8 | BuiltinType::Int8 | BuiltinType::Float8 => 8,
                BuiltinType::UInt16 | BuiltinType::Int16 | BuiltinType::Float16 => 16,
                BuiltinType::String => 8,
                BuiltinType::Void => 0,
            };
            (s, s.max(1))
        }
        Ty::Pointer(_) | Ty::Function { .. } => (8, 8),
        Ty::Array { element_type, size } => {
            let (s, a) = typed_type_size_align(element_type, scope);
            (round_up(s, a) * (*size as usize), a)
        }
        Ty::Struct { fields } => {
            struct_layout_size_align(fields.iter().map(|(_, t)| t.as_ref()), scope)
        }
        Ty::Tuple { elements } => struct_layout_size_align(elements.iter(), scope),
        Ty::Enum { variants } => {
            let payload = enum_max_payload_bytes(variants, scope);
            if payload == 0 {
                (4, 4)
            } else {
                // Matches the lowered shape `{ i32 tag, [N x i64] payload }`.
                let words = payload.div_ceil(8);
                (8 + words * 8, 8)
            }
        }
        Ty::Identifier(id) => match scope.lookup(id) {
            Some(TypedSymbol::Type(inner)) => typed_type_size_align(inner, scope),
            _ => (0, 1),
        },
        Ty::QualifiedIdentifier { module, name } => {
            if let Some(m) = scope.lookup_module(module)
                && let Some(TypedSymbol::Type(inner)) = m.exports.get(name)
            {
                typed_type_size_align(inner, scope)
            } else {
                (0, 1)
            }
        }
        Ty::Type => (0, 1),
    }
}

/// Lay out a sequence of field types like an LLVM struct and return the
/// padded total size and alignment.
pub(crate) fn struct_layout_size_align<'t>(
    field_types: impl Iterator<Item = &'t Ty>,
    scope: &SymbolTable,
) -> (usize, usize) {
    let mut offset = 0usize;
    let mut align = 1usize;
    for ty in field_types {
        let (s, a) = typed_type_size_align(ty, scope);
        offset = round_up(offset, a) + s;
        align = align.max(a);
    }
    (round_up(offset, align), align)
}

/// Returns the maximum padded payload size (in bytes) across the variants of
/// an enum, or 0 if the enum has no payload-carrying variants.
pub(crate) fn enum_max_payload_bytes(variants: &[TypedEnumVariant], scope: &SymbolTable) -> usize {
    variants
        .iter()
        .filter_map(|v| {
            v.payload
                .as_ref()
                .map(|fs| struct_layout_size_align(fs.iter().map(|(_, t)| t), scope).0)
        })
        .max()
        .unwrap_or(0)
}

pub(crate) fn map_type_to_llvm<'ctx>(
    ty: &Ty,
    ctx: &'ctx Context,
    current_scope: SymbolTable,
) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
    match ty {
        Ty::Builtin(builtin) => {
            let any_ty = match builtin {
                BuiltinType::Boolean => BasicTypeEnum::IntType(ctx.bool_type()),
                // Unsigned maps to the same `iN` as signed: signedness is
                // opcode-level (`UDiv`/`ULT`/`zext`/`lshr`). See
                // `crate::ir::ty_is_unsigned` at use sites.
                BuiltinType::UInt1 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::UInt2 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::UInt4 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::UInt8 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::UInt16 => BasicTypeEnum::IntType(ctx.i128_type()),
                BuiltinType::Int1 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::Int2 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::Int4 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::Int8 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::Int16 => BasicTypeEnum::IntType(ctx.i128_type()),
                BuiltinType::Float2 => BasicTypeEnum::FloatType(ctx.f16_type()),
                BuiltinType::Float4 => BasicTypeEnum::FloatType(ctx.f32_type()),
                BuiltinType::Float8 => BasicTypeEnum::FloatType(ctx.f64_type()),
                BuiltinType::Float16 => BasicTypeEnum::FloatType(ctx.f128_type()),
                BuiltinType::String => {
                    BasicTypeEnum::PointerType(ctx.ptr_type(AddressSpace::default()))
                }
                BuiltinType::Char => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::Never => BasicTypeEnum::IntType(ctx.bool_type()),
                BuiltinType::Void => return Err("void type cannot be used as a value type".into()),
            };
            Ok(any_ty)
        }
        Ty::Identifier(identifier) => {
            let symbol = current_scope
                .lookup(identifier)
                .ok_or_else(|| format!("identifier {} not found in current scope", identifier))?;
            if let TypedSymbol::Type(ty) = symbol {
                map_type_to_llvm(ty, ctx, current_scope.clone())
            } else {
                Err(format!("symbol {:?} is not a type", symbol).into())
            }
        }
        Ty::Struct { fields } => {
            let field_types: Vec<BasicTypeEnum> = fields
                .iter()
                .map(|(_, ty)| map_type_to_llvm(ty, ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            Ok(ctx.struct_type(&field_types, false).into())
        }
        Ty::Tuple { elements } => {
            let field_types: Vec<BasicTypeEnum> = elements
                .iter()
                .map(|ty| map_type_to_llvm(ty, ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            Ok(ctx.struct_type(&field_types, false).into())
        }
        Ty::Enum { variants } => {
            let payload_bytes = enum_max_payload_bytes(variants, &current_scope);
            if payload_bytes == 0 {
                Ok(ctx.i32_type().into())
            } else {
                // Use an i64-word payload so the region is 8-byte aligned;
                // variant payload structs are stored into it via struct GEPs.
                let tag_ty: BasicTypeEnum = ctx.i32_type().into();
                let payload_words = payload_bytes.div_ceil(8);
                let payload_ty: BasicTypeEnum =
                    ctx.i64_type().array_type(payload_words as u32).into();
                Ok(ctx.struct_type(&[tag_ty, payload_ty], false).into())
            }
        }
        Ty::Pointer(_) => Ok(ctx.ptr_type(AddressSpace::default()).into()),
        Ty::Array { element_type, size } => {
            let elem_ty = map_type_to_llvm(element_type, ctx, current_scope)?;
            Ok(elem_ty.array_type(*size as u32).into())
        }
        Ty::Function { .. } => {
            // Function pointers are opaque `ptr` in LLVM 16.
            Ok(ctx.ptr_type(AddressSpace::default()).into())
        }
        Ty::Type => {
            Err("compiler bug: Ty::Type reached LLVM lowering — comptime type values must not appear in runtime code".into())
        }
        Ty::QualifiedIdentifier { module, name } => {
            // Resolve through the scope's imported modules
            let module_data = current_scope.lookup_module(module).ok_or_else(|| {
                format!("map_type_to_llvm: module '{}' not found in scope", module)
            })?;
            let sym = module_data.exports.get(name).ok_or_else(|| {
                format!(
                    "map_type_to_llvm: '{}' not found in module '{}'",
                    name, module
                )
            })?;
            if let TypedSymbol::Type(inner_ty) = sym {
                let inner_ty = inner_ty.clone();
                map_type_to_llvm(&inner_ty, ctx, current_scope)
            } else {
                Err(format!("map_type_to_llvm: '{}::{}' is not a type", module, name).into())
            }
        }
    }
}
