//! Validated analyzed-source to owned SSA lowering. Pipeline orchestration and
//! physical ABI placement belong to the driver and backend respectively.
use crate::{
    diagnostics::{CompilerError, ErrorKind, Span},
    frontend::{
        analysis_annotation::BindingId,
        analyzer::{AnalyzedProject, evaluate_numeric, known_boolean, validate_analysis},
        ast::{
            expression::{Expression, ExpressionKind as E},
            statement::{Statement, StatementKind as S},
        },
        identifier::{DeclId, Entity, LocalId, ResolvedName},
        semantic_type::{SemanticType, TypeId as SemanticId},
        tokens::{Literal, Operator, builtin::BuiltinType},
    },
    ir::*,
    plan::FileId,
};
use std::collections::{BTreeMap, HashMap};
const SCALARS: [BuiltinType; 16] = [
    BuiltinType::Int32,
    BuiltinType::Boolean,
    BuiltinType::Int8,
    BuiltinType::Int16,
    BuiltinType::Int64,
    BuiltinType::Int128,
    BuiltinType::UInt8,
    BuiltinType::UInt16,
    BuiltinType::UInt32,
    BuiltinType::UInt64,
    BuiltinType::UInt128,
    BuiltinType::Float16,
    BuiltinType::Float32,
    BuiltinType::Float64,
    BuiltinType::Float128,
    BuiltinType::String,
];
fn scalar_id(b: &BuiltinType) -> TypeId {
    TypeId(
        SCALARS
            .iter()
            .position(|ty| ty == b)
            .expect("registered scalar"),
    )
}
fn scalar_type(b: &BuiltinType) -> IrType {
    use crate::numeric::NumericType;
    match b {
        BuiltinType::Boolean => IrType::Bool,
        BuiltinType::String => IrType::Bytes,
        _ => match NumericType::builtin(b).expect("registered scalar") {
            NumericType::Integer { width, signed } => IrType::Integer {
                width: IntegerWidth::Bits(width),
                signed,
            },
            NumericType::Float(width) => IrType::Float(match width {
                16 => FloatWidth::F16,
                32 => FloatWidth::F32,
                64 => FloatWidth::F64,
                128 => FloatWidth::F128,
                _ => unreachable!(),
            }),
        },
    }
}

struct Source<'a> {
    project: &'a AnalyzedProject,
    file: FileId,
}
impl Source<'_> {
    fn error(&self, span: Span, message: impl Into<String>) -> anyhow::Error {
        let mut error = CompilerError::new(ErrorKind::Backend, message.into()).with_span(span);
        error.filename = self.project.source_paths.get(&self.file).cloned();
        if error.filename.is_none() {
            error.hint = Some(format!("source file {:?}", self.file));
        }
        error.into()
    }
    fn ir_type(&self, ty: SemanticId, span: Span) -> anyhow::Result<Option<TypeId>> {
        match self.project.types.get(self.project.types.canonical(ty)?)? {
            SemanticType::Builtin(BuiltinType::Void) => Ok(None),
            SemanticType::Builtin(b) if SCALARS.contains(b) => Ok(Some(scalar_id(b))),
            _ => Err(self.error(
                span,
                "IR lowering supports only fixed-width numeric, bool, string, and void types",
            )),
        }
    }
    fn value_type(&self, ty: SemanticId, span: Span) -> anyhow::Result<TypeId> {
        self.ir_type(ty, span)?
            .ok_or_else(|| self.error(span, "void is not an IR value type"))
    }
    fn expression_type(&self, expr: &Expression) -> anyhow::Result<TypeId> {
        let info = expr
            .annotation
            .resolved
            .ok_or_else(|| self.error(expr.span, "missing expression annotation"))?;
        self.value_type(info.ty, expr.span)
    }
    fn signature(&self, declaration: DeclId, span: Span) -> anyhow::Result<Signature> {
        let semantic = self
            .project
            .types
            .declarations
            .get(&declaration)
            .copied()
            .ok_or_else(|| self.error(span, "missing function semantic type"))?;
        let SemanticType::Function(function) = self
            .project
            .types
            .get(self.project.types.canonical(semantic)?)?
        else {
            return Err(self.error(span, "function declaration has a non-function type"));
        };
        let params = function
            .parameters
            .iter()
            .map(|ty| self.value_type(*ty, span))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let results = self
            .ir_type(function.return_type, span)?
            .into_iter()
            .collect();
        Ok(Signature {
            params,
            results,
            calling_convention: source_convention(),
        })
    }
}

