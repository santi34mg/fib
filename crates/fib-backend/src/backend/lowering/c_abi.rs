//! C calling-convention classification for the supported C data subset:
//! System V x86-64 (Linux and macOS) and AAPCS64 as used by Apple arm64.
//! This module contains no LLVM values. Definitions and calls use the same plan.
use super::{BackendError, abi::AbiTarget};
use crate::ir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Integer(u16),
    Float32,
    Float64,
    /// `[count x i64]`: AAPCS64 composites that occupy consecutive general
    /// registers, never split between registers and the stack.
    Integers64(u8),
    /// `[count x float]`/`[count x double]`: AAPCS64 homogeneous float
    /// aggregates, assigned to consecutive SIMD/FP registers as a unit.
    Floats32(u8),
    Floats64(u8),
}
impl PieceKind {
    /// Bytes read or written for this piece, including whole-register tails.
    pub fn size(self) -> u64 {
        match self {
            Self::Integer(bits) => u64::from(bits).div_ceil(8),
            Self::Float32 => 4,
            Self::Float64 => 8,
            Self::Integers64(n) | Self::Floats64(n) => 8 * u64::from(n),
            Self::Floats32(n) => 4 * u64::from(n),
        }
    }
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
    /// Passed by address. `byval` arguments are copied by the callee frame
    /// (System V); otherwise the caller passes a pointer to its own copy
    /// (AAPCS64). Indirect results always use `sret`.
    Indirect {
        alignment: u32,
        byval: bool,
    },
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
            byval: true,
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
/// The C conventions this classifier implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CConvention {
    SysV64,
    Aapcs64,
}

/// The C convention for a target triple, or an error naming the supported set.
pub fn convention(target: &AbiTarget) -> Result<CConvention, BackendError> {
    let triple = target.triple.as_str();
    let os = triple.contains("linux")
        || triple.contains("apple-darwin")
        || triple.contains("apple-macos");
    if target.data_layout.is_empty() || !os {
        return Err(error(&format!(
            "external C ABI is unsupported for target '{triple}'; supported: x86_64 Linux/macOS, aarch64 Linux/macOS"
        )));
    }
    if triple.starts_with("x86_64-") {
        Ok(CConvention::SysV64)
    } else if triple.starts_with("aarch64-") || triple.starts_with("arm64-") {
        Ok(CConvention::Aapcs64)
    } else {
        Err(error(&format!(
            "external C ABI is unsupported for target '{triple}'; supported: x86_64 Linux/macOS, aarch64 Linux/macOS"
        )))
    }
}

pub fn classify(
    signature: &Signature,
    types: &[IrType],
    target: &AbiTarget,
) -> Result<CSignature, BackendError> {
    let convention = convention(target)?;
    if signature.results.len() > 1 {
        return Err(error("C functions require one result or void"));
    }
    // Unknown type IDs fall through to layout(), which reports them.
    if signature
        .results
        .first()
        .is_some_and(|t| matches!(types.get(t.0), Some(IrType::Array { .. })))
    {
        return Err(error("C cannot return arrays by value"));
    }
    if signature
        .params
        .iter()
        .any(|t| matches!(types.get(t.0), Some(IrType::Array { .. })))
    {
        return Err(error("C array parameters require explicit pointers"));
    }
    if convention == CConvention::Aapcs64 {
        return classify_aapcs64(signature, types);
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
                byval: true,
            };
        } else {
            integers += ni;
            floats += nf;
        }
        arguments.push(b);
    }
    Ok(CSignature { arguments, result })
}

/// The common floating-point leaf type of a homogeneous float aggregate with
/// one to four leaves, as AAPCS64 defines it. None for any other composite.
fn homogeneous_float(
    types: &[IrType],
    ty: TypeId,
) -> Result<Option<(FloatWidth, u8)>, BackendError> {
    fn leaves(
        types: &[IrType],
        ty: TypeId,
        found: &mut Vec<FloatWidth>,
    ) -> Result<bool, BackendError> {
        match &types[ty.0] {
            IrType::Float(width) => {
                found.push(*width);
                Ok(true)
            }
            IrType::Struct { fields, .. } => {
                for field in fields {
                    if !leaves(types, *field, found)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            IrType::Array { element, len } => {
                for _ in 0..*len {
                    if !leaves(types, *element, found)? || found.len() > 4 {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    let mut found = Vec::new();
    if !leaves(types, ty, &mut found)? || found.is_empty() || found.len() > 4 {
        return Ok(None);
    }
    let width = found[0];
    if found.iter().any(|w| *w != width) {
        return Ok(None);
    }
    Ok(Some((width, found.len() as u8)))
}

/// AAPCS64 composite classification, matching clang for Apple arm64 and Linux:
/// homogeneous float aggregates travel in SIMD/FP registers as one array,
/// other composites up to 16 bytes in one or two general registers (one
/// 128-bit value when 16-byte aligned), and larger composites by reference to
/// a caller-owned copy. Results above 16 bytes use `sret` (x8).
fn aapcs64_boundary(types: &[IrType], ty: TypeId) -> Result<Boundary, BackendError> {
    let l = layout(types, ty)?;
    if !matches!(types[ty.0], IrType::Struct { .. }) {
        return Ok(Boundary::Direct);
    }
    if let Some((width, count)) = homogeneous_float(types, ty)? {
        let kind = match width {
            FloatWidth::F32 => PieceKind::Floats32(count),
            FloatWidth::F64 => PieceKind::Floats64(count),
            // layout() already rejects other widths at the C boundary.
            _ => return Err(error("unsupported C ABI float width")),
        };
        return Ok(Boundary::Coerce(vec![Piece { offset: 0, kind }]));
    }
    if l.size > 16 {
        return Ok(Boundary::Indirect {
            alignment: l.alignment,
            byval: false,
        });
    }
    let kind = if l.alignment >= 16 {
        PieceKind::Integer(128)
    } else if l.size <= 8 {
        PieceKind::Integer(64)
    } else {
        PieceKind::Integers64(2)
    };
    Ok(Boundary::Coerce(vec![Piece { offset: 0, kind }]))
}

fn classify_aapcs64(signature: &Signature, types: &[IrType]) -> Result<CSignature, BackendError> {
    let result = signature
        .results
        .first()
        .map(|t| aapcs64_boundary(types, *t))
        .transpose()?;
    let arguments = signature
        .params
        .iter()
        .map(|t| aapcs64_boundary(types, *t))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CSignature { arguments, result })
}
