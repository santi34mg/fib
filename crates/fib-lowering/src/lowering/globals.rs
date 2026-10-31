//! Constant global data and declaration-address relocation lowering.
use super::*;
use crate::frontend::analyzer::constant_data::{
    DataAddress, DataEvaluator, DataKind, DataProjection, DataTarget, DataValue, binding,
    require_initializer,
};

pub(super) fn global(expr: &Expression, globals: &HashMap<DeclId, GlobalId>) -> Option<GlobalId> {
    let BindingId::Declaration(id) = binding(expr)? else {
        return None;
    };
    globals.get(&id).copied()
}

pub(super) fn initialize(
    project: &AnalyzedProject,
    ir: &mut Ir,
    runtime_types: &HashMap<SemanticId, TypeId>,
    pointers: &HashMap<TypeId, TypeId>,
    globals: &HashMap<DeclId, GlobalId>,
    functions: &HashMap<FunctionKey, FunctionId>,
) -> anyhow::Result<()> {
    let mut evaluator = DataEvaluator::new(&project.units, &project.types);
    for unit in &project.units {
        evaluator.set_file(unit.source_file_id);
        let source = Source {
            project,
            types: &project.types,
            file: unit.source_file_id,
            runtime_types,
            pointers,
            globals,
        };
        for node in &unit.ast.statements {
            let bindings: Vec<_> = match &node.kind {
                S::VariableDeclaration(v) => vec![(v, v.expression.as_ref())],
                S::MultiVariableDeclaration { bindings, values } => bindings
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (v, values.get(i)))
                    .collect(),
                _ => vec![],
            };
            for (binding, value) in bindings.into_iter().filter(|(v, _)| !v.is_comp) {
                let Some(BindingId::Declaration(id)) = binding.annotation.binding else {
                    unreachable!()
                };
                let value = evaluator.evaluate(require_initializer(value, node.span)?)?;
                let constant = lower_data(&source, ir, value, functions)?;
                ir.globals[globals[&id].0].initializer = GlobalInitializer::Constant(constant);
            }
        }
    }
    Ok(())
}

fn intern(ir: &mut Ir, ty: TypeId, kind: ConstantKind) -> ConstantId {
    let index = ir
        .constants
        .iter()
        .position(|c| c.ty == ty && c.kind == kind)
        .unwrap_or_else(|| {
            let index = ir.constants.len();
            ir.constants.push(Constant { ty, kind });
            index
        });
    ConstantId(index)
}

pub(super) fn lower_data(
    source: &Source<'_>,
    ir: &mut Ir,
    value: DataValue,
    functions: &HashMap<FunctionKey, FunctionId>,
) -> anyhow::Result<ConstantId> {
    let ty = source.value_type(value.ty, Span::new(1, 1))?;
    let kind = match value.kind {
        DataKind::Numeric(n) => match n.ty {
            crate::numeric::NumericType::Integer { .. } => ConstantKind::Integer { bits: n.bits },
            crate::numeric::NumericType::Float(_) => ConstantKind::Float { bits: n.bits },
        },
        DataKind::Bool(b) => ConstantKind::Bool(b),
        DataKind::Null => ConstantKind::Null,
        DataKind::Bytes(mut b) => {
            if let Some(sentinel) = source.types.sentinel(value.ty)? {
                b.push(sentinel);
            }
            ConstantKind::Bytes(b)
        }
        DataKind::CString(b) => ConstantKind::CString(b),
        DataKind::Aggregate(elements) => {
            let mut elements = elements
                .into_iter()
                .map(|v| lower_data(source, ir, v, functions))
                .collect::<anyhow::Result<Vec<_>>>()?;
            if let Some(sentinel) = source.types.sentinel(value.ty)? {
                elements.push(intern(
                    ir,
                    scalar_id(&BuiltinType::UInt8),
                    ConstantKind::Integer {
                        bits: u128::from(sentinel),
                    },
                ));
            }
            ConstantKind::Aggregate(elements)
        }
        DataKind::Enum { variant, fields } => ConstantKind::Enum {
            variant,
            fields: fields
                .into_iter()
                .map(|v| lower_data(source, ir, v, functions))
                .collect::<anyhow::Result<_>>()?,
        },
        DataKind::Function(id) => ConstantKind::FunctionAddress(
            *functions
                .get(&FunctionKey::Declaration(id))
                .ok_or_else(|| {
                    source.error(Span::new(1, 1), "missing constant function identity")
                })?,
        ),
        DataKind::Address(address) => {
            let id = lower_address(source, ir, address, functions)?;
            if ir.constants[id.0].ty == ty {
                return Ok(id);
            }
            ConstantKind::PointerCast(id)
        }
        DataKind::Slice { address, length } => ConstantKind::Slice {
            data: lower_address(source, ir, address, functions)?,
            length,
        },
    };
    Ok(intern(ir, ty, kind))
}

fn lower_address(
    source: &Source<'_>,
    ir: &mut Ir,
    address: DataAddress,
    functions: &HashMap<FunctionKey, FunctionId>,
) -> anyhow::Result<ConstantId> {
    let (global, mut ty) = match address.target {
        DataTarget::Global(id, ty) => {
            (source.globals[&id], source.value_type(ty, Span::new(1, 1))?)
        }
        DataTarget::Anonymous { file, span, value } => {
            let identity = linkage::type_key(source.project, value.ty, &mut Vec::new())?;
            let initializer = lower_data(source, ir, *value, functions)?;
            let ty = ir.constants[initializer.0].ty;
            let name = format!(
                "fib.v1.literal.{}.{}.{}.{}",
                linkage::module(source.project, file)?,
                span.line,
                span.column,
                identity
            );
            if let Some(index) = ir.globals.iter().position(|g| g.name == name) {
                let global = &ir.globals[index];
                anyhow::ensure!(
                    global.ty == ty
                        && global.initializer == GlobalInitializer::Constant(initializer),
                    "inconsistent constant backing storage"
                );
                (GlobalId(index), ty)
            } else {
                let id = GlobalId(ir.globals.len());
                ir.globals.push(Global {
                    name,
                    ty,
                    mutable: true,
                    linkage: Linkage::Internal,
                    initializer: GlobalInitializer::Constant(initializer),
                });
                (id, ty)
            }
        }
    };
    let mut value = intern(
        ir,
        source.pointer_type(ty)?,
        ConstantKind::GlobalAddress(global),
    );
    for projection in address.projections {
        let kind = match projection {
            DataProjection::Field(field) => {
                let IrType::Struct { fields, .. } = &ir.types[ty.0] else {
                    anyhow::bail!("constant field requires struct");
                };
                ty = fields[field];
                ConstantKind::FieldAddress { base: value, field }
            }
            DataProjection::ArrayElement(index) => {
                let IrType::Array { element, .. } = ir.types[ty.0] else {
                    anyhow::bail!("constant array projection requires array");
                };
                ty = element;
                value = intern(ir, source.pointer_type(ty)?, ConstantKind::ArrayData(value));
                ConstantKind::ElementAddress { base: value, index }
            }
            DataProjection::Offset(index) => ConstantKind::ElementAddress { base: value, index },
        };
        value = intern(ir, source.pointer_type(ty)?, kind);
    }
    Ok(value)
}
