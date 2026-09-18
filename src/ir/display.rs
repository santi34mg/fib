use std::fmt;

use super::{BinOp, Instruction, IrFunction, IrProgram, Label, Operand, SymbolId, Temp, UnOp};
// ---------------------------------------------------------------------------
// Pretty printing (used by tests and `--emit=ir` later)
// ---------------------------------------------------------------------------

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%t{}", self.0)
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "L{}", self.0)
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%s{}", self.0)
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temp(t) => write!(f, "{}", t),
            Self::Symbol(s) => write!(f, "{}", s),
            Self::ConstInt { value, ty } => write!(f, "{}:{}", value, ty),
            Self::ConstFloat { value, ty } => write!(f, "{}:{}", value, ty),
            Self::ConstBool(v) => write!(f, "{}", v),
            Self::StringLit(s) => write!(f, "\"{}\"", s),
            Self::Null => write!(f, "null"),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Alloca { sym, name, ty } => write!(f, "{} = alloca {} : {}", sym, name, ty),
            Self::Store { dst, src } => write!(f, "store {} <- {}", dst, src),
            Self::Load { dst, src } => write!(f, "{} = load {}", dst, src),
            Self::Copy { dst, src } => write!(f, "{} = copy {}", dst, src),
            Self::Binary {
                dst,
                op,
                lhs,
                rhs,
                ty,
            } => {
                write!(f, "{} = {} {}, {} : {}", dst, op, lhs, rhs, ty)
            }
            Self::Unary { dst, op, src, ty } => {
                let s = match op {
                    UnOp::Neg => "neg",
                    UnOp::Not => "not",
                };
                write!(f, "{} = {} {} : {}", dst, s, src, ty)
            }
            Self::AddrOf { dst, src } => write!(f, "{} = addr_of {}", dst, src),
            Self::LoadPtr { dst, ptr, ty } => write!(f, "{} = load_ptr {} : {}", dst, ptr, ty),
            Self::StorePtr { ptr, src } => write!(f, "store_ptr {} <- {}", ptr, src),
            Self::Cast {
                dst, src, target, ..
            } => write!(f, "{} = cast {} to {}", dst, src, target),
            Self::Call {
                dst, callee, args, ..
            } => {
                let arg_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                match dst {
                    Some(d) => write!(f, "{} = call {}({})", d, callee, arg_str),
                    None => write!(f, "call {}({})", callee, arg_str),
                }
            }
            Self::BuiltinCall {
                dst, builtin, args, ..
            } => {
                let arg_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                match dst {
                    Some(d) => write!(f, "{} = builtin @{}({})", d, builtin.name(), arg_str),
                    None => write!(f, "builtin @{}({})", builtin.name(), arg_str),
                }
            }
            Self::StructConstruct {
                dst,
                type_name,
                fields,
                ..
            } => {
                let fs = fields
                    .iter()
                    .map(|(n, o)| format!("{}: {}", n, o))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = struct {} {{ {} }}", dst, type_name, fs)
            }
            Self::FieldLoad {
                dst,
                base,
                field_index,
                ..
            } => {
                write!(f, "{} = field_load {}.{} ", dst, base, field_index)
            }
            Self::FieldStore {
                base,
                field_index,
                src,
            } => {
                write!(f, "field_store {}.{} <- {}", base, field_index, src)
            }
            Self::ArrayLiteral { dst, elems, .. } => {
                let es = elems
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = array [{}]", dst, es)
            }
            Self::IndexLoad {
                dst, base, index, ..
            } => {
                write!(f, "{} = index_load {}[{}]", dst, base, index)
            }
            Self::IndexStore { base, index, src } => {
                write!(f, "index_store {}[{}] <- {}", base, index, src)
            }
            Self::EnumLiteral {
                dst, discriminant, ..
            } => {
                write!(f, "{} = enum_tag {}", dst, discriminant)
            }
            Self::EnumConstruct {
                dst,
                discriminant,
                payload,
                ..
            } => {
                let ps = payload
                    .iter()
                    .map(|(n, o)| format!("{}: {}", n, o))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = enum_construct {} {{ {} }}", dst, discriminant, ps)
            }
            Self::SwitchDispatch {
                subject,
                cases,
                default,
            } => {
                let cs = cases
                    .iter()
                    .map(|(d, l)| format!("{} -> {}", d, l))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "switch {} [{}] default {}", subject, cs, default)
            }
            Self::IfGoto { cond, target } => write!(f, "if {} goto {}", cond, target),
            Self::Goto { target } => write!(f, "goto {}", target),
            Self::LabelDef(l) => write!(f, "{}:", l),
            Self::Return { values } => {
                let vs = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "return {}", vs)
            }
            Self::Unreachable => write!(f, "unreachable"),
        }
    }
}

impl fmt::Display for IrFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = self
            .params
            .iter()
            .map(|(s, n, t)| format!("{} {}: {}", s, n, t))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(f, "fn {}({}) -> {} {{", self.name, params, self.return_ty)?;
        for (s, n, t) in &self.symbols {
            writeln!(f, "  ; sym {} {} : {}", s, n, t)?;
        }
        for block in &self.blocks {
            writeln!(f, "  {}:", block.label)?;
            for instr in &block.instrs {
                // Skip duplicate label defs (already printed as block header)
                if let Instruction::LabelDef(l) = instr
                    && *l == block.label
                {
                    continue;
                }
                writeln!(f, "    {}", instr)?;
            }
        }
        writeln!(f, "}}")
    }
}

impl fmt::Display for IrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for func in &self.functions {
            writeln!(f, "{}", func)?;
        }
        Ok(())
    }
}