/// Lowers the validated, analyzed source subset into owned SSA IR.
///
/// This entry point requires exactly one source unit. It first registers builtin
/// and user function identities so calls can refer to later declarations, then
/// checks supported constructs and gathers constants before building function
/// bodies. The resulting IR is verified before it is returned.
///
/// Unsupported declarations, expressions, or statements produce a diagnostic
/// associated with their source span. Functions that can reach their end must
/// have no return values; value-returning functions must return explicitly.
pub fn lower(project: &AnalyzedProject) -> anyhow::Result<Ir> {
    validate_analysis(project)?;
    anyhow::ensure!(
        project.units.len() == 1,
        "IR lowering currently requires one source file"
    );
    let mut ir = Ir::new();
    ir.types = SCALARS[..scalar_id(&BuiltinType::String).0]
        .iter()
        .map(scalar_type)
        .collect();
    // NOTE: This conditional Bytes-type registration probably needs to be approached differently in the future.
    if !project.builtins.is_empty()
        || project
            .types
            .types
            .iter()
            .any(|ty| matches!(ty, SemanticType::Builtin(BuiltinType::String)))
    {
        ir.types.push(IrType::Bytes);
    }
    let mut functions = HashMap::new();
    let mut declarations = Vec::new();

    // Sort builtins to assign stable function IDs regardless of map iteration order.
    let mut builtins = project.builtins.iter().collect::<Vec<_>>();
    builtins.sort_by_key(|(declaration, _)| (declaration.file_id, declaration.index));
    for (&declaration, &builtin) in builtins {
        let source = Source {
            project,
            file: declaration.file_id,
        };
        let function_id = FunctionId(ir.functions.len());
        functions.insert(declaration, function_id);
        ir.functions.push(Function {
            identifier: format!("fib.build.v1.{}", builtin.name()),
            signature: Signature {
                calling_convention: CallingConvention::Fib,
                ..source.signature(declaration, Span::new(1, 1))?
            },
            linkage: Linkage::External,
            body: FunctionBody::Declaration,
        });
        ir.builtin_functions.insert(function_id, builtin);
    }

    let entry_name = if project.builtins.is_empty() {
        "main"
    } else {
        "build"
    };
    for unit in &project.units {
        let source = Source {
            project,
            file: unit.source_file_id,
        };
        for node in &unit.ast.statements {
            let S::FunctionDeclaration(function) = &node.kind else {
                return Err(source.error(
                    node.span,
                    "only function definitions are supported at file scope",
                ));
            };
            let signature = &function.signature;
            if function.is_extern
                || function.body.is_none()
                || !signature.generic_parameters.is_empty()
            {
                return Err(
                    source.error(node.span, "extern and generic functions are not supported")
                );
            }
            let Some(BindingId::Declaration(declaration)) = signature.annotation.binding else {
                return Err(source.error(node.span, "missing function identity"));
            };
            let function_id = FunctionId(ir.functions.len());
            if functions.insert(declaration, function_id).is_some() {
                return Err(source.error(node.span, "duplicate lowered function identity"));
            }
            let entry = signature.identifier == entry_name;
            let mut lowered_signature = source.signature(declaration, node.span)?;
            if !entry {
                lowered_signature.calling_convention = CallingConvention::Fib;
            }
            ir.functions.push(Function {
                identifier: signature.identifier.clone(),
                signature: lowered_signature,
                linkage: if entry {
                    Linkage::External
                } else {
                    Linkage::Internal
                },
                body: FunctionBody::Declaration,
            });
            declarations.push((function_id, function, node.span, unit.source_file_id));
        }
    }
    // Validate the entire subset, including dead branches, and collect constants
    // before the builder borrows IR. Forward function identities already exist.
    for (_, function, _, file) in &declarations {
        let source = Source {
            project,
            file: *file,
        };
        Checker {
            source: &source,
            functions: &functions,
            constants: &mut ir.constants,
        }
        .statements(
            &function
                .body
                .as_ref()
                .expect("checked definition")
                .statements,
        )?;
    }
    let constants = ir.constants.clone();
    for (id, function, span, file) in declarations {
        let source = Source { project, file };
        let parameter_types = ir.functions[id.0].signature.params.clone();
        let result_count = ir.functions[id.0].signature.results.len();
        let mut builder = FunctionBuilder::new(&mut ir, id)?;
        let entry = builder.create_block(&parameter_types)?;
        let mut locals = BTreeMap::new();
        for (parameter, &value) in function
            .signature
            .parameters
            .iter()
            .zip(builder.block_params(entry)?)
        {
            let Some(BindingId::Local(local)) = parameter.annotation.binding else {
                return Err(source.error(span, "missing parameter identity"));
            };
            locals.insert(local, Operand::Value(value));
        }
        let mut lowerer = Lowerer {
            source: &source,
            functions: &functions,
            constants: &constants,
            builder,
        };
        let remaining = lowerer.statements(
            &function
                .body
                .as_ref()
                .expect("checked definition")
                .statements,
            State {
                block: entry,
                locals,
            },
        )?;
        if let Some(state) = remaining {
            if result_count == 0 {
                lowerer
                    .builder
                    .set_terminator(state.block, Terminator::Return { values: vec![] })?;
            } else {
                return Err(source.error(span, "function reaches its end without a return"));
            }
        }
        lowerer.builder.finish(entry)?;
    }
    verify_ir(&ir)?;
    Ok(ir)
}

