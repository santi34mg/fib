//! Validated analyzed-source to owned SSA lowering. Pipeline orchestration and
//! physical ABI placement belong to the driver and backend respectively.
use crate::{
    diagnostics::{CompilerError, ErrorKind, Span},
    frontend::{
        analysis_annotation::{BindingId, MemberKind, ValueCategory},
        analyzer::{AnalyzedProject, evaluate_numeric, known_boolean, validate_analysis},
        ast::{
            expression::{Expression, ExpressionKind as E},
            pattern::Pattern,
            statement::{Statement, StatementKind as S},
            switch::SwitchArm,
        },
        identifier::{DeclId, Entity, LocalId, ResolvedName},
        semantic_type::{SemanticType, TypeId as SemanticId},
        tokens::{Literal, Operator, builtin::BuiltinType},
    },
    ir::*,
    plan::FileId,
};
use std::collections::{BTreeMap, HashMap};

mod runtime_types;
mod storage;
use runtime_types::register_runtime_types;
use storage::storage_bindings;
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
    runtime_types: &'a HashMap<SemanticId, TypeId>,
    pointers: &'a HashMap<TypeId, TypeId>,
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
        let ty = self.project.types.canonical(ty)?;
        if matches!(
            self.project.types.get(ty)?,
            SemanticType::Builtin(BuiltinType::Void)
        ) {
            return Ok(None);
        }
        self.runtime_types
            .get(&ty)
            .copied()
            .map(Some)
            .ok_or_else(|| self.error(span, "unsupported runtime type in IR lowering"))
    }
    fn pointer_type(&self, ty: TypeId) -> anyhow::Result<TypeId> {
        self.pointers
            .get(&ty)
            .copied()
            .ok_or_else(|| self.error(Span::new(1, 1), "unsupported memory storage type"))
    }
    fn intrinsic(&self, callee: &Expression) -> bool {
        if let E::QualifiedIdentifier(name) = &callee.kind
            && let Some(ResolvedName::Entity(Entity::Declaration(id))) = name.annotation.resolved
        {
            return self
                .project
                .builtins
                .get(&id)
                .is_some_and(|b| b.is_intrinsic());
        }
        false
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
    fn variant(&self, expr: &Expression) -> Option<(SemanticId, usize)> {
        if let E::ExprAcess { member, .. } = &expr.kind {
            let info = member.resolved?;
            if let MemberKind::EnumVariant(index) = info.kind {
                return Some((info.owner, index as usize));
            }
        }
        None
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
/// have no return values; value-returning functions must return or diverge.
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
    if project.builtins.values().any(|b| !b.is_intrinsic())
        || project
            .types
            .types
            .iter()
            .any(|ty| matches!(ty, SemanticType::Builtin(BuiltinType::String)))
    {
        ir.types.push(IrType::Bytes);
    }
    let runtime_types = register_runtime_types(project, &mut ir)?;
    let mut pointers = HashMap::new();
    for (index, ty) in ir.types.iter().enumerate() {
        if let IrType::Pointer { pointee } = ty {
            pointers.insert(*pointee, TypeId(index));
        }
    }
    // Storage needs an address type even when the source never takes its address.
    for index in 0..ir.types.len() {
        let ty = TypeId(index);
        if !matches!(ir.types[index], IrType::Bytes) && !pointers.contains_key(&ty) {
            let id = TypeId(ir.types.len());
            ir.types.push(IrType::Pointer { pointee: ty });
            pointers.insert(ty, id);
        }
    }
    for (index, ty) in ir.types.iter().enumerate() {
        if matches!(ty, IrType::Pointer { .. }) {
            ir.constants.push(Constant {
                ty: TypeId(index),
                kind: ConstantKind::Null,
            });
        }
    }
    // Source enum tags are declaration-order ordinals, bounded by semantic IDs.
    for ty in &ir.types {
        if let IrType::Enum { variants, .. } = ty {
            for v in variants {
                let c = Constant {
                    ty: scalar_id(&BuiltinType::UInt32),
                    kind: ConstantKind::Integer {
                        bits: v.discriminant,
                    },
                };
                if !ir
                    .constants
                    .iter()
                    .any(|x| x.ty == c.ty && x.kind == c.kind)
                {
                    ir.constants.push(c);
                }
            }
        }
    }
    let mut functions = HashMap::new();
    let mut declarations = Vec::new();

    // Sort builtins to assign stable function IDs regardless of map iteration order.
    let mut builtins = project.builtins.iter().collect::<Vec<_>>();
    builtins.sort_by_key(|(declaration, _)| (declaration.file_id, declaration.index));
    for (&declaration, &builtin) in builtins {
        if builtin.is_intrinsic() {
            continue;
        }
        let source = Source {
            project,
            file: declaration.file_id,
            runtime_types: &runtime_types,
            pointers: &pointers,
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

    let entry_name = if !project.builtins.values().any(|b| !b.is_intrinsic()) {
        "main"
    } else {
        "build"
    };
    for unit in &project.units {
        let source = Source {
            project,
            file: unit.source_file_id,
            runtime_types: &runtime_types,
            pointers: &pointers,
        };
        for node in &unit.ast.statements {
            if matches!(&node.kind, S::VariableDeclaration(v) if v.is_comp) {
                continue;
            }
            let S::FunctionDeclaration(function) = &node.kind else {
                return Err(source.error(
                    node.span,
                    "only function definitions are supported at file scope",
                ));
            };
            let signature = &function.signature;
            if signature.is_comp {
                continue;
            }
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
            runtime_types: &runtime_types,
            pointers: &pointers,
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
        let source = Source {
            project,
            file,
            runtime_types: &runtime_types,
            pointers: &pointers,
        };
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
        let body = &function
            .body
            .as_ref()
            .expect("checked definition")
            .statements;
        let mut stored = storage_bindings(body, &source)?;
        for parameter in &function.signature.parameters {
            if let Some(BindingId::Local(id)) = parameter.annotation.binding
                && source.is_aggregate_local(id)?
            {
                stored.insert(id);
            }
        }
        let mut slots = BTreeMap::new();
        for local in stored {
            let ty = source.value_type(source.project.types.locals[&local], span)?;
            let slot = builder.create_stack_slot(ty, None)?;
            let address = Operand::Value(
                builder.append_instruction(
                    entry,
                    InstructionKind::StackAddress(slot),
                    &[source.pointer_type(ty)?],
                )?[0],
            );
            if let Some(value) = locals.remove(&local) {
                builder.append_instruction(
                    entry,
                    InstructionKind::Store {
                        address: Address(address),
                        value,
                    },
                    &[],
                )?;
            }
            slots.insert(local, address);
        }
        let mut lowerer = Lowerer {
            source: &source,
            functions: &functions,
            constants: &constants,
            builder,
            loops: Vec::new(),
            slots,
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
        E::Literal(Literal::Null) => Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Null,
        }),
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
        if self.source.variant(expr).is_some() {
            self.source.expression_type(expr)?;
            return Ok(());
        }
        if let E::Call { callee, args } = &expr.kind
            && self.source.variant(callee).is_some()
        {
            self.source.expression_type(expr)?;
            for arg in args {
                self.expression(arg)?;
            }
            return Ok(());
        }
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
            } if callee(function, self.functions).is_some() || self.source.intrinsic(function) => {
                for arg in args {
                    self.expression(arg)?;
                }
                Ok(())
            }
            E::UnaryExpression(unary)
                if matches!(
                    unary.operator,
                    Operator::Minus
                        | Operator::Tilde
                        | Operator::LogicalNot
                        | Operator::AddressOf
                        | Operator::Dereference
                ) =>
            {
                self.expression(&unary.expression)
            }
            E::ExprAcess { expr, member, .. }
                if matches!(member.resolved.map(|m| m.kind), Some(MemberKind::Field(_))) =>
            {
                self.expression(expr)
            }
            E::StructConstruct { fields, .. } => {
                for field in fields {
                    self.expression(&field.expression)?;
                }
                Ok(())
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
                    if let Some(expr) = &variable.expression {
                        self.expression(expr)?;
                    }
                }
                S::VariableDeclaration(variable) if variable.is_comp => {}
                S::Assignment { left, right } => {
                    self.expression(left)?;
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
                S::While { condition, body } => {
                    if let Some(condition) = condition {
                        self.expression(condition)?;
                    }
                    self.statements(body)?;
                }
                S::Switch { subject, arms } => {
                    self.expression(subject)?;
                    for arm in arms {
                        self.statements(&arm.body)?;
                    }
                }
                S::Break | S::Continue => {}
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
// Every edge uses the same ordered set of bindings visible before the loop.
// Body-local bindings never escape through a break or continue.
struct LoopContext {
    header: BlockId,
    exit: Option<BlockId>,
    locals: Vec<LocalId>,
    types: Vec<TypeId>,
}
struct Lowerer<'a, 'p, 'ir> {
    source: &'a Source<'p>,
    functions: &'a HashMap<DeclId, FunctionId>,
    constants: &'a [Constant],
    builder: FunctionBuilder<'ir>,
    loops: Vec<LoopContext>,
    slots: BTreeMap<LocalId, Operand>,
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
        let construction = match &expr.kind {
            E::Call { callee, args } => self.source.variant(callee).map(|v| (v, args.as_slice())),
            _ => self.source.variant(expr).map(|v| (v, &[][..])),
        };
        if let Some(((owner, variant), args)) = construction {
            let SemanticType::Enum(variants) = self
                .source
                .project
                .types
                .get(self.source.project.types.canonical(owner)?)?
            else {
                unreachable!()
            };
            let payload = variants[variant].payload;
            let mut elements = Vec::new();
            for arg in args {
                elements.push(self.value_expression(state, arg)?);
            }
            let fields = if let Some(payload) = payload {
                vec![self.instruction(
                    state,
                    InstructionKind::Aggregate { elements },
                    self.source.value_type(payload, expr.span)?,
                )?]
            } else {
                vec![]
            };
            return self.instruction(
                state,
                InstructionKind::Enum { variant, fields },
                self.source.expression_type(expr)?,
            );
        }
        if self.is_place(expr) {
            let address = self.place(state, expr)?;
            let ty = self.source.expression_type(expr)?;
            return self.instruction(
                state,
                InstructionKind::Load {
                    address: Address(address),
                },
                ty,
            );
        }
        if let Some(id) = local(expr) {
            return state.locals.get(&id).copied().ok_or_else(|| {
                self.source
                    .error(expr.span, "missing initialized local SSA value")
            });
        }
        if let E::UnaryExpression(unary) = &expr.kind
            && unary.operator == Operator::AddressOf
        {
            return self.place(state, &unary.expression);
        }
        if let E::ExprAcess {
            expr: receiver,
            member,
            ..
        } = &expr.kind
        {
            let value = self.value_expression(state, receiver)?;
            let Some(MemberKind::Field(field)) = member.resolved.map(|m| m.kind) else {
                return Err(self.source.error(expr.span, "unsupported member"));
            };
            return self.instruction(
                state,
                InstructionKind::ExtractField {
                    aggregate: value,
                    field: field as usize,
                },
                self.source.expression_type(expr)?,
            );
        }
        if let E::StructConstruct { fields, .. } = &expr.kind {
            let mut elements = vec![None; fields.len()];
            for field in fields {
                let Some(MemberKind::Field(index)) = field.annotation.resolved.map(|m| m.kind)
                else {
                    return Err(self
                        .source
                        .error(expr.span, "missing initializer field identity"));
                };
                let destination = elements.get_mut(index as usize).ok_or_else(|| {
                    self.source
                        .error(expr.span, "initializer field index out of bounds")
                })?;
                *destination = Some(self.value_expression(state, &field.expression)?);
            }
            let elements = elements
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    self.source
                        .error(expr.span, "incomplete struct initializer")
                })?;
            return self.instruction(
                state,
                InstructionKind::Aggregate { elements },
                self.source.expression_type(expr)?,
            );
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
    fn instruction(
        &mut self,
        state: &State,
        kind: InstructionKind,
        ty: TypeId,
    ) -> anyhow::Result<Operand> {
        Ok(Operand::Value(
            self.builder.append_instruction(state.block, kind, &[ty])?[0],
        ))
    }
    fn is_place(&self, expr: &Expression) -> bool {
        if let Some(id) = local(expr) {
            return self.slots.contains_key(&id);
        }
        expr.annotation
            .resolved
            .is_some_and(|i| i.category != ValueCategory::Value)
    }
    fn place(&mut self, state: &mut State, expr: &Expression) -> anyhow::Result<Operand> {
        if let Some(id) = local(expr) {
            return self
                .slots
                .get(&id)
                .copied()
                .ok_or_else(|| self.source.error(expr.span, "binding has no storage"));
        }
        match &expr.kind {
            E::ExprAcess {
                expr: base, member, ..
            } => {
                let base = self.place(state, base)?;
                let Some(MemberKind::Field(field)) = member.resolved.map(|m| m.kind) else {
                    return Err(self.source.error(expr.span, "unsupported field address"));
                };
                self.instruction(
                    state,
                    InstructionKind::FieldAddress {
                        base: Address(base),
                        field: field as usize,
                    },
                    self.source
                        .pointer_type(self.source.expression_type(expr)?)?,
                )
            }
            E::UnaryExpression(unary) if unary.operator == Operator::Dereference => {
                let pointer = self.value_expression(state, &unary.expression)?;
                let ty = self.source.expression_type(&unary.expression)?;
                let null = self
                    .constants
                    .iter()
                    .position(|c| c.ty == ty && c.kind == ConstantKind::Null)
                    .ok_or_else(|| self.source.error(expr.span, "missing null constant"))?;
                let valid = self.instruction(
                    state,
                    InstructionKind::Binary {
                        op: BinaryOp::Ne,
                        lhs: pointer,
                        rhs: Operand::Constant(ConstantId(null)),
                    },
                    scalar_id(&BuiltinType::Boolean),
                )?;
                let next = self.builder.create_block(&[])?;
                let fail = self.builder.create_block(&[])?;
                self.builder.set_terminator(
                    state.block,
                    Terminator::Branch {
                        condition: valid,
                        then_edge: Edge {
                            target: next,
                            args: vec![],
                        },
                        else_edge: Edge {
                            target: fail,
                            args: vec![],
                        },
                    },
                )?;
                self.builder
                    .set_terminator(fail, Terminator::Trap(TrapReason::NullPointerDereference))?;
                state.block = next;
                Ok(pointer)
            }
            E::Call { callee, args } if self.source.intrinsic(callee) => {
                let [pointer] = args.as_slice() else {
                    return Err(self.source.error(expr.span, "invalid intrinsic arity"));
                };
                self.value_expression(state, pointer)
            }
            _ => Err(self
                .source
                .error(expr.span, "unsupported place in IR lowering")),
        }
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
                S::VariableDeclaration(variable) if variable.is_comp => {}
                S::VariableDeclaration(variable) => {
                    let Some(BindingId::Local(id)) = variable.annotation.binding else {
                        return Err(self.source.error(node.span, "missing local identity"));
                    };
                    if let Some(expr) = &variable.expression {
                        let value = self.value_expression(&mut state, expr)?;
                        if let Some(&address) = self.slots.get(&id) {
                            self.builder.append_instruction(
                                state.block,
                                InstructionKind::Store {
                                    address: Address(address),
                                    value,
                                },
                                &[],
                            )?;
                        } else {
                            state.locals.insert(id, value);
                        }
                    }
                }
                S::Assignment { left, right } => {
                    let value = self.value_expression(&mut state, right)?;
                    if let Some(id) = local(left)
                        && !self.slots.contains_key(&id)
                    {
                        state.locals.insert(id, value);
                    } else {
                        let address = self.place(&mut state, left)?;
                        self.builder.append_instruction(
                            state.block,
                            InstructionKind::Store {
                                address: Address(address),
                                value,
                            },
                            &[],
                        )?;
                    }
                }
                S::ExpressionStatement(expression) => {
                    self.expression_statement(&mut state, expression)?;
                }
                S::Switch { subject, arms } => {
                    current = self.switch(state, subject, arms)?;
                    continue;
                }
                S::While { condition, body } => {
                    current = self.while_loop(state, condition.as_ref(), body, node.span)?;
                    continue;
                }
                S::Break | S::Continue => {
                    self.loop_jump(state, matches!(node.kind, S::Break), node.span)?;
                    continue;
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
    fn switch(
        &mut self,
        mut state: State,
        subject: &Expression,
        arms: &[SwitchArm],
    ) -> anyhow::Result<Option<State>> {
        let value = self.value_expression(&mut state, subject)?;
        let tag = self.instruction(
            &state,
            InstructionKind::EnumTag { value },
            scalar_id(&BuiltinType::UInt32),
        )?;
        let ty = subject.annotation.resolved.expect("validated switch").ty;
        let SemanticType::Enum(variants) = self
            .source
            .project
            .types
            .get(self.source.project.types.canonical(ty)?)?
        else {
            unreachable!()
        };
        let mut cases = Vec::new();
        let mut default = None;
        let mut branches = Vec::new();
        for arm in arms {
            let block = self.builder.create_block(&[])?;
            match &arm.pattern {
                Pattern::EnumVariant { member, .. } => {
                    let MemberKind::EnumVariant(index) =
                        member.resolved.expect("validated pattern").kind
                    else {
                        unreachable!()
                    };
                    let constant = self
                        .constants
                        .iter()
                        .position(|c| {
                            c.ty == scalar_id(&BuiltinType::UInt32)
                                && c.kind
                                    == (ConstantKind::Integer {
                                        bits: index as u128,
                                    })
                        })
                        .expect("prepared enum tag");
                    cases.push(SwitchCase {
                        value: ConstantId(constant),
                        edge: Edge {
                            target: block,
                            args: vec![],
                        },
                    });
                }
                Pattern::Wildcard => {
                    default = Some(Edge {
                        target: block,
                        args: vec![],
                    })
                }
            }
            branches.push((block, arm));
        }
        let mut continuing = Vec::new();
        let default = match default {
            Some(edge) => edge,
            None => {
                let block = self.builder.create_block(&[])?;
                if cases.len() == variants.len() {
                    self.builder
                        .set_terminator(block, Terminator::Unreachable)?;
                } else {
                    continuing.push(State {
                        block,
                        locals: state.locals.clone(),
                    });
                }
                Edge {
                    target: block,
                    args: vec![],
                }
            }
        };
        self.builder.set_terminator(
            state.block,
            Terminator::Switch {
                subject: tag,
                cases,
                default,
            },
        )?;
        for (block, arm) in branches {
            let branch = State {
                block,
                locals: state.locals.clone(),
            };
            if let Pattern::EnumVariant {
                member,
                binding: Some(binding),
                ..
            } = &arm.pattern
            {
                let MemberKind::EnumVariant(index) =
                    member.resolved.expect("validated pattern").kind
                else {
                    unreachable!()
                };
                let Some(BindingId::Local(id)) = binding.annotation.binding else {
                    return Err(self
                        .source
                        .error(subject.span, "missing pattern binding identity"));
                };
                let payload = self.instruction(
                    &branch,
                    InstructionKind::EnumField {
                        value,
                        variant: index as usize,
                        field: 0,
                    },
                    self.source.value_type(
                        variants[index as usize].payload.expect("validated payload"),
                        subject.span,
                    )?,
                )?;
                let address = self.slots[&id];
                self.builder.append_instruction(
                    block,
                    InstructionKind::Store {
                        address: Address(address),
                        value: payload,
                    },
                    &[],
                )?;
            }
            if let Some(mut next) = self.statements(&arm.body, branch)? {
                next.locals.retain(|id, _| state.locals.contains_key(id));
                continuing.push(next);
            }
        }
        if continuing.is_empty() {
            return Ok(None);
        }
        if continuing.len() == 1 {
            return Ok(continuing.pop());
        }
        let locals = state.locals.keys().copied().collect::<Vec<_>>();
        let types = locals
            .iter()
            .map(|id| {
                self.source
                    .value_type(self.source.project.types.locals[id], subject.span)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let join = self.builder.create_block(&types)?;
        for branch in continuing {
            self.builder.set_terminator(
                branch.block,
                Terminator::Jump(Edge {
                    target: join,
                    args: locals.iter().map(|id| branch.locals[id]).collect(),
                }),
            )?;
        }
        state.block = join;
        state.locals = locals
            .into_iter()
            .zip(
                self.builder
                    .block_params(join)?
                    .iter()
                    .copied()
                    .map(Operand::Value),
            )
            .collect();
        Ok(Some(state))
    }
    fn scope(&mut self, nodes: &[Statement], state: State) -> anyhow::Result<Option<State>> {
        let visible = state.locals.clone();
        let mut next = self.statements(nodes, state)?;
        if let Some(next) = &mut next {
            next.locals.retain(|id, _| visible.contains_key(id));
        }
        Ok(next)
    }
    fn loop_jump(&mut self, state: State, is_break: bool, span: Span) -> anyhow::Result<()> {
        let context = self.loops.last_mut().ok_or_else(|| {
            self.source
                .error(span, "break/continue requires an enclosing loop")
        })?;
        let target = if is_break {
            match context.exit {
                Some(exit) => exit,
                None => {
                    let exit = self.builder.create_block(&context.types)?;
                    context.exit = Some(exit);
                    exit
                }
            }
        } else {
            context.header
        };
        let args = context.locals.iter().map(|id| state.locals[id]).collect();
        self.builder
            .set_terminator(state.block, Terminator::Jump(Edge { target, args }))?;
        Ok(())
    }

    fn while_loop(
        &mut self,
        state: State,
        condition: Option<&Expression>,
        body: &[Statement],
        span: Span,
    ) -> anyhow::Result<Option<State>> {
        let known = condition.map(known_boolean).unwrap_or(Some(true));
        if known == Some(false) {
            return Ok(Some(state));
        }
        let locals = state.locals.keys().copied().collect::<Vec<_>>();
        let types =
            locals
                .iter()
                .map(|id| {
                    let ty =
                        self.source.project.types.locals.get(id).ok_or_else(|| {
                            self.source.error(span, "missing local semantic type")
                        })?;
                    self.source.value_type(*ty, span)
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
        let header = self.builder.create_block(&types)?;
        self.builder.set_terminator(
            state.block,
            Terminator::Jump(Edge {
                target: header,
                args: locals.iter().map(|id| state.locals[id]).collect(),
            }),
        )?;
        let mut header_state = State {
            block: header,
            locals: locals
                .iter()
                .copied()
                .zip(
                    self.builder
                        .block_params(header)?
                        .iter()
                        .copied()
                        .map(Operand::Value),
                )
                .collect(),
        };
        let exit = if known == Some(true) {
            None
        } else {
            // Short-circuit conditions can change header_state.block. The branch
            // belongs to the final condition block, while continues re-enter header.
            let value =
                self.value_expression(&mut header_state, condition.expect("dynamic condition"))?;
            let body_block = self.builder.create_block(&[])?;
            let exit = self.builder.create_block(&types)?;
            self.builder.set_terminator(
                header_state.block,
                Terminator::Branch {
                    condition: value,
                    then_edge: Edge {
                        target: body_block,
                        args: vec![],
                    },
                    else_edge: Edge {
                        target: exit,
                        args: locals.iter().map(|id| header_state.locals[id]).collect(),
                    },
                },
            )?;
            header_state.block = body_block;
            Some(exit)
        };
        self.loops.push(LoopContext {
            header,
            exit,
            locals,
            types,
        });
        if let Some(next) = self.scope(body, header_state)? {
            self.loop_jump(next, false, span)?;
        }
        let context = self.loops.pop().expect("active loop");
        context
            .exit
            .map(|exit| {
                Ok(State {
                    block: exit,
                    locals: context
                        .locals
                        .into_iter()
                        .zip(
                            self.builder
                                .block_params(exit)?
                                .iter()
                                .copied()
                                .map(Operand::Value),
                        )
                        .collect(),
                })
            })
            .transpose()
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

impl Source<'_> {
    fn is_aggregate_local(&self, id: LocalId) -> anyhow::Result<bool> {
        let ty = self
            .project
            .types
            .locals
            .get(&id)
            .ok_or_else(|| self.error(Span::new(1, 1), "missing local semantic type"))?;
        Ok(matches!(
            self.project.types.get(self.project.types.canonical(*ty)?)?,
            SemanticType::Struct(_) | SemanticType::Enum(_)
        ))
    }
}
