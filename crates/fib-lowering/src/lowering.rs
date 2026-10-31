//! Validated analyzed-source to owned SSA lowering. Pipeline orchestration and
//! physical ABI placement belong to the driver and backend respectively.
use crate::{
    diagnostics::{CompilerError, ErrorKind, Span},
    frontend::{
        analysis_annotation::{BindingId, MemberKind, ValueCategory},
        analyzer::{
            AnalyzedProject, evaluate_boolean, evaluate_numeric, known_boolean, validate_analysis,
        },
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

mod globals;
mod indexing;
mod runtime_types;
mod storage;
use runtime_types::register_runtime_types;
use storage::storage_bindings;
const SCALARS: [BuiltinType; 17] = [
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
    BuiltinType::Usize,
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
        BuiltinType::Usize => IrType::Integer {
            width: IntegerWidth::Pointer,
            signed: false,
        },
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
    types: &'a crate::frontend::semantic_type::SemanticTypes,
    file: FileId,
    runtime_types: &'a HashMap<SemanticId, TypeId>,
    pointers: &'a HashMap<TypeId, TypeId>,
    globals: &'a HashMap<DeclId, GlobalId>,
}
impl Source<'_> {
    fn comp_value(
        &self,
        expr: &Expression,
    ) -> anyhow::Result<Option<crate::frontend::analyzer::constant_data::DataValue>> {
        if !matches!(expr.kind, E::QualifiedIdentifier(_)) {
            return Ok(None);
        }
        if expr.annotation.resolved.is_none_or(|info| {
            info.category != ValueCategory::Value
                || info.type_value.is_some()
                || matches!(self.types.get(info.ty), Ok(SemanticType::Function(_)))
        }) {
            return Ok(None);
        }
        let mut evaluator = crate::frontend::analyzer::constant_data::DataEvaluator::new(
            &self.project.units,
            self.types,
        );
        evaluator.set_file(self.file);
        if evaluator.is_value_binding(expr) {
            evaluator.evaluate(expr).map(Some)
        } else {
            Ok(None)
        }
    }
    fn error(&self, span: Span, message: impl Into<String>) -> anyhow::Error {
        let mut error = CompilerError::new(ErrorKind::Backend, message.into()).with_span(span);
        error.filename = self.project.source_paths.get(&self.file).cloned();
        if error.filename.is_none() {
            error.hint = Some(format!("source file {:?}", self.file));
        }
        error.into()
    }
    fn ir_type(&self, ty: SemanticId, span: Span) -> anyhow::Result<Option<TypeId>> {
        let ty = self.types.canonical(ty)?;
        if matches!(
            self.types.get(ty)?,
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
    fn builtin(&self, callee: &Expression) -> Option<crate::builtins::BuiltinFunction> {
        if let E::QualifiedIdentifier(name) = &callee.kind
            && let Some(ResolvedName::Entity(Entity::Declaration(id))) = name.annotation.resolved
        {
            return self.project.builtins.get(&id).copied();
        }
        None
    }
    fn intrinsic(&self, callee: &Expression) -> bool {
        self.builtin(callee).is_some_and(|b| b.is_intrinsic())
    }
    fn value_type(&self, ty: SemanticId, span: Span) -> anyhow::Result<TypeId> {
        self.runtime_types
            .get(&self.types.canonical(ty)?)
            .copied()
            .ok_or_else(|| self.error(span, "unsupported runtime value type in IR lowering"))
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
            .types
            .declarations
            .get(&declaration)
            .copied()
            .ok_or_else(|| self.error(span, "missing function semantic type"))?;
        let SemanticType::Function(function) = self.types.get(self.types.canonical(semantic)?)?
        else {
            return Err(self.error(span, "function declaration has a non-function type"));
        };
        let params = function
            .parameters
            .iter()
            .filter(|ty| {
                !matches!(
                    self.types.get(**ty),
                    Ok(SemanticType::Builtin(BuiltinType::MetaType))
                )
            })
            .map(|ty| self.value_type(*ty, span))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let results = self
            .ir_type(function.return_type, span)?
            .into_iter()
            .collect();
        Ok(Signature {
            params,
            results,
            calling_convention: match function.calling_convention {
                crate::frontend::semantic_type::CallConvention::C => CallingConvention::C,
                _ => source_convention(),
            },
        })
    }
}

/// Lowers the validated, analyzed source subset into owned SSA IR.
///
/// This entry point registers declarations across all selected source units and concrete generic instances before lowering bodies. It registers builtin
/// and user function identities so calls can refer to later declarations, then
/// checks supported constructs and gathers constants before building function
/// bodies. The resulting IR is verified before it is returned.
///
/// Unsupported declarations, expressions, or statements produce a diagnostic
/// associated with their source span. Functions that can reach their end must
/// have no return values; value-returning functions must return or diverge.
pub fn lower(project: &AnalyzedProject) -> anyhow::Result<Ir> {
    let name = if !project.builtins.values().any(|b| !b.is_intrinsic()) {
        "main"
    } else {
        "build"
    };
    lower_with_entry(project, name)
}

/// Lower a compiler-generated process wrapper as the exported C `main`.
/// The original Fib main keeps its ordinary internal calling convention.
pub fn lower_with_entry(project: &AnalyzedProject, entry_name: &str) -> anyhow::Result<Ir> {
    validate_analysis(project)?;
    let mut ir = Ir::new();
    ir.types = SCALARS[..scalar_id(&BuiltinType::String).0]
        .iter()
        .map(scalar_type)
        .collect();
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
        if matches!(ty, IrType::Pointer { .. } | IrType::FunctionPointer(_)) {
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
    let mut globals = HashMap::new();
    for unit in &project.units {
        for node in &unit.ast.statements {
            let bindings: Vec<_> = match &node.kind {
                S::VariableDeclaration(v) => vec![v],
                S::MultiVariableDeclaration { bindings, .. } => bindings.iter().collect(),
                _ => vec![],
            };
            for binding in bindings.into_iter().filter(|v| !v.is_comp) {
                let Some(BindingId::Declaration(id)) = binding.annotation.binding else {
                    anyhow::bail!("missing global identity");
                };
                let source = Source {
                    project,
                    types: &project.types,
                    file: unit.source_file_id,
                    runtime_types: &runtime_types,
                    pointers: &pointers,
                    globals: &globals,
                };
                let ty = source.value_type(project.types.declarations[&id], node.span)?;
                globals.insert(id, GlobalId(ir.globals.len()));
                ir.globals.push(Global {
                    name: format!(
                        "fib.global.{}.{}.{}",
                        id.file_id.0, id.index, binding.identifier
                    ),
                    ty,
                    // Binding const does not freeze array/slice elements. Keep
                    // storage writable; source analysis enforces permissions.
                    mutable: true,
                    linkage: Linkage::Internal,
                    initializer: GlobalInitializer::Declaration,
                });
            }
        }
    }

    // Sort builtins to assign stable function IDs regardless of map iteration order.
    let mut builtins = project.builtins.iter().collect::<Vec<_>>();
    builtins.sort_by_key(|(declaration, _)| (declaration.file_id, declaration.index));
    for (&declaration, &builtin) in builtins {
        if builtin.is_intrinsic() {
            continue;
        }
        let source = Source {
            project,
            types: &project.types,
            file: declaration.file_id,
            runtime_types: &runtime_types,
            pointers: &pointers,
            globals: &globals,
        };
        let function_id = FunctionId(ir.functions.len());
        functions.insert(FunctionKey::Declaration(declaration), function_id);
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

    for unit in &project.units {
        let source = Source {
            project,
            types: &project.types,
            file: unit.source_file_id,
            runtime_types: &runtime_types,
            pointers: &pointers,
            globals: &globals,
        };
        for node in &unit.ast.statements {
            if matches!(&node.kind, S::VariableDeclaration(v) if v.is_comp)
                || matches!(&node.kind, S::MultiVariableDeclaration { bindings, .. } if bindings.iter().all(|v| v.is_comp))
            {
                continue;
            }
            if matches!(
                node.kind,
                S::VariableDeclaration(_) | S::MultiVariableDeclaration { .. }
            ) {
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
            if source
                .types
                .generic_parameters
                .get(&match signature.annotation.binding {
                    Some(BindingId::Declaration(id)) => id,
                    _ => return Err(source.error(node.span, "missing function identity")),
                })
                .is_some_and(|parameters| !parameters.is_empty())
            {
                continue;
            }
            if !signature.generic_parameters.is_empty() {
                return Err(
                    source.error(node.span, "extern and generic functions are not supported")
                );
            }
            let Some(BindingId::Declaration(declaration)) = signature.annotation.binding else {
                return Err(source.error(node.span, "missing function identity"));
            };
            let function_id = FunctionId(ir.functions.len());
            if functions
                .insert(FunctionKey::Declaration(declaration), function_id)
                .is_some()
            {
                return Err(source.error(node.span, "duplicate lowered function identity"));
            }
            let entry = unit.source_file_id == project.entry && signature.identifier == entry_name;
            let mut lowered_signature = source.signature(declaration, node.span)?;
            if entry {
                lowered_signature.calling_convention = CallingConvention::C;
            }
            ir.functions.push(Function {
                identifier: if entry && entry_name != "build" {
                    "main".into()
                } else if entry
                    || function.is_extern
                    || (project.units.len() == 1 && entry_name == "main")
                {
                    signature.identifier.clone()
                } else {
                    format!(
                        "fib.{}.{}.{}",
                        declaration.file_id.0, declaration.index, signature.identifier
                    )
                },
                signature: lowered_signature,
                linkage: if entry || function.is_extern {
                    Linkage::External
                } else {
                    Linkage::Internal
                },
                body: FunctionBody::Declaration,
            });
            if function.body.is_some() {
                declarations.push((function_id, function, node.span, unit.source_file_id, None));
            }
        }
    }
    let mut instances = project
        .types
        .generic_instances
        .values()
        .filter(|instance| instance.body.is_some())
        .collect::<Vec<_>>();
    instances.sort_by_key(|instance| {
        (
            instance.key.template.file_id,
            instance.key.template.index,
            instance
                .key
                .arguments
                .iter()
                .map(|ty| ty.0)
                .collect::<Vec<_>>(),
        )
    });
    for instance in instances {
        let function = instance.body.as_ref().expect("concrete instance");
        if function.signature.is_comp {
            continue;
        }
        let mut environment = project.types.clone();
        for (&binding, &info) in &instance.bindings {
            environment.bind(binding, info);
        }
        environment
            .declarations
            .insert(instance.key.template, instance.signature);
        let source = Source {
            project,
            types: &environment,
            file: instance.key.template.file_id,
            runtime_types: &runtime_types,
            pointers: &pointers,
            globals: &globals,
        };
        let id = FunctionId(ir.functions.len());
        functions.insert(FunctionKey::Instance(instance.key.clone()), id);
        ir.functions.push(Function {
            identifier: format!(
                "fib.generic.{}.{}.{}",
                instance.key.template.file_id.0,
                instance.key.template.index,
                instance
                    .key
                    .arguments
                    .iter()
                    .map(|t| t.0.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
            signature: Signature {
                calling_convention: CallingConvention::Fib,
                ..source.signature(instance.key.template, Span::new(1, 1))?
            },
            linkage: Linkage::Internal,
            body: FunctionBody::Declaration,
        });
        declarations.push((
            id,
            function,
            Span::new(1, 1),
            instance.key.template.file_id,
            Some(environment),
        ));
    }
    // Validate the entire subset, including dead branches, and collect constants
    // before the builder borrows IR. Forward function identities already exist.
    globals::initialize(
        project,
        &mut ir,
        &runtime_types,
        &pointers,
        &globals,
        &functions,
    )?;
    for (_, function, _, file, environment) in &declarations {
        let source = Source {
            project,
            types: environment.as_ref().unwrap_or(&project.types),
            file: *file,
            runtime_types: &runtime_types,
            pointers: &pointers,
            globals: &globals,
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
    for (id, function, span, file, environment) in declarations {
        let source = Source {
            project,
            types: environment.as_ref().unwrap_or(&project.types),
            file,
            runtime_types: &runtime_types,
            pointers: &pointers,
            globals: &globals,
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
            .filter(|p| {
                !matches!(
                    p.parameter_type
                        .annotation
                        .resolved
                        .and_then(|ty| source.types.get(ty).ok()),
                    Some(SemanticType::Builtin(BuiltinType::MetaType))
                )
            })
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
            if matches!(
                parameter
                    .parameter_type
                    .annotation
                    .resolved
                    .and_then(|ty| source.types.get(ty).ok()),
                Some(SemanticType::Builtin(BuiltinType::MetaType))
            ) {
                continue;
            }
            if let Some(BindingId::Local(id)) = parameter.annotation.binding
                && source.is_aggregate_local(id)?
            {
                stored.insert(id);
            }
        }
        let mut slots = BTreeMap::new();
        for local in stored {
            let ty = source.value_type(source.types.locals[&local], span)?;
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
            defers: Vec::new(),
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
    CallingConvention::Fib
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FunctionKey {
    Declaration(DeclId),
    Instance(crate::frontend::analyzer::generics::GenericInstanceKey),
}
fn runtime_argument(expr: &&Expression) -> bool {
    expr.annotation
        .resolved
        .is_none_or(|info| info.type_value.is_none())
}
fn callee(
    expr: &Expression,
    args: &[Expression],
    functions: &HashMap<FunctionKey, FunctionId>,
) -> Option<FunctionId> {
    if let E::QualifiedIdentifier(name) = &expr.kind
        && let Some(ResolvedName::Entity(Entity::Declaration(id))) = name.annotation.resolved
    {
        let arguments = args
            .iter()
            .filter_map(|arg| arg.annotation.resolved?.type_value)
            .collect::<Vec<_>>();
        let key = if arguments.is_empty() {
            FunctionKey::Declaration(id)
        } else {
            FunctionKey::Instance(crate::frontend::analyzer::generics::GenericInstanceKey {
                template: id,
                arguments,
            })
        };
        functions.get(&key).copied()
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
fn unit_comparison(source: &Source<'_>, expr: &Expression) -> anyhow::Result<Option<Constant>> {
    if let E::BinaryExpresison(binary) = &expr.kind
        && matches!(
            binary.operator,
            Operator::DoubleEquals | Operator::Different
        )
        && source
            .ir_type(
                binary
                    .left
                    .annotation
                    .resolved
                    .expect("validated operand")
                    .ty,
                binary.left.span,
            )?
            .is_none()
    {
        return Ok(Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Bool(binary.operator == Operator::DoubleEquals),
        }));
    }
    Ok(None)
}
fn array_length(source: &Source<'_>, expr: &Expression) -> anyhow::Result<Option<Constant>> {
    if let E::ExprAcess { member, .. } = &expr.kind
        && let Some(info) = member.resolved
        && info.kind == MemberKind::Length
        && let (SemanticType::Array { length, .. }, _) = source.types.container(info.owner)?
    {
        return Ok(Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Integer {
                bits: u128::from(*length),
            },
        }));
    }
    Ok(None)
}
fn literal(source: &Source<'_>, expr: &Expression) -> anyhow::Result<Option<Constant>> {
    if let Some(value) =
        evaluate_numeric(expr, source.types).map_err(|e| source.error(expr.span, e.to_string()))?
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
    if let Some(value) = evaluate_boolean(expr, source.types)? {
        return Ok(Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Bool(value),
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
        E::Literal(Literal::CString(value)) => Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::CString(value.as_bytes().to_vec()),
        }),
        E::SliceLiteral {
            object,
            start: None,
            end: None,
            ..
        } if matches!(object.kind, E::Literal(Literal::String(_))) => {
            let E::Literal(Literal::String(value)) = &object.kind else {
                unreachable!()
            };
            Some(Constant {
                ty: source.expression_type(expr)?,
                kind: ConstantKind::Bytes(value.as_bytes().to_vec()),
            })
        }
        E::Literal(Literal::String(value)) => Some(Constant {
            ty: source.expression_type(expr)?,
            kind: ConstantKind::Bytes(value.as_bytes().to_vec()),
        }),
        _ => None,
    })
}
struct Checker<'a, 'p> {
    source: &'a Source<'p>,
    functions: &'a HashMap<FunctionKey, FunctionId>,
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
        if let E::Call { callee, args } = &expr.kind
            && self.source.builtin(callee) == Some(crate::builtins::BuiltinFunction::Sizeof)
        {
            let ty = args[0]
                .annotation
                .resolved
                .and_then(|i| i.type_value)
                .ok_or_else(|| {
                    self.source
                        .error(expr.span, "sizeof requires a type argument")
                })?;
            self.source.ir_type(ty, expr.span)?.ok_or_else(|| {
                self.source
                    .error(expr.span, "sizeof requires a sized runtime type")
            })?;
            return Ok(());
        }
        if self.source.comp_value(expr)?.is_some() {
            return Ok(());
        }
        if let Some(constant) = unit_comparison(self.source, expr)? {
            let E::BinaryExpresison(binary) = &expr.kind else {
                unreachable!()
            };
            self.expression(&binary.left)?;
            self.expression(&binary.right)?;
            if !self
                .constants
                .iter()
                .any(|c| c.ty == constant.ty && c.kind == constant.kind)
            {
                self.constants.push(constant);
            }
            return Ok(());
        }
        if let Some(constant) = array_length(self.source, expr)? {
            if let E::ExprAcess { expr: receiver, .. } = &expr.kind {
                self.expression(receiver)?;
            }
            if !self
                .constants
                .iter()
                .any(|c| c.ty == constant.ty && c.kind == constant.kind)
            {
                self.constants.push(constant);
            }
            return Ok(());
        }
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
            E::VoidExpression => Ok(()),
            E::QualifiedIdentifier(_)
                if local(expr).is_some()
                    || callee(expr, &[], self.functions).is_some()
                    || globals::global(expr, self.source.globals).is_some() =>
            {
                Ok(())
            }
            E::Call {
                callee: function,
                args,
            } => {
                if callee(function, args, self.functions).is_none()
                    && !self.source.intrinsic(function)
                {
                    self.expression(function)?;
                }
                for arg in args.iter().filter(runtime_argument) {
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
                if matches!(
                    member.resolved.map(|m| m.kind),
                    Some(MemberKind::Field(_) | MemberKind::Length)
                ) =>
            {
                self.expression(expr)
            }
            E::ArrayLiteral { elements } => {
                for element in elements {
                    self.expression(element)?;
                }
                Ok(())
            }
            E::IndexAccess { object, index } => {
                self.expression(object)?;
                self.expression(index)
            }
            E::SliceLiteral {
                object, start, end, ..
            } => {
                self.expression(object)?;
                for bound in start.iter().chain(end) {
                    self.expression(bound)?;
                }
                Ok(())
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
                S::MultiVariableDeclaration { bindings, values } => {
                    if !values.is_empty() && values.len() != bindings.len() {
                        return Err(self
                            .source
                            .error(node.span, "unsupported multi-result initializer"));
                    }
                    for (index, binding) in bindings.iter().enumerate() {
                        if !binding.is_comp {
                            if let Some(ty) = &binding.ty {
                                self.source.value_type(
                                    ty.annotation.resolved.ok_or_else(|| {
                                        self.source.error(node.span, "missing binding type")
                                    })?,
                                    node.span,
                                )?;
                            }
                            if let Some(value) = values.get(index) {
                                self.expression(value)?;
                            }
                        }
                    }
                }
                S::Assignment { left, right } => {
                    self.expression(left)?;
                    self.expression(right)?;
                }
                S::IndexAssign {
                    object,
                    index,
                    expr,
                } => {
                    self.expression(object)?;
                    self.expression(index)?;
                    self.expression(expr)?;
                }
                S::MultiAssignment { targets, values } => {
                    for expr in targets.iter().chain(values) {
                        self.expression(expr)?;
                    }
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
                    if callee(function, args, self.functions).is_none()
                        && !self.source.intrinsic(function)
                    {
                        self.expression(function)?;
                    }
                    self.source.ir_type(
                        expression
                            .annotation
                            .resolved
                            .ok_or_else(|| {
                                self.source
                                    .error(expression.span, "missing expression annotation")
                            })?
                            .ty,
                        expression.span,
                    )?;
                    for arg in args.iter().filter(runtime_argument) {
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
                S::Defer(body) => self.statements(body)?,
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
    defer_depth: usize,
    exit: Option<BlockId>,
    locals: Vec<LocalId>,
    types: Vec<TypeId>,
}
struct Lowerer<'a, 'p, 'ir> {
    source: &'a Source<'p>,
    functions: &'a HashMap<FunctionKey, FunctionId>,
    constants: &'a [Constant],
    builder: FunctionBuilder<'ir>,
    loops: Vec<LoopContext>,
    // One registration list per active lexical block; bodies run at exit.
    defers: Vec<Vec<Vec<Statement>>>,
    slots: BTreeMap<LocalId, Operand>,
}
impl Lowerer<'_, '_, '_> {
    fn value_expression(
        &mut self,
        state: &mut State,
        expr: &Expression,
    ) -> anyhow::Result<Operand> {
        if let Some(value) = self.source.comp_value(expr)? {
            return globals::lower_data(self.source, self.builder.ir_mut(), value, self.functions)
                .map(Operand::Constant);
        }
        if matches!(expr.kind, E::VoidExpression)
            || (matches!(expr.kind, E::Call { .. })
                && self
                    .source
                    .ir_type(
                        expr.annotation.resolved.expect("validated expression").ty,
                        expr.span,
                    )?
                    .is_none())
        {
            if matches!(expr.kind, E::Call { .. }) {
                self.expression_statement(state, expr)?;
            }
            return self.instruction(
                state,
                InstructionKind::Aggregate { elements: vec![] },
                self.source.expression_type(expr)?,
            );
        }
        if let Some(constant) = unit_comparison(self.source, expr)? {
            let E::BinaryExpresison(binary) = &expr.kind else {
                unreachable!()
            };
            self.value_expression(state, &binary.left)?;
            self.value_expression(state, &binary.right)?;
            let index = self
                .constants
                .iter()
                .position(|c| c.ty == constant.ty && c.kind == constant.kind)
                .ok_or_else(|| {
                    self.source
                        .error(expr.span, "missing unit comparison constant")
                })?;
            return Ok(Operand::Constant(ConstantId(index)));
        }
        if let Some(constant) = literal(self.source, expr)? {
            let index = self
                .constants
                .iter()
                .position(|c| c.ty == constant.ty && c.kind == constant.kind)
                .ok_or_else(|| self.source.error(expr.span, "missing prepared constant"))?;
            return Ok(Operand::Constant(ConstantId(index)));
        }
        if let E::Call { callee, args } = &expr.kind
            && let Some(builtin) = self.source.builtin(callee).filter(|b| b.is_intrinsic())
        {
            use crate::builtins::BuiltinFunction as F;
            if builtin == F::Sizeof {
                let ty = args[0]
                    .annotation
                    .resolved
                    .and_then(|i| i.type_value)
                    .ok_or_else(|| {
                        self.source
                            .error(expr.span, "sizeof requires a type argument")
                    })?;
                let ty = self.source.ir_type(ty, expr.span)?.ok_or_else(|| {
                    self.source
                        .error(expr.span, "sizeof requires a sized runtime type")
                })?;
                return self.instruction(
                    state,
                    InstructionKind::Sizeof(ty),
                    self.source.expression_type(expr)?,
                );
            }
            if builtin == F::UncheckedDeref {
                let address = self.value_expression(state, &args[0])?;
                return self.instruction(
                    state,
                    InstructionKind::Load {
                        address: Address(address),
                    },
                    self.source.expression_type(expr)?,
                );
            }
            let values = args
                .iter()
                .filter(runtime_argument)
                .map(|a| self.value_expression(state, a))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let kind = if builtin.syscall_arity().is_some() {
                InstructionKind::Syscall {
                    number: values[0],
                    args: values[1..].to_vec(),
                }
            } else {
                match builtin {
                    F::SliceData => InstructionKind::SliceData(values[0]),
                    F::SliceFromParts => InstructionKind::Slice {
                        data: values[0],
                        length: values[1],
                    },
                    F::PointerFromWord => InstructionKind::Cast {
                        kind: CastKind::IntegerToPointer,
                        operand: values[0],
                    },
                    F::PointerToWord => InstructionKind::Cast {
                        kind: CastKind::PointerToInteger,
                        operand: values[0],
                    },
                    _ => return Err(self.source.error(expr.span, "unsupported intrinsic")),
                }
            };
            return self.instruction(state, kind, self.source.expression_type(expr)?);
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
                .get(self.source.types.canonical(owner)?)?
            else {
                unreachable!()
            };
            let payload = self.source.types.enum_payload(variants[variant].payload)?;
            let mut elements = Vec::new();
            for arg in args {
                elements.push(self.value_expression(state, arg)?);
            }
            let fields = if payload.is_some() { elements } else { vec![] };
            return self.instruction(
                state,
                InstructionKind::Enum { variant, fields },
                self.source.expression_type(expr)?,
            );
        }
        if let Some(constant) = array_length(self.source, expr)? {
            if let E::ExprAcess {
                expr: receiver,
                member,
                ..
            } = &expr.kind
            {
                if member.implicit_deref {
                    self.checked_pointer(state, receiver)?;
                } else {
                    self.value_expression(state, receiver)?;
                }
            }
            let index = self
                .constants
                .iter()
                .position(|c| c.ty == constant.ty && c.kind == constant.kind)
                .ok_or_else(|| {
                    self.source
                        .error(expr.span, "missing array length constant")
                })?;
            return Ok(Operand::Constant(ConstantId(index)));
        }
        if let E::ArrayLiteral { elements } = &expr.kind {
            let elements = elements
                .iter()
                .map(|e| self.value_expression(state, e))
                .collect::<anyhow::Result<Vec<_>>>()?;
            return self.instruction(
                state,
                InstructionKind::Aggregate { elements },
                self.source.expression_type(expr)?,
            );
        }
        if let E::SliceLiteral {
            object,
            start,
            end,
            inclusive,
        } = &expr.kind
        {
            return self.slice(
                state,
                expr,
                object,
                start.as_deref(),
                end.as_deref(),
                *inclusive,
            );
        }
        if let E::IndexAccess { object, index } = &expr.kind {
            let address = self.index_address(state, object, index)?;
            return self.instruction(
                state,
                InstructionKind::Load {
                    address: Address(address),
                },
                self.source.expression_type(expr)?,
            );
        }
        if let Some(function) = callee(expr, &[], self.functions) {
            return self.instruction(
                state,
                InstructionKind::FunctionAddress(function),
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
            let address = self.place(state, &unary.expression)?;
            if self
                .source
                .ir_type(
                    unary
                        .expression
                        .annotation
                        .resolved
                        .expect("validated place")
                        .ty,
                    unary.expression.span,
                )?
                .is_none()
            {
                return self.instruction(
                    state,
                    InstructionKind::Cast {
                        kind: CastKind::Bitcast,
                        operand: address,
                    },
                    self.source.expression_type(expr)?,
                );
            }
            return Ok(address);
        }
        if let E::ExprAcess {
            expr: receiver,
            member,
            ..
        } = &expr.kind
        {
            let value = self.value_expression(state, receiver)?;
            if member
                .resolved
                .is_some_and(|m| m.kind == MemberKind::Length)
            {
                return self.instruction(
                    state,
                    InstructionKind::SliceLength(value),
                    self.source.expression_type(expr)?,
                );
            }
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
                let direct = callee(function, args, self.functions);
                let indirect = if direct.is_none() {
                    Some(self.checked_pointer(state, function)?)
                } else {
                    None
                };
                let args = args
                    .iter()
                    .filter(runtime_argument)
                    .map(|arg| self.value_expression(state, arg))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                match direct {
                    Some(callee) => InstructionKind::Call { callee, args },
                    None => InstructionKind::CallIndirect {
                        callee: indirect.unwrap(),
                        args,
                    },
                }
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
                kind: if matches!(
                    self.source
                        .types
                        .container(expr.annotation.resolved.unwrap().ty)?
                        .0,
                    SemanticType::Pointer(_)
                ) {
                    CastKind::Bitcast
                } else {
                    CastKind::Numeric
                },
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
    fn checked_pointer(&mut self, state: &mut State, expr: &Expression) -> anyhow::Result<Operand> {
        let pointer = self.value_expression(state, expr)?;
        let ty = self.source.expression_type(expr)?;
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
    fn place(&mut self, state: &mut State, expr: &Expression) -> anyhow::Result<Operand> {
        if let Some(id) = globals::global(expr, self.source.globals) {
            return self.instruction(
                state,
                InstructionKind::GlobalAddress(id),
                self.source
                    .pointer_type(self.source.expression_type(expr)?)?,
            );
        }
        if let Some(id) = local(expr) {
            return self
                .slots
                .get(&id)
                .copied()
                .ok_or_else(|| self.source.error(expr.span, "binding has no storage"));
        }
        match &expr.kind {
            E::IndexAccess { object, index } => self.index_address(state, object, index),
            E::ExprAcess {
                expr: base, member, ..
            } => {
                let base = if member.implicit_deref {
                    self.checked_pointer(state, base)?
                } else {
                    self.place(state, base)?
                };
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
                self.checked_pointer(state, &unary.expression)
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
        if self.source.intrinsic(function) {
            self.value_expression(state, expr)?;
            return Ok(());
        }
        let direct = callee(function, args, self.functions);
        let indirect = if direct.is_none() {
            Some(self.checked_pointer(state, function)?)
        } else {
            None
        };
        let args = args
            .iter()
            .filter(runtime_argument)
            .map(|arg| self.value_expression(state, arg))
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.builder.append_instruction(
            state.block,
            match direct {
                Some(callee) => InstructionKind::Call { callee, args },
                None => InstructionKind::CallIndirect {
                    callee: indirect.unwrap(),
                    args,
                },
            },
            &self
                .source
                .ir_type(
                    expr.annotation
                        .resolved
                        .ok_or_else(|| self.source.error(expr.span, "missing call annotation"))?
                        .ty,
                    expr.span,
                )?
                .into_iter()
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
    fn initialize_binding(
        &mut self,
        state: &mut State,
        variable: &crate::frontend::ast::variable_declaration::VariableDeclaration,
        value: Operand,
        span: Span,
    ) -> anyhow::Result<()> {
        let Some(BindingId::Local(id)) = variable.annotation.binding else {
            return Err(self.source.error(span, "missing local identity"));
        };
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
        Ok(())
    }
    fn statements(&mut self, nodes: &[Statement], state: State) -> anyhow::Result<Option<State>> {
        self.defers.push(Vec::new());
        let result = self.statements_inner(nodes, state);
        let result = match result {
            Ok(Some(state)) => self.run_defers(state, self.defers.len() - 1).map(Some),
            other => other,
        };
        self.defers.pop();
        result
    }

    /// Emit only cleanup for scopes being exited. Keep registrations intact so
    /// sibling CFG paths can emit their own exits from the same lexical scopes.
    fn run_defers(&mut self, mut state: State, depth: usize) -> anyhow::Result<State> {
        let bodies = self.defers[depth..]
            .iter()
            .rev()
            .flat_map(|frame| frame.iter().rev().cloned())
            .collect::<Vec<_>>();
        for body in bodies {
            state = self.scope(&body, state)?.ok_or_else(|| {
                self.source
                    .error(Span::new(1, 1), "defer must complete normally")
            })?;
        }
        Ok(state)
    }

    fn statements_inner(
        &mut self,
        nodes: &[Statement],
        state: State,
    ) -> anyhow::Result<Option<State>> {
        let mut current = Some(state);
        for node in nodes {
            let Some(mut state) = current.take() else {
                break;
            };
            match &node.kind {
                S::VariableDeclaration(variable) if variable.is_comp => {}
                S::VariableDeclaration(variable) => {
                    if let Some(expr) = &variable.expression {
                        let value = self.value_expression(&mut state, expr)?;
                        self.initialize_binding(&mut state, variable, value, node.span)?;
                    }
                }
                S::MultiVariableDeclaration { bindings, values } => {
                    let values = values
                        .iter()
                        .zip(bindings)
                        .map(|(value, binding)| {
                            if binding.is_comp {
                                Ok(None)
                            } else {
                                self.value_expression(&mut state, value).map(Some)
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    for (binding, value) in bindings.iter().zip(values) {
                        if let Some(value) = value {
                            self.initialize_binding(&mut state, binding, value, node.span)?;
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
                S::IndexAssign {
                    object,
                    index,
                    expr,
                } => {
                    let value = self.value_expression(&mut state, expr)?;
                    let address = self.index_address(&mut state, object, index)?;
                    self.builder.append_instruction(
                        state.block,
                        InstructionKind::Store {
                            address: Address(address),
                            value,
                        },
                        &[],
                    )?;
                }
                S::MultiAssignment { targets, values } => {
                    let values = values
                        .iter()
                        .map(|e| self.value_expression(&mut state, e))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    let addresses = targets
                        .iter()
                        .map(|e| {
                            if local(e).is_some_and(|id| !self.slots.contains_key(&id)) {
                                Ok(None)
                            } else {
                                self.place(&mut state, e).map(Some)
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    for ((target, address), value) in targets.iter().zip(addresses).zip(values) {
                        if let Some(address) = address {
                            self.builder.append_instruction(
                                state.block,
                                InstructionKind::Store {
                                    address: Address(address),
                                    value,
                                },
                                &[],
                            )?;
                        } else {
                            state.locals.insert(local(target).unwrap(), value);
                        }
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
                S::Defer(body) => {
                    self.defers
                        .last_mut()
                        .expect("active lexical block")
                        .push(body.clone());
                }
                S::Break | S::Continue => {
                    let depth = self
                        .loops
                        .last()
                        .ok_or_else(|| {
                            self.source
                                .error(node.span, "break/continue requires an enclosing loop")
                        })?
                        .defer_depth;
                    state = self.run_defers(state, depth)?;
                    self.loop_jump(state, matches!(node.kind, S::Break), node.span)?;
                    continue;
                }
                S::Return(None) => {
                    state = self.run_defers(state, 0)?;
                    self.builder
                        .set_terminator(state.block, Terminator::Return { values: vec![] })?;
                    continue;
                }
                S::Return(Some(values)) => {
                    let value = self.value_expression(&mut state, &values[0])?;
                    let returns_value = self
                        .source
                        .ir_type(
                            values[0].annotation.resolved.expect("validated return").ty,
                            values[0].span,
                        )?
                        .is_some();
                    state = self.run_defers(state, 0)?;
                    self.builder.set_terminator(
                        state.block,
                        Terminator::Return {
                            values: if returns_value { vec![value] } else { vec![] },
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
            .get(self.source.types.canonical(ty)?)?
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
                let payload_type = self
                    .source
                    .types
                    .enum_payload(variants[index as usize].payload)?;
                let payload = if let Some(payload_type) = payload_type {
                    self.instruction(
                        &branch,
                        InstructionKind::EnumField {
                            value,
                            variant: index as usize,
                            field: 0,
                        },
                        self.source.value_type(payload_type, subject.span)?,
                    )?
                } else {
                    self.instruction(
                        &branch,
                        InstructionKind::Aggregate { elements: vec![] },
                        self.source
                            .value_type(self.source.types.locals[&id], subject.span)?,
                    )?
                };
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
                    .value_type(self.source.types.locals[id], subject.span)
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
                        self.source.types.locals.get(id).ok_or_else(|| {
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
            defer_depth: self.defers.len(),
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
                        let ty = self.source.types.locals.get(id).ok_or_else(|| {
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
            .types
            .locals
            .get(&id)
            .ok_or_else(|| self.error(Span::new(1, 1), "missing local semantic type"))?;
        Ok(matches!(
            self.types.container(*ty)?.0,
            SemanticType::Struct(_) | SemanticType::Enum(_) | SemanticType::Array { .. }
        ))
    }
}