// Temporary supported source convention; changing policy must not introduce
// physical ABI details into semantic signatures or CFG construction.
fn source_convention() -> CallingConvention {
    CallingConvention::C
}

fn local(expr: &Expression) -> Option<LocalId> {
    if let E::QualifiedIdentifier(name) = &expr.kind
        && let Some(ResolvedName::Local(id)) = name.annotation.resolved
    {
        Some(id)
    } else {
        None
    }
}
fn callee(expr: &Expression, functions: &HashMap<DeclId, FunctionId>) -> Option<FunctionId> {
    if let E::QualifiedIdentifier(name) = &expr.kind
        && let Some(ResolvedName::Entity(Entity::Declaration(id))) = name.annotation.resolved
    {
        functions.get(&id).copied()
    } else {
        None
    }
}
fn binary_op(operator: Operator) -> Option<BinaryOp> {
    Some(match operator {
        Operator::Plus => BinaryOp::Add,
        Operator::Minus => BinaryOp::Sub,
        Operator::Star => BinaryOp::Mul,
        Operator::Slash => BinaryOp::Div,
        Operator::Percent => BinaryOp::Rem,
        Operator::LeftShift => BinaryOp::Shl,
        Operator::RightShift => BinaryOp::Shr,
        Operator::Ampersand => BinaryOp::And,
        Operator::Pipe => BinaryOp::Or,
        Operator::Caret => BinaryOp::Xor,
        Operator::DoubleEquals => BinaryOp::Eq,
        Operator::Different => BinaryOp::Ne,
        Operator::LesserThan => BinaryOp::Lt,
        Operator::LesserEqual => BinaryOp::Le,
        Operator::GreaterThan => BinaryOp::Gt,
        Operator::GreaterEqual => BinaryOp::Ge,
        _ => return None,
    })
}
fn literal(source: &Source<'_>, expr: &Expression) -> anyhow::Result<Option<Constant>> {
    if let Some(value) = evaluate_numeric(expr, &source.project.types)
        .map_err(|e| source.error(expr.span, e.to_string()))?
    {
        let kind = match value.ty {
            crate::numeric::NumericType::Integer { .. } => {
                ConstantKind::Integer { bits: value.bits }
            }
            crate::numeric::NumericType::Float(_) => ConstantKind::Float { bits: value.bits },
        };
        return Ok(Some(Constant {
            ty: source.expression_type(expr)?,
            kind,
        }));
    }
    Ok(match &expr.kind {
        E::Literal(Literal::Boolean(value)) => Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Bool(*value),
        }),
        E::Literal(Literal::String(value)) => Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Bytes(value.as_bytes().to_vec()),
        }),
        _ => None,
    })
}
struct Checker<'a, 'p> {
    source: &'a Source<'p>,
    functions: &'a HashMap<DeclId, FunctionId>,
    constants: &'a mut Vec<Constant>,
}
impl Checker<'_, '_> {
    fn expression(&mut self, expr: &Expression) -> anyhow::Result<()> {
        self.source.expression_type(expr)?;
        if let Some(constant) = literal(self.source, expr)? {
            if !self
                .constants
                .iter()
                .any(|c| c.ty == constant.ty && c.kind == constant.kind)
            {
                self.constants.push(constant);
            }
            return Ok(());
        }
        match &expr.kind {
            E::QualifiedIdentifier(_) if local(expr).is_some() => Ok(()),
            E::Call {
                callee: function,
                args,
            } if callee(function, self.functions).is_some() => {
                for arg in args {
                    self.expression(arg)?;
                }
                Ok(())
            }
            E::UnaryExpression(unary)
                if matches!(
                    unary.operator,
                    Operator::Minus | Operator::Tilde | Operator::LogicalNot
                ) =>
            {
                self.expression(&unary.expression)
            }
            E::Cast { expr, .. } => self.expression(expr),
            E::BinaryExpresison(binary)
                if binary_op(binary.operator).is_some()
                    || matches!(binary.operator, Operator::LogicalAnd | Operator::LogicalOr) =>
            {
                self.expression(&binary.left)?;
                self.expression(&binary.right)
            }
            E::BinaryExpresison(_) => Err(self
                .source
                .error(expr.span, "unsupported binary operator in IR lowering")),
            _ => Err(self
                .source
                .error(expr.span, "unsupported expression in IR lowering")),
        }
    }
    fn statements(&mut self, statements: &[Statement]) -> anyhow::Result<()> {
        for node in statements {
            match &node.kind {
                S::VariableDeclaration(variable) if !variable.is_comp => {
                    if let Some(ty) = &variable.ty {
                        self.source.value_type(
                            ty.annotation.resolved.ok_or_else(|| {
                                self.source.error(node.span, "missing local type")
                            })?,
                            node.span,
                        )?;
                    }
                    let expr = variable.expression.as_ref().ok_or_else(|| {
                        self.source.error(
                            node.span,
                            "uninitialized declarations are not supported by IR lowering",
                        )
                    })?;
                    self.expression(expr)?;
                }
                S::Assignment { left, right } if local(left).is_some() => {
                    self.source.expression_type(left)?;
                    self.expression(right)?;
                }
                S::ExpressionStatement(expression) => {
                    let E::Call {
                        callee: function,
                        args,
                    } = &expression.kind
                    else {
                        return Err(self.source.error(
                            expression.span,
                            "only calls may be used as expression statements",
                        ));
                    };
                    callee(function, self.functions).ok_or_else(|| {
                        self.source
                            .error(expression.span, "unsupported call target")
                    })?;
                    if self.source.project.types.get(
                        self.source.project.types.canonical(
                            expression
                                .annotation
                                .resolved
                                .ok_or_else(|| {
                                    self.source
                                        .error(expression.span, "missing expression annotation")
                                })?
                                .ty,
                        )?,
                    )? != &SemanticType::Builtin(BuiltinType::Void)
                    {
                        return Err(self
                            .source
                            .error(expression.span, "discarding call results is not supported"));
                    }
                    for arg in args {
                        self.expression(arg)?;
                    }
                }
                S::Return(None) => {}
                S::Return(Some(values)) if values.len() == 1 => self.expression(&values[0])?,
                S::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    self.expression(condition)?;
                    self.statements(then_branch)?;
                    if let Some(branch) = else_branch {
                        self.statements(branch)?;
                    }
                }
                _ => {
                    return Err(self
                        .source
                        .error(node.span, "unsupported statement in IR lowering"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct State {
    block: BlockId,
    locals: BTreeMap<LocalId, Operand>,
}
struct Lowerer<'a, 'p, 'ir> {
    source: &'a Source<'p>,
    functions: &'a HashMap<DeclId, FunctionId>,
    constants: &'a [Constant],
    builder: FunctionBuilder<'ir>,
}
impl Lowerer<'_, '_, '_> {
    fn value_expression(
        &mut self,
        state: &mut State,
        expr: &Expression,
    ) -> anyhow::Result<Operand> {
        if let Some(constant) = literal(self.source, expr)? {
            let index = self
                .constants
                .iter()
                .position(|c| c.ty == constant.ty && c.kind == constant.kind)
                .ok_or_else(|| self.source.error(expr.span, "missing prepared constant"))?;
            return Ok(Operand::Constant(ConstantId(index)));
        }
        if let Some(id) = local(expr) {
            return state.locals.get(&id).copied().ok_or_else(|| {
                self.source
                    .error(expr.span, "missing initialized local SSA value")
            });
        }
        if let E::Cast { expr: source, .. } = &expr.kind
            && self.source.expression_type(source)? == self.source.expression_type(expr)?
        {
            return self.value_expression(state, source);
        }
        if let E::BinaryExpresison(binary) = &expr.kind
            && matches!(binary.operator, Operator::LogicalAnd | Operator::LogicalOr)
        {
            let left = self.value_expression(state, &binary.left)?;
            let rhs_block = self.builder.create_block(&[])?;
            let join = self
                .builder
                .create_block(&[scalar_id(&BuiltinType::Boolean)])?;
            let rhs_edge = Edge {
                target: rhs_block,
                args: vec![],
            };
            let skip_edge = Edge {
                target: join,
                args: vec![left],
            };
            let (then_edge, else_edge) = if binary.operator == Operator::LogicalAnd {
                (rhs_edge, skip_edge)
            } else {
                (skip_edge, rhs_edge)
            };
            self.builder.set_terminator(
                state.block,
                Terminator::Branch {
                    condition: left,
                    then_edge,
                    else_edge,
                },
            )?;
            state.block = rhs_block;
            let right = self.value_expression(state, &binary.right)?;
            self.builder.set_terminator(
                state.block,
                Terminator::Jump(Edge {
                    target: join,
                    args: vec![right],
                }),
            )?;
            state.block = join;
            return Ok(Operand::Value(self.builder.block_params(join)?[0]));
        }
        let kind = match &expr.kind {
            E::Call {
                callee: function,
                args,
            } => {
                let callee = callee(function, self.functions)
                    .ok_or_else(|| self.source.error(expr.span, "unsupported call target"))?;
                let args = args
                    .iter()
                    .map(|arg| self.value_expression(state, arg))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                InstructionKind::Call { callee, args }
            }
            E::UnaryExpression(unary) => InstructionKind::Unary {
                op: if unary.operator == Operator::Minus {
                    UnaryOp::Neg
                } else {
                    UnaryOp::Not
                },
                operand: self.value_expression(state, &unary.expression)?,
            },
            E::Cast { expr, .. } => InstructionKind::Cast {
                kind: CastKind::Numeric,
                operand: self.value_expression(state, expr)?,
            },
            E::BinaryExpresison(binary) => InstructionKind::Binary {
                op: binary_op(binary.operator)
                    .ok_or_else(|| self.source.error(expr.span, "unsupported binary operator"))?,
                lhs: self.value_expression(state, &binary.left)?,
                rhs: self.value_expression(state, &binary.right)?,
            },
            _ => return Err(self.source.error(expr.span, "unsupported expression")),
        };
        let ty = self.source.expression_type(expr)?;
        let value = self.builder.append_instruction(state.block, kind, &[ty])?[0];
        Ok(Operand::Value(value))
    }
    fn expression_statement(&mut self, state: &mut State, expr: &Expression) -> anyhow::Result<()> {
        let E::Call {
            callee: function,
            args,
        } = &expr.kind
        else {
            return Err(self
                .source
                .error(expr.span, "only calls may be used as expression statements"));
        };
        let callee = callee(function, self.functions)
            .ok_or_else(|| self.source.error(expr.span, "unsupported call target"))?;
        let args = args
            .iter()
            .map(|arg| self.value_expression(state, arg))
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.builder.append_instruction(
            state.block,
            InstructionKind::Call { callee, args },
            &[],
        )?;
        Ok(())
    }
    fn statements(&mut self, nodes: &[Statement], state: State) -> anyhow::Result<Option<State>> {
        let mut current = Some(state);
        for node in nodes {
            let Some(mut state) = current.take() else {
                break;
            };
            match &node.kind {
                S::VariableDeclaration(variable) => {
                    let Some(BindingId::Local(id)) = variable.annotation.binding else {
                        return Err(self.source.error(node.span, "missing local identity"));
                    };
                    let value = self.value_expression(
                        &mut state,
                        variable.expression.as_ref().expect("checked initializer"),
                    )?;
                    state.locals.insert(id, value);
                }
                S::Assignment { left, right } => {
                    let value = self.value_expression(&mut state, right)?;
                    state.locals.insert(
                        local(left).ok_or_else(|| {
                            self.source.error(left.span, "missing assignment identity")
                        })?,
                        value,
                    );
                }
                S::ExpressionStatement(expression) => {
                    self.expression_statement(&mut state, expression)?;
                }
                S::Return(None) => {
                    self.builder
                        .set_terminator(state.block, Terminator::Return { values: vec![] })?;
                    continue;
                }
                S::Return(Some(values)) => {
                    let value = self.value_expression(&mut state, &values[0])?;
                    self.builder.set_terminator(
                        state.block,
                        Terminator::Return {
                            values: vec![value],
                        },
                    )?;
                    continue;
                }
                S::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    current = self.conditional(
                        state,
                        condition,
                        then_branch,
                        else_branch.as_deref().unwrap_or(&[]),
                    )?;
                    continue;
                }
                _ => return Err(self.source.error(node.span, "unsupported statement")),
            }
            current = Some(state);
        }
        Ok(current)
    }
    fn scope(&mut self, nodes: &[Statement], state: State) -> anyhow::Result<Option<State>> {
        let visible = state.locals.clone();
        let mut next = self.statements(nodes, state)?;
        if let Some(next) = &mut next {
            next.locals.retain(|id, _| visible.contains_key(id));
        }
        Ok(next)
    }
    fn conditional(
        &mut self,
        mut state: State,
        condition: &Expression,
        then_branch: &[Statement],
        else_branch: &[Statement],
    ) -> anyhow::Result<Option<State>> {
        if let Some(known) = known_boolean(condition) {
            return self.scope(if known { then_branch } else { else_branch }, state);
        }
        let value = self.value_expression(&mut state, condition)?;
        let then_block = self.builder.create_block(&[])?;
        let else_block = self.builder.create_block(&[])?;
        self.builder.set_terminator(
            state.block,
            Terminator::Branch {
                condition: value,
                then_edge: Edge {
                    target: then_block,
                    args: vec![],
                },
                else_edge: Edge {
                    target: else_block,
                    args: vec![],
                },
            },
        )?;
        let then_state = self.scope(
            then_branch,
            State {
                block: then_block,
                locals: state.locals.clone(),
            },
        )?;
        let else_state = self.scope(
            else_branch,
            State {
                block: else_block,
                locals: state.locals.clone(),
            },
        )?;
        match (then_state, else_state) {
            (None, None) => Ok(None),
            (Some(next), None) | (None, Some(next)) => Ok(Some(next)),
            (Some(left), Some(right)) => {
                let changed: Vec<_> = state
                    .locals
                    .keys()
                    .filter(|id| left.locals.get(id) != right.locals.get(id))
                    .copied()
                    .collect();
                let types = changed
                    .iter()
                    .map(|id| {
                        let ty = self.source.project.types.locals.get(id).ok_or_else(|| {
                            self.source
                                .error(condition.span, "missing local semantic type")
                        })?;
                        self.source.value_type(*ty, condition.span)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let join = self.builder.create_block(&types)?;
                for (id, &value) in changed.iter().zip(self.builder.block_params(join)?) {
                    state.locals.insert(*id, Operand::Value(value));
                }
                // Unchanged values may have been assigned identically in both arms.
                for (&id, &value) in &left.locals {
                    if !changed.contains(&id) {
                        state.locals.insert(id, value);
                    }
                }
                for next in [left, right] {
                    let args = changed
                        .iter()
                        .map(|id| {
                            next.locals.get(id).copied().ok_or_else(|| {
                                self.source
                                    .error(condition.span, "missing branch SSA value")
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    self.builder.set_terminator(
                        next.block,
                        Terminator::Jump(Edge { target: join, args }),
                    )?;
                }
                state.block = join;
                Ok(Some(state))
            }
        }
    }
}
