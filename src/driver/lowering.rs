//! Structured scalar AST to SSA. Physical ABI placement belongs to the backend.
use crate::{
    build_plan::FileId,
    diagnostics::{CompilerError, ErrorKind, Span},
    frontend::{
        analysis_annotation::BindingId,
        analyzer::{AnalyzedProject, known_boolean, validate_analysis},
        ast::{
            expression::{Expression, ExpressionKind as E},
            statement::{Statement, StatementKind as S},
        },
        identifier::{DeclId, Entity, LocalId, ResolvedName},
        semantic_type::{SemanticType, TypeId as SemanticId},
        tokens::{Literal, Operator, builtin::BuiltinType},
    },
    ir::*,
};
use std::collections::{BTreeMap, HashMap};
const I32: TypeId = TypeId(0);
const BOOL: TypeId = TypeId(1);

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
    fn ty(&self, ty: SemanticId, span: Span) -> anyhow::Result<TypeId> {
        match self.project.types.get(self.project.types.canonical(ty)?)? {
            SemanticType::Builtin(BuiltinType::Int32) => Ok(I32),
            SemanticType::Builtin(BuiltinType::Boolean) => Ok(BOOL),
            _ => Err(self.error(span, "IR lowering supports only int32 and bool values")),
        }
    }
    fn expression_type(&self, expr: &Expression) -> anyhow::Result<TypeId> {
        let info = expr
            .annotation
            .resolved
            .ok_or_else(|| self.error(expr.span, "missing expression annotation"))?;
        self.ty(info.ty, expr.span)
    }
}

