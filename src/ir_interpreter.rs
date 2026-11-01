use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::{bail, ensure};

use crate::{builtins::BuiltinFunction, ir::*, plan::PlanBuilder};

const STEP_LIMIT: usize = 100_000;
const CALL_LIMIT: usize = 64;

/// Builder effects are committed only after the complete interpretation succeeds.
pub fn interpret_ir(ir: Ir, builder: &mut PlanBuilder) -> anyhow::Result<()> {
    verify_ir(&ir)?;
    let entry = FunctionId(
        ir.functions
            .iter()
            .position(|f| f.identifier == "build")
            .ok_or_else(|| anyhow::anyhow!("couldn't find build function"))?,
    );
    preflight(&ir, entry, &mut HashSet::new())?;
    let mut machine = Machine {
        ir: &ir,
        steps: STEP_LIMIT,
        effects: Vec::new(),
    };
    machine.call(entry, Vec::new(), 0)?;
    for (builtin, string) in machine.effects {
        match builtin {
            BuiltinFunction::AddSource => builder.modules.push(PathBuf::from(string)),
            BuiltinFunction::AddCSource => builder.native.sources.push(PathBuf::from(string)),
            BuiltinFunction::AddLinkLib => builder.native.libraries.push(string),
            BuiltinFunction::AddLibraryPath => {
                builder.native.library_paths.push(PathBuf::from(string))
            }
            BuiltinFunction::SetEntry => builder.entry_name = Some(string),
            BuiltinFunction::SetOutput => builder.output = Some(PathBuf::from(string)),
            _ => unreachable!("preflight checked builtin"),
        }
    }
    Ok(())
}

