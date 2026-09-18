use super::{Instruction, LowerError, Operand, UnOp};
use crate::frontend::typed_ast::{LogicalOp, Ty, TypedExpr, TypedExprKind};

use super::builder::FunctionBuilder;

// ---------------------------------------------------------------------------
// Expressions (each returns the Operand holding its value)
// ---------------------------------------------------------------------------

pub(super) fn lower_expr(b: &mut FunctionBuilder, expr: &TypedExpr) -> Result<Operand, LowerError> {
    match &expr.expression {
        TypedExprKind::LiteralInt { value } => Ok(Operand::ConstInt {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExprKind::LiteralFloat { value } => Ok(Operand::ConstFloat {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExprKind::LiteralBool(v) => Ok(Operand::ConstBool(*v)),
        TypedExprKind::LiteralString { value } => Ok(Operand::StringLit(value.clone())),
        TypedExprKind::Null => Ok(Operand::Null),
        TypedExprKind::Identifier(name) => {
            let sym = b
                .lookup(&name.value)
                .ok_or_else(|| LowerError::Unsupported(format!("unknown variable '{}'", name)))?;
            let dst = b.fresh_temp();
            b.emit(Instruction::Load { dst, src: sym });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::Binary {
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
        TypedExprKind::ShortCircuit {
            left,
            operator,
            right,
        } => lower_short_circuit(b, expr, left, *operator, right),
        TypedExprKind::Call { callee, args } => {
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
                callee: callee.value.clone(),
                args: operands,
                ret_ty: expr.inferred_type.clone(),
                is_variadic: false,
            });
            Ok(ret.unwrap_or(Operand::ConstBool(false)))
        }
        TypedExprKind::BuiltinCall { builtin, args } => {
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
        TypedExprKind::Cast {
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
        TypedExprKind::AddressOf(inner) => {
            // Only `&x` for a named place is modeled in the core slice.
            if let TypedExprKind::Identifier(name) = &inner.expression {
                let sym = b.lookup(&name.value).ok_or_else(|| {
                    LowerError::Unsupported(format!("address-of unknown '{}'", name))
                })?;
                let dst = b.fresh_temp();
                b.emit(Instruction::AddrOf { dst, src: sym });
                Ok(Operand::Temp(dst))
            } else {
                Err(LowerError::Unsupported(
                    "address-of non-identifier (phase 2b: general lvalues)".to_string(),
                ))
            }
        }
        TypedExprKind::Deref(inner) => {
            let ptr = lower_expr(b, inner)?;
            let dst = b.fresh_temp();
            b.emit(Instruction::LoadPtr {
                dst,
                ptr,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::FieldAccess { .. } => Err(LowerError::Unsupported(
            "field access (phase 2b: FieldLoad)".to_string(),
        )),
        TypedExprKind::StructConstruct { .. } => Err(LowerError::Unsupported(
            "struct construction (phase 2b: StructConstruct)".to_string(),
        )),
        TypedExprKind::IndexAccess { .. } => Err(LowerError::Unsupported(
            "index access (phase 2b: IndexLoad)".to_string(),
        )),
        TypedExprKind::ArrayLiteral { .. } => Err(LowerError::Unsupported(
            "array literal (phase 2b: ArrayLiteral)".to_string(),
        )),
        TypedExprKind::QualifiedAccess { module, name } => Err(LowerError::Unsupported(format!(
            "qualified access {}::{} (phase 2b: module mangling)",
            module, name
        ))),
        TypedExprKind::ComptimeType(_) => Err(LowerError::Unsupported(
            "comptime type value in runtime code".to_string(),
        )),
        TypedExprKind::EnumLiteral { discriminant, .. } => {
            let dst = b.fresh_temp();
            b.emit(Instruction::EnumLiteral {
                dst,
                discriminant: *discriminant,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::EnumVariantConstruct {
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

pub(super) fn lower_short_circuit(
    b: &mut FunctionBuilder,
    _expr: &TypedExpr,
    left: &TypedExpr,
    op: LogicalOp,
    right: &TypedExpr,
) -> Result<Operand, LowerError> {
    use crate::frontend::tokens::builtin::BuiltinType;
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
