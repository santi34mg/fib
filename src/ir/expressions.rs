use anyhow::{Context, bail};

use super::{Instruction, Operand, UnOp};
use crate::frontend::typed_ast::{LogicalOp, TypedExpr, TypedExpressionKind, ty::Ty};
use crate::frontend::tokens::builtin::BuiltinType;

use super::builder::BasicBlockBuilder;

// ---------------------------------------------------------------------------
// Expressions (each returns the Operand holding its value)
// ---------------------------------------------------------------------------

pub(super) fn lower_expr<'t>(b: &mut BasicBlockBuilder, expr: &TypedExpr) -> anyhow::Result<Operand<'t>> {
    match &expr.expression {
        TypedExpressionKind::LiteralInt { value } => Ok(Operand::ConstInt {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExpressionKind::LiteralFloat { value } => Ok(Operand::ConstFloat {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExpressionKind::LiteralBool(v) => Ok(Operand::ConstBool(*v)),
        TypedExpressionKind::LiteralString { value } => Ok(Operand::StringLit(value.clone())),
        TypedExpressionKind::Null => Ok(Operand::Null),
        TypedExpressionKind::Identifier(name) => {
            let sym = b
                .lookup(&name.raw_str).context(format!("unknown variable '{}'", name))?;
            let dst = b.fresh_temp();
            b.emit(Instruction::Load { dst, src: sym });
            Ok(Operand::Temp(dst))
        }
        TypedExpressionKind::Binary {
            left,
            operator,
            right,
        } => {
            // `operator` is already a semantic `BinOp` (mapped once in
            // analysis), shared with `Instruction::Binary` — no conversion.
            let l = lower_expr(b, left)?;
            let r = lower_expr(b, right)?;
            let dst = b.fresh_temp();
            // `ty` is the *operand* type (left's) for opcode selection
            // (`SDiv` vs `UDiv`, `SLT` vs `ULT`, ...). For comparisons the
            // result is `@bool`, but the opcode still depends on the
            // operands —mirroring `codegen_expr` which keys off
            // `left.inferred_type`.
            b.emit(Instruction::Binary {
                dst,
                op: *operator,
                lhs: l,
                rhs: r,
                ty: left.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        // Short-circuit && / || via explicit control flow + join temp.
        TypedExpressionKind::ShortCircuit {
            left,
            operator,
            right,
        } => lower_short_circuit(b, expr, left, *operator, right),
        TypedExpressionKind::Call { callee, args } => {
            let mut operands = Vec::with_capacity(args.len());
            for a in args {
                operands.push(lower_expr(b, a)?);
            }
            // Void calls get no destination temp.
            use crate::frontend::tokens::builtin::BuiltinType;
            let is_void = matches!(expr.inferred_type, Ty::Builtin(BuiltinType::Void));
            let dst = if is_void { None } else { Some(b.fresh_temp()) };
            let ret = dst.map(Operand::Temp);
            b.emit(Instruction::Call {
                dst,
                callee: callee.raw_str.clone(),
                args: operands,
                ret_ty: expr.inferred_type.clone(),
                is_variadic: false,
            });
            Ok(ret.unwrap_or(Operand::ConstBool(false)))
        }
        TypedExpressionKind::BuiltinCall { builtin, args } => {
            let mut operands = Vec::with_capacity(args.len());
            for a in args {
                operands.push(lower_expr(b, a)?);
            }
            let dst = b.fresh_temp();
            let ret = Operand::Temp(dst);
            b.emit(Instruction::BuiltinCall {
                dst: Some(dst),
                builtin: builtin.clone(),
                args: operands,
                ret_ty: expr.inferred_type.clone(),
            });
            Ok(ret)
        }
        TypedExpressionKind::Cast {
            expr: inner,
            target_type,
        } => {
            let src = lower_expr(b, inner)?;
            let dst = b.fresh_temp();
            b.emit(Instruction::Cast {
                dst,
                src,
                from: inner.inferred_type.clone(),
                target: target_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExpressionKind::AddressOf(inner) => {
            // Only `&x` for a named place is modeled in the core slice.
            if let TypedExpressionKind::Identifier(name) = &inner.expression {
                let sym = b.lookup(&name.raw_str).context(format!("address-of unknown '{}'", name))?;
                let dst = b.fresh_temp();
                b.emit(Instruction::AddrOf { dst, src: sym });
                Ok(Operand::Temp(dst))
            } else {
                bail!("address-of non-identifier (phase 2b: general lvalues)")
            }
        }
        TypedExpressionKind::Deref(inner) => {
            let ptr = lower_expr(b, inner)?;
            let dst = b.fresh_temp();
            b.emit(Instruction::LoadPtr {
                dst,
                ptr,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExpressionKind::FieldAccess { .. } => bail!("field access (phase 2b: FieldLoad)"),
        TypedExpressionKind::ArrayLen { .. } => bail!("array/slice '@len' property (phase 2b: comptime const / slice len)"),
        TypedExpressionKind::ArrayToSlice { .. } => bail!("explicit array-to-slice (phase 2b: slice struct)"),
        TypedExpressionKind::Slice { .. } => bail!("slice range (phase 2b: slice struct)"),
        TypedExpressionKind::StructConstruct { .. } => bail!("struct construction (phase 2b: StructConstruct)"),
        TypedExpressionKind::IndexAccess { .. } => bail!("index access (phase 2b: IndexLoad)"),
        TypedExpressionKind::ArrayLiteral { .. } => bail!("array literal (phase 2b: ArrayLiteral)"),
        TypedExpressionKind::QualifiedAccess { module, name } => {
            bail!(
                "qualified access {}::{} (phase 2b: module mangling)",
                module, name
            )
        }
        TypedExpressionKind::ComptimeType(_) => bail!("comptime type value in runtime code"),
        TypedExpressionKind::EnumLiteral { discriminant, .. } => {
            let dst = b.fresh_temp();
            b.emit(Instruction::EnumLiteral {
                dst,
                discriminant: *discriminant,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExpressionKind::EnumVariantConstruct {
            discriminant,
            fields,
            ..
        } => {
            let mut payload = Vec::with_capacity(fields.len());
            for (fname, fexpr) in fields {
                payload.push((fname.clone(), lower_expr(b, fexpr)?));
            }
            let dst = b.fresh_temp();
            b.emit(Instruction::EnumConstruct {
                dst,
                discriminant: *discriminant,
                payload,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
    }
}

pub(super) fn lower_short_circuit<'t>(
    b: &mut BasicBlockBuilder,
    _expr: &TypedExpr,
    left: &TypedExpr,
    op: LogicalOp,
    right: &TypedExpr,
) -> anyhow::Result<Operand<'t>> {
    let bool_ty = Ty::Builtin(BuiltinType::Boolean);
    let l = lower_expr(b, left)?;
    let result = b.fresh_temp();
    let rhs_label = b.fresh_label();
    let merge_label = b.fresh_label();
    let is_and = matches!(op, LogicalOp::And);
    // Seed the join temp with the short-circuit value, then overwrite from RHS.
    let short_val = Operand::ConstBool(!is_and);
    b.emit(Instruction::Copy {
        dst: result,
        src: short_val,
    });
    // if (is_and ? !l : l) goto merge  <=>  skip RHS when decided
    if is_and {
        let ntmp = b.fresh_temp();
        b.emit(Instruction::Unary {
            dst: ntmp,
            op: UnOp::Not,
            src: l,
            ty: bool_ty.clone(),
        });
        b.emit(Instruction::IfGoto {
            cond: Operand::Temp(ntmp),
            target: merge_label,
        });
    } else {
        b.emit(Instruction::IfGoto {
            cond: l,
            target: merge_label,
        });
    }
    b.emit(Instruction::Goto { target: rhs_label });
    b.emit(Instruction::LabelDef(rhs_label));
    let r = lower_expr(b, right)?;
    b.emit(Instruction::Copy {
        dst: result,
        src: r,
    });
    b.emit(Instruction::LabelDef(merge_label));
    Ok(Operand::Temp(result))
}