pub(super) fn lower(project: &AnalyzedProject) -> anyhow::Result<Ir> {
    validate_analysis(project)?;
    anyhow::ensure!(
        project.units.len() == 1,
        "IR lowering requires a single file"
    );
    let unit = &project.units[0];
    let source = Source {
        project,
        file: unit.source_file_id,
    };
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed: true,
        },
        IrType::Bool,
    ];
    let mut functions = HashMap::new();
    let mut declarations = Vec::new();
    let mut main_found = false;
    for node in &unit.ast.statements {
        let S::FunctionDeclaration(function) = &node.kind else {
            return Err(source.error(
                node.span,
                "only function definitions are supported at file scope",
            ));
        };
        let signature = &function.signature;
        if function.is_extern || function.body.is_none() || !signature.generic_parameters.is_empty()
        {
            return Err(source.error(node.span, "extern and generic functions are not supported"));
        }
        let Some(BindingId::Declaration(id)) = signature.annotation.binding else {
            return Err(source.error(node.span, "missing function identity"));
        };
        let return_ty = signature
            .return_type
            .annotation
            .resolved
            .ok_or_else(|| source.error(node.span, "missing return type"))?;
        if source.ty(return_ty, node.span)? != I32 {
            return Err(source.error(node.span, "functions must return int32"));
        }
        for parameter in &signature.parameters {
            let ty = parameter
                .parameter_type
                .annotation
                .resolved
                .ok_or_else(|| source.error(node.span, "missing parameter type"))?;
            if source.ty(ty, node.span)? != I32 {
                return Err(source.error(node.span, "function parameters must be int32"));
            }
        }
        let is_main = signature.identifier == "main";
        if is_main {
            if !signature.parameters.is_empty() {
                return Err(source.error(node.span, "main must have no parameters"));
            }
            main_found = true;
        }
        let function_id = FunctionId(ir.functions.len());
        functions.insert(id, function_id);
        ir.functions.push(Function {
            name: if is_main {
                "main".into()
            } else {
                format!("fib.{}.{}.{}", id.file_id.0, id.index, signature.identifier)
            },
            signature: Signature {
                params: vec![I32; signature.parameters.len()],
                results: vec![I32],
                calling_convention: source_convention(),
            },
            linkage: if is_main {
                Linkage::External
            } else {
                Linkage::Internal
            },
            body: FunctionBody::Declaration,
        });
        declarations.push((function_id, function, node.span));
    }
    if !main_found {
        return Err(source.error(Span::new(1, 1), "a main() int32 definition is required"));
    }
    // Validate the entire subset, including dead branches, and collect constants
    // before the builder borrows IR. Forward function identities already exist.
    let mut checker = Checker {
        source: &source,
        functions: &functions,
        constants: &mut ir.constants,
    };
    for (_, function, _) in &declarations {
        checker.statements(
            &function
                .body
                .as_ref()
                .expect("checked definition")
                .statements,
        )?;
    }
    let constants = ir.constants.clone();
    for (id, function, span) in declarations {
        let mut builder = FunctionBuilder::new(&mut ir, id)?;
        let entry = builder.create_block(&vec![I32; function.signature.parameters.len()])?;
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
        if lowerer
            .statements(
                &function
                    .body
                    .as_ref()
                    .expect("checked definition")
                    .statements,
                State {
                    block: entry,
                    locals,
                },
            )?
            .is_some()
        {
            return Err(source.error(span, "function reaches its end without a return"));
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
fn comparison(operator: Operator) -> Option<BinaryOp> {
    Some(match operator {
        Operator::DoubleEquals => BinaryOp::Eq,
        Operator::Different => BinaryOp::Ne,
        Operator::LesserThan => BinaryOp::Lt,
        Operator::LesserEqual => BinaryOp::Le,
        Operator::GreaterThan => BinaryOp::Gt,
        Operator::GreaterEqual => BinaryOp::Ge,
        _ => return None,
    })
}
fn literal(expr: &Expression) -> Option<Constant> {
    match &expr.kind {
        E::Literal(Literal::Integer(bits)) if *bits <= i32::MAX as u64 => Some(Constant {
            ty: I32,
            kind: ConstantKind::Integer {
                bits: u128::from(*bits),
            },
        }),
        E::Literal(Literal::Boolean(value)) => Some(Constant {
            ty: BOOL,
            kind: ConstantKind::Bool(*value),
        }),
        E::UnaryExpression(unary) if unary.operator == Operator::Minus => {
            if let E::Literal(Literal::Integer(bits)) = unary.expression.kind
                && bits <= 1u64 << 31
            {
                Some(Constant {
                    ty: I32,
                    kind: ConstantKind::Integer {
                        bits: u128::from(0u32.wrapping_sub(bits as u32)),
                    },
                })
            } else {
                None
            }
        }
        _ => None,
    }
}
struct Checker<'a, 'p> {
    source: &'a Source<'p>,
    functions: &'a HashMap<DeclId, FunctionId>,
    constants: &'a mut Vec<Constant>,
}
impl Checker<'_, '_> {
    fn expression(&mut self, expr: &Expression) -> anyhow::Result<()> {
        self.source.expression_type(expr)?;
        if let Some(constant) = literal(expr) {
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
            E::BinaryExpresison(binary) if comparison(binary.operator).is_some() => {
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
                        self.source.ty(
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
    fn expression(&mut self, state: &mut State, expr: &Expression) -> anyhow::Result<Operand> {
        if let Some(constant) = literal(expr) {
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
        let kind = match &expr.kind {
            E::Call {
                callee: function,
                args,
            } => {
                let callee = callee(function, self.functions)
                    .ok_or_else(|| self.source.error(expr.span, "unsupported call target"))?;
                let args = args
                    .iter()
                    .map(|arg| self.expression(state, arg))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                InstructionKind::Call { callee, args }
            }
            E::BinaryExpresison(binary) => InstructionKind::Binary {
                op: comparison(binary.operator)
                    .ok_or_else(|| self.source.error(expr.span, "unsupported binary operator"))?,
                lhs: self.expression(state, &binary.left)?,
                rhs: self.expression(state, &binary.right)?,
            },
            _ => return Err(self.source.error(expr.span, "unsupported expression")),
        };
        let ty = self.source.expression_type(expr)?;
        let value = self.builder.append_instruction(state.block, kind, &[ty])?[0];
        Ok(Operand::Value(value))
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
                    let value = self.expression(
                        &mut state,
                        variable.expression.as_ref().expect("checked initializer"),
                    )?;
                    state.locals.insert(id, value);
                }
                S::Assignment { left, right } => {
                    let value = self.expression(&mut state, right)?;
                    state.locals.insert(
                        local(left).ok_or_else(|| {
                            self.source.error(left.span, "missing assignment identity")
                        })?,
                        value,
                    );
                }
                S::Return(Some(values)) => {
                    let value = self.expression(&mut state, &values[0])?;
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
        let value = self.expression(&mut state, condition)?;
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
                        self.source.ty(*ty, condition.span)
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