// Inspect every block in the transitive direct-call graph, including untaken arms.
fn preflight(ir: &Ir, function: FunctionId, seen: &mut HashSet<FunctionId>) -> anyhow::Result<()> {
    if !seen.insert(function) {
        return Ok(());
    }
    if let Some(builtin) = ir.builtin_functions.get(&function) {
        ensure!(
            !builtin.is_intrinsic(),
            "runtime intrinsic cannot be interpreted"
        );
        return Ok(());
    }
    let FunctionBody::Definition { blocks, .. } = &ir.functions[function.0].body else {
        bail!("call to unsupported declaration {function:?}");
    };
    for block in blocks {
        for instruction in &ir.blocks[block.0].instructions {
            match &instruction.kind {
                InstructionKind::Call { callee, .. } => preflight(ir, *callee, seen)?,
                InstructionKind::Constant(_)
                | InstructionKind::Binary { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Cast {
                    kind: CastKind::Numeric,
                    ..
                }
                | InstructionKind::SliceLength(_) => {}
                InstructionKind::GlobalAddress(_) => {
                    bail!("globals are not implemented in the build-file interpreter")
                }
                InstructionKind::Syscall { .. } => {
                    bail!("syscalls cannot be interpreted in build files")
                }
                _ => bail!(
                    "unsupported instruction in build-file interpreter: {:?}",
                    instruction.kind
                ),
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Bool(bool),
    Integer(u128),
    Bytes(Vec<u8>),
}

struct Machine<'a> {
    ir: &'a Ir,
    steps: usize,
    effects: Vec<(BuiltinFunction, String)>,
}

impl Machine<'_> {
    fn tick(&mut self) -> anyhow::Result<()> {
        ensure!(self.steps > 0, "build-file interpreter step limit exceeded");
        self.steps -= 1;
        Ok(())
    }

    fn constant(&self, id: ConstantId) -> anyhow::Result<Value> {
        Ok(match &self.ir.constants[id.0].kind {
            ConstantKind::Bool(v) => Value::Bool(*v),
            ConstantKind::Integer { bits } => Value::Integer(*bits),
            ConstantKind::Bytes(v) => Value::Bytes(v.clone()),
            kind => bail!("unsupported constant in build-file interpreter: {kind:?}"),
        })
    }

    fn operand(&self, operand: Operand, values: &HashMap<ValueId, Value>) -> anyhow::Result<Value> {
        match operand {
            Operand::Constant(id) => self.constant(id),
            Operand::Value(id) => values
                .get(&id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing interpreted value {id:?}")),
        }
    }

    fn operands(
        &self,
        operands: &[Operand],
        values: &HashMap<ValueId, Value>,
    ) -> anyhow::Result<Vec<Value>> {
        operands.iter().map(|o| self.operand(*o, values)).collect()
    }

    fn integer_type(&self, operand: Operand) -> anyhow::Result<(u16, bool)> {
        match self.ir.types[self.ir.operand_type(operand).unwrap().0] {
            IrType::Integer {
                width: IntegerWidth::Bits(width),
                signed,
            } => Ok((width, signed)),
            _ => bail!("unsupported numeric type in build-file interpreter"),
        }
    }

    fn call(
        &mut self,
        function: FunctionId,
        args: Vec<Value>,
        depth: usize,
    ) -> anyhow::Result<Vec<Value>> {
        ensure!(
            depth < CALL_LIMIT,
            "build-file interpreter call depth limit exceeded"
        );
        self.tick()?;
        if let Some(builtin) = self.ir.builtin_functions.get(&function) {
            let [Value::Bytes(bytes)] = args.as_slice() else {
                bail!("build builtin argument must be bytes");
            };
            self.effects
                .push((*builtin, std::str::from_utf8(bytes)?.to_owned()));
            return Ok(Vec::new());
        }
        let FunctionBody::Definition { entry, .. } = self.ir.functions[function.0].body else {
            bail!("expected function body");
        };
        let mut block_id = entry;
        let mut incoming = args;
        let mut values = HashMap::new();
        loop {
            self.tick()?;
            let block = &self.ir.blocks[block_id.0];
            ensure!(
                incoming.len() == block.params.len(),
                "incorrect interpreted argument count"
            );
            for (id, value) in block.params.iter().zip(incoming) {
                values.insert(*id, value);
            }
            for instruction in &block.instructions {
                self.tick()?;
                let results = match &instruction.kind {
                    InstructionKind::Call { callee, args } => {
                        self.call(*callee, self.operands(args, &values)?, depth + 1)?
                    }
                    InstructionKind::Constant(id) => vec![self.constant(*id)?],
                    InstructionKind::Binary { op, lhs, rhs } => {
                        vec![self.binary(*op, *lhs, *rhs, &values)?]
                    }
                    InstructionKind::Unary { op, operand } => {
                        let value = match self.operand(*operand, &values)? {
                            Value::Bool(v) => Value::Bool(!v),
                            Value::Integer(v) => {
                                let (width, _) = self.integer_type(*operand)?;
                                Value::Integer(
                                    match op {
                                        UnaryOp::Neg => v.wrapping_neg(),
                                        UnaryOp::Not => !v,
                                    } & mask(width),
                                )
                            }
                            _ => bail!("unsupported unary value"),
                        };
                        vec![value]
                    }
                    InstructionKind::Cast { operand, .. } => {
                        let source = self.operand(*operand, &values)?;
                        let ty = &self.ir.types[self.ir.values[instruction.results[0].0].ty.0];
                        let bits = match source {
                            Value::Bool(v) => u128::from(v),
                            Value::Integer(v) => {
                                let (width, signed) = self.integer_type(*operand)?;
                                if signed {
                                    signed_value(v, width) as u128
                                } else {
                                    v
                                }
                            }
                            _ => bail!("unsupported numeric cast"),
                        };
                        vec![match ty {
                            IrType::Bool => Value::Bool(bits != 0),
                            IrType::Integer {
                                width: IntegerWidth::Bits(width),
                                ..
                            } => Value::Integer(bits & mask(*width)),
                            _ => bail!("unsupported numeric cast"),
                        }]
                    }
                    InstructionKind::SliceLength(operand) => {
                        let Value::Bytes(bytes) = self.operand(*operand, &values)? else {
                            bail!("unsupported slice value");
                        };
                        vec![Value::Integer(bytes.len() as u128)]
                    }
                    _ => bail!("unsupported instruction in build-file interpreter"),
                };
                ensure!(
                    results.len() == instruction.results.len(),
                    "incorrect interpreted result count"
                );
                values.extend(instruction.results.iter().copied().zip(results));
            }
            let edge = match &block.terminator {
                Terminator::Return { values: operands } => return self.operands(operands, &values),
                Terminator::Jump(edge) => edge,
                Terminator::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } => {
                    let Value::Bool(condition) = self.operand(*condition, &values)? else {
                        bail!("expected boolean branch");
                    };
                    if condition { then_edge } else { else_edge }
                }
                Terminator::Switch {
                    subject,
                    cases,
                    default,
                } => {
                    let subject = self.operand(*subject, &values)?;
                    let mut edge = default;
                    for case in cases {
                        if self.constant(case.value)? == subject {
                            edge = &case.edge;
                            break;
                        }
                    }
                    edge
                }
                Terminator::Trap(reason) => bail!("build-file interpreter trap: {reason:?}"),
                Terminator::Unreachable => bail!("build-file interpreter reached unreachable"),
            };
            // Evaluate all edge operands before overwriting any loop parameters.
            incoming = self.operands(&edge.args, &values)?;
            block_id = edge.target;
        }
    }

    fn binary(
        &self,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
        values: &HashMap<ValueId, Value>,
    ) -> anyhow::Result<Value> {
        use BinaryOp::*;
        let left = self.operand(lhs, values)?;
        let right = self.operand(rhs, values)?;
        if let (Value::Bool(a), Value::Bool(b)) = (&left, &right) {
            return Ok(Value::Bool(match op {
                Eq => a == b,
                Ne => a != b,
                And => *a & *b,
                Or => *a | *b,
                Xor => *a ^ *b,
                _ => bail!("unsupported boolean operation"),
            }));
        }
        let (Value::Integer(a), Value::Integer(b)) = (left, right) else {
            bail!("unsupported binary values");
        };
        let (width, signed) = self.integer_type(lhs)?;
        let sa = signed_value(a, width);
        let sb = signed_value(b, width);
        let order = if signed { sa.cmp(&sb) } else { a.cmp(&b) };
        let comparison = match op {
            Eq => Some(a == b),
            Ne => Some(a != b),
            Lt => Some(order.is_lt()),
            Le => Some(!order.is_gt()),
            Gt => Some(order.is_gt()),
            Ge => Some(!order.is_lt()),
            _ => None,
        };
        if let Some(value) = comparison {
            return Ok(Value::Bool(value));
        }
        if matches!(op, Div | Rem) {
            ensure!(b != 0, "build-file interpreter division by zero");
        }
        if matches!(op, Shl | Shr) {
            ensure!(
                b < u128::from(width),
                "build-file interpreter invalid shift"
            );
        }
        Ok(Value::Integer(
            match op {
                Add => a.wrapping_add(b),
                Sub => a.wrapping_sub(b),
                Mul => a.wrapping_mul(b),
                Div if signed => sa.wrapping_div(sb) as u128,
                Rem if signed => sa.wrapping_rem(sb) as u128,
                Div => a / b,
                Rem => a % b,
                Shl => a << b,
                Shr if signed => (sa >> b) as u128,
                Shr => a >> b,
                And => a & b,
                Or => a | b,
                Xor => a ^ b,
                _ => unreachable!(),
            } & mask(width),
        ))
    }
}

fn mask(width: u16) -> u128 {
    u128::MAX >> (128 - width)
}
fn signed_value(bits: u128, width: u16) -> i128 {
    ((bits << (128 - width)) as i128) >> (128 - width)
}
