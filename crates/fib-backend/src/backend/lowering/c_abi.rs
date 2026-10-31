//! Linux x86-64 System V classification for the supported C data subset.
//! This module contains no LLVM values. Definitions and calls use the same plan.
use super::{BackendError, abi::AbiTarget};
use crate::ir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Integer(u16),
    Float32,
    Float64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub offset: u64,
    pub kind: PieceKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Boundary {
    Direct,
    Coerce(Vec<Piece>),
    Indirect { alignment: u32 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSignature {
    pub arguments: Vec<Boundary>,
    pub result: Option<Boundary>,
}
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub size: u64,
    pub alignment: u32,
}
fn error(message: &str) -> BackendError {
    BackendError::Invalid {
        message: message.into(),
    }
}
fn round(size: u64, alignment: u32) -> Result<u64, BackendError> {
    size.checked_add(u64::from(alignment) - 1)
        .map(|s| s / u64::from(alignment) * u64::from(alignment))
        .ok_or_else(|| error("C layout overflow"))
}
pub fn layout(types: &[IrType], ty: TypeId) -> Result<Layout, BackendError> {
    fn visit(
        types: &[IrType],
        ty: TypeId,
        active: &mut Vec<TypeId>,
    ) -> Result<Layout, BackendError> {
        if active.contains(&ty) {
            return Err(error("recursive by-value C layout"));
        }
        active.push(ty);
        let result = match types.get(ty.0) {
            Some(IrType::Bool) => Layout {
                size: 1,
                alignment: 1,
            },
            Some(IrType::Integer {
                width: IntegerWidth::Bits(w),
                ..
            }) if [8, 16, 32, 64, 128].contains(w) => Layout {
                size: u64::from(*w) / 8,
                alignment: u32::from(*w) / 8,
            },
            Some(
                IrType::Integer {
                    width: IntegerWidth::Pointer,
                    ..
                }
                | IrType::Pointer { .. }
                | IrType::FunctionPointer(_),
            ) => Layout {
                size: 8,
                alignment: 8,
            },
            Some(IrType::Float(FloatWidth::F32)) => Layout {
                size: 4,
                alignment: 4,
            },
            Some(IrType::Float(FloatWidth::F64)) => Layout {
                size: 8,
                alignment: 8,
            },
            Some(IrType::Array { element, len }) => {
                let e = visit(types, *element, active)?;
                Layout {
                    size: e
                        .size
                        .checked_mul(*len)
                        .ok_or_else(|| error("C array layout overflow"))?,
                    alignment: e.alignment,
                }
            }
            Some(IrType::Struct { fields, .. }) if !fields.is_empty() => {
                let mut size = 0;
                let mut alignment = 1;
                for field in fields {
                    let f = visit(types, *field, active)?;
                    size = round(size, f.alignment)?
                        .checked_add(f.size)
                        .ok_or_else(|| error("C struct layout overflow"))?;
                    alignment = alignment.max(f.alignment);
                }
                Layout {
                    size: round(size, alignment)?,
                    alignment,
                }
            }
            _ => {
                return Err(error(
                    "unsupported C ABI type; use a C wrapper with supported scalar/pointer/struct types",
                ));
            }
        };
        active.pop();
        Ok(result)
    }
    visit(types, ty, &mut Vec::new())
}
fn boundary(types: &[IrType], ty: TypeId) -> Result<Boundary, BackendError> {
    let l = layout(types, ty)?;
    if !matches!(types[ty.0], IrType::Struct { .. }) {
        return Ok(Boundary::Direct);
    }
    if l.size > 16 {
        return Ok(Boundary::Indirect {
            alignment: l.alignment,
        });
    }
    // INTEGER dominates SSE in a shared eightbyte. Track occupied byte extents
    // so tail padding is not read as part of a physical scalar.
    let mut classes = [(0u8, 0u64); 2];
    fn mark(
        types: &[IrType],
        ty: TypeId,
        offset: u64,
        classes: &mut [(u8, u64); 2],
    ) -> Result<(), BackendError> {
        match &types[ty.0] {
            IrType::Struct { fields, .. } => {
                let mut at = offset;
                for field in fields {
                    let l = layout(types, *field)?;
                    at = round(at, l.alignment)?;
                    mark(types, *field, at, classes)?;
                    at += l.size;
                }
            }
            IrType::Array { element, len } => {
                let l = layout(types, *element)?;
                for i in 0..*len {
                    mark(types, *element, offset + i * l.size, classes)?;
                }
            }
            t => {
                let l = layout(types, ty)?;
                let class = if matches!(t, IrType::Float(_)) { 1 } else { 2 };
                for byte in offset..offset + l.size {
                    let slot = &mut classes[(byte / 8) as usize];
                    slot.0 = slot.0.max(class);
                    slot.1 = slot.1.max(byte % 8 + 1);
                }
            }
        }
        Ok(())
    }
    mark(types, ty, 0, &mut classes)?;
    let pieces = classes
        .iter()
        .enumerate()
        .filter(|(_, (class, _))| *class != 0)
        .map(|(i, (class, bytes))| Piece {
            offset: i as u64 * 8,
            kind: if *class == 2 {
                PieceKind::Integer((*bytes * 8) as u16)
            } else if *bytes <= 4 {
                PieceKind::Float32
            } else {
                PieceKind::Float64
            },
        })
        .collect();
    Ok(Boundary::Coerce(pieces))
}
pub fn classify(
    signature: &Signature,
    types: &[IrType],
    target: &AbiTarget,
) -> Result<CSignature, BackendError> {
    if !target.triple.starts_with("x86_64-")
        || !target.triple.contains("linux")
        || target.data_layout.is_empty()
    {
        return Err(error("external C ABI currently requires Linux x86-64"));
    }
    if signature.results.len() > 1 {
        return Err(error("C functions require one result or void"));
    }
    let result = signature
        .results
        .first()
        .map(|t| boundary(types, *t))
        .transpose()?;
    let mut integers = usize::from(matches!(result, Some(Boundary::Indirect { .. })));
    let mut floats = 0;
    let mut arguments = Vec::new();
    for ty in &signature.params {
        let mut b = boundary(types, *ty)?;
        if matches!(types[ty.0], IrType::Array { .. }) {
            return Err(error("C array parameters require explicit pointers"));
        }
        let (ni, nf) = match &b {
            Boundary::Indirect { .. } => (0, 0),
            Boundary::Coerce(pieces) => (
                pieces
                    .iter()
                    .filter(|p| matches!(p.kind, PieceKind::Integer(_)))
                    .count(),
                pieces
                    .iter()
                    .filter(|p| !matches!(p.kind, PieceKind::Integer(_)))
                    .count(),
            ),
            Boundary::Direct => {
                if matches!(types[ty.0], IrType::Float(_)) {
                    (0, 1)
                } else if matches!(
                    types[ty.0],
                    IrType::Integer {
                        width: IntegerWidth::Bits(128),
                        ..
                    }
                ) {
                    (2, 0)
                } else {
                    (1, 0)
                }
            }
        };
        if matches!(b, Boundary::Coerce(_)) && (integers + ni > 6 || floats + nf > 8) {
            b = Boundary::Indirect {
                alignment: layout(types, *ty)?.alignment,
            };
        } else {
            integers += ni;
            floats += nf;
        }
        arguments.push(b);
    }
    if signature
        .results
        .first()
        .is_some_and(|t| matches!(types[t.0], IrType::Array { .. }))
    {
        return Err(error("C cannot return arrays by value"));
    }
    Ok(CSignature { arguments, result })
}
