//! Target-independent constant data. Addresses name storage, never read it.
//! Value-producing comp function execution deliberately remains unimplemented.
use super::{at_span, constant::numeric_type};
use crate::{
    CompilationUnit,
    diagnostics::Span,
    frontend::{
        analysis_annotation::{BindingId, MemberKind},
        ast::{
            expression::{Expression, ExpressionKind as E},
            statement::{Statement, StatementKind as S},
        },
        identifier::{DeclId, Entity, ResolvedName},
        semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
        tokens::{Literal, Operator as O, builtin::BuiltinType as B},
    },
    numeric::{NumericType, NumericValue},
    plan::FileId,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct DataValue {
    pub ty: TypeId,
    pub kind: DataKind,
}
#[derive(Clone, Debug)]
pub enum DataKind {
    Numeric(NumericValue),
    Bool(bool),
    Null,
    Bytes(Vec<u8>),
    CString(Vec<u8>),
    Aggregate(Vec<DataValue>),
    Enum {
        variant: usize,
        fields: Vec<DataValue>,
    },
    Address(DataAddress),
    Function(DeclId),
    Slice {
        address: DataAddress,
        length: u64,
    },
}
#[derive(Clone, Debug)]
pub struct DataAddress {
    pub target: DataTarget,
    pub projections: Vec<DataProjection>,
}
#[derive(Clone, Debug)]
pub enum DataTarget {
    Global(DeclId, TypeId),
    Anonymous {
        file: FileId,
        span: Span,
        value: Box<DataValue>,
    },
}
#[derive(Clone, Debug)]
pub enum DataProjection {
    Field(usize),
    ArrayElement(u64),
    Offset(u64),
}

/// Distinguishes unsupported execution from expressions that require runtime state.
#[derive(Debug)]
pub enum EvaluationError {
    NonConstant,
    NotImplemented,
}
impl std::fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonConstant => "global initializer requires constant data; runtime calls and runtime binding reads are forbidden",
            Self::NotImplemented => "value-returning comp function evaluation is not implemented",
        })
    }
}
impl std::error::Error for EvaluationError {}

pub struct DataEvaluator<'a> {
    types: &'a SemanticTypes,
    definitions: HashMap<BindingId, (bool, Option<Expression>, FileId)>,
    file: FileId,
    active: HashSet<BindingId>,
    depth: usize,
}
pub fn binding(expr: &Expression) -> Option<BindingId> {
    let E::QualifiedIdentifier(name) = &expr.kind else {
        return None;
    };
    match name.annotation.resolved? {
        ResolvedName::Local(id) => Some(BindingId::Local(id)),
        ResolvedName::Entity(Entity::Declaration(id)) => Some(BindingId::Declaration(id)),
        _ => None,
    }
}
impl<'a> DataEvaluator<'a> {
    pub fn is_value_binding(&self, expr: &Expression) -> bool {
        binding(expr).is_some_and(|id| self.definitions.get(&id).is_some_and(|(comp, _, _)| *comp))
            && expr.annotation.resolved.is_some_and(|i| {
                i.type_value.is_none() && !matches!(self.types.get(i.ty), Ok(T::Function(_)))
            })
    }
    pub fn new(units: &[CompilationUnit], types: &'a SemanticTypes) -> Self {
        fn collect(
            nodes: &[Statement],
            definitions: &mut HashMap<BindingId, (bool, Option<Expression>, FileId)>,
            file: FileId,
        ) {
            for node in nodes {
                match &node.kind {
                    S::VariableDeclaration(v) => {
                        if let Some(id) = v.annotation.binding {
                            definitions.insert(id, (v.is_comp, v.expression.clone(), file));
                        }
                    }
                    S::MultiVariableDeclaration { bindings, values } => {
                        for (i, v) in bindings.iter().enumerate() {
                            if let Some(id) = v.annotation.binding {
                                definitions.insert(id, (v.is_comp, values.get(i).cloned(), file));
                            }
                        }
                    }
                    S::FunctionDeclaration(f) => {
                        if let Some(body) = &f.body {
                            collect(&body.statements, definitions, file);
                        }
                    }
                    S::If {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        collect(then_branch, definitions, file);
                        if let Some(body) = else_branch {
                            collect(body, definitions, file);
                        }
                    }
                    S::While { body, .. } | S::Defer(body) => collect(body, definitions, file),
                    S::Switch { arms, .. } => {
                        for arm in arms {
                            collect(&arm.body, definitions, file);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut definitions = HashMap::new();
        for unit in units {
            collect(&unit.ast.statements, &mut definitions, unit.source_file_id);
        }
        Self {
            types,
            definitions,
            file: FileId(0),
            active: HashSet::new(),
            depth: 0,
        }
    }

    pub fn set_file(&mut self, file: FileId) {
        self.file = file;
    }

    pub fn evaluate(&mut self, expr: &Expression) -> anyhow::Result<DataValue> {
        self.check_calls(expr)?;
        self.compute(expr).map_err(|e| Self::located(e, expr.span))
    }

    pub fn try_evaluate(&mut self, expr: &Expression) -> anyhow::Result<Option<DataValue>> {
        match self.compute(expr) {
            Ok(value) => Ok(Some(value)),
            Err(error)
                if matches!(
                    error.downcast_ref::<EvaluationError>(),
                    Some(EvaluationError::NonConstant)
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(Self::located(error, expr.span)),
        }
    }

    fn located(error: anyhow::Error, span: Span) -> anyhow::Error {
        if error.is::<crate::diagnostics::CompilerError>() {
            at_span(error, span)
        } else {
            at_span(type_error(error.to_string()), span)
        }
    }

    fn compute(&mut self, expr: &Expression) -> anyhow::Result<DataValue> {
        if self.depth >= 128 {
            return Err(at_span(
                type_error("constant data evaluation depth limit exceeded"),
                expr.span,
            ));
        }
        self.depth += 1;
        let result = self.inner(expr);
        self.depth -= 1;
        result.map_err(|e| {
            if e.is::<EvaluationError>() {
                e
            } else {
                Self::located(e, expr.span)
            }
        })
    }

    fn check_calls(&self, expr: &Expression) -> anyhow::Result<()> {
        match &expr.kind {
            E::Call { callee, args } => {
                let variant = matches!(&callee.kind, E::ExprAcess { member, .. } if matches!(member.resolved.map(|m| m.kind), Some(MemberKind::EnumVariant(_))));
                let comp = callee.annotation.resolved.is_some_and(
                    |i| matches!(self.types.get(i.ty), Ok(T::Function(f)) if f.is_comp),
                );
                if !variant && !comp {
                    return Err(at_span(
                        type_error(EvaluationError::NonConstant.to_string()),
                        expr.span,
                    ));
                }
                for arg in args {
                    self.check_calls(arg)?;
                }
            }
            E::UnaryExpression(u) => self.check_calls(&u.expression)?,
            E::BinaryExpresison(b) => {
                self.check_calls(&b.left)?;
                self.check_calls(&b.right)?;
            }
            E::Cast { expr, .. } | E::ExprAcess { expr, .. } => self.check_calls(expr)?,
            E::IndexAccess { object, index } => {
                self.check_calls(object)?;
                self.check_calls(index)?;
            }
            E::SliceLiteral {
                object, start, end, ..
            } => {
                self.check_calls(object)?;
                for e in start.iter().chain(end) {
                    self.check_calls(e)?;
                }
            }
            E::ArrayLiteral { elements } => {
                for e in elements {
                    self.check_calls(e)?;
                }
            }
            E::StructConstruct { fields, .. } => {
                for f in fields {
                    self.check_calls(&f.expression)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn inner(&mut self, expr: &Expression) -> anyhow::Result<DataValue> {
        let ty = expr
            .annotation
            .resolved
            .ok_or_else(|| type_error("missing constant expression annotation"))?
            .ty;
        if let Some(value) = super::constant::evaluate_numeric(expr, self.types)? {
            return Ok(DataValue {
                ty,
                kind: DataKind::Numeric(value),
            });
        }
        let kind = match &expr.kind {
            E::VoidExpression => DataKind::Aggregate(vec![]),
            E::Literal(Literal::Boolean(value)) => DataKind::Bool(*value),
            E::Literal(Literal::Null) => DataKind::Null,
            E::Literal(Literal::String(value)) => DataKind::Bytes(value.as_bytes().to_vec()),
            E::Literal(Literal::CString(value)) => DataKind::CString(value.as_bytes().to_vec()),
            E::QualifiedIdentifier(_) => {
                let id = binding(expr).ok_or(EvaluationError::NonConstant)?;
                if let BindingId::Declaration(id) = id
                    && !self.definitions.contains_key(&BindingId::Declaration(id))
                    && matches!(self.types.get(self.types.canonical(ty)?)?, T::Function(f) if !f.is_comp)
                {
                    return Ok(DataValue {
                        ty,
                        kind: DataKind::Function(id),
                    });
                }
                let (comp, value, file) = self
                    .definitions
                    .get(&id)
                    .cloned()
                    .ok_or(EvaluationError::NonConstant)?;
                if !comp {
                    return Err(EvaluationError::NonConstant.into());
                }
                if !self.active.insert(id) {
                    return Err(type_error("constant value dependency cycle"));
                }
                let previous_file = std::mem::replace(&mut self.file, file);
                let result = value
                    .as_ref()
                    .ok_or_else(|| type_error("constant binding requires an initializer"))
                    .and_then(|value| self.compute(value));
                self.active.remove(&id);
                self.file = previous_file;
                return result.map(|value| DataValue { ty, ..value });
            }
            E::ArrayLiteral { elements } => DataKind::Aggregate(
                elements
                    .iter()
                    .map(|e| self.compute(e))
                    .collect::<anyhow::Result<_>>()?,
            ),
            E::StructConstruct { fields, .. } => {
                let mut elements = vec![None; fields.len()];
                for field in fields {
                    let Some(MemberKind::Field(index)) = field.annotation.resolved.map(|m| m.kind)
                    else {
                        return Err(type_error("missing constant field identity"));
                    };
                    elements[index as usize] = Some(self.compute(&field.expression)?);
                }
                DataKind::Aggregate(
                    elements
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| type_error("incomplete constant struct"))?,
                )
            }
            E::UnaryExpression(unary) if unary.operator == O::AddressOf => {
                DataKind::Address(self.address(&unary.expression)?)
            }
            E::UnaryExpression(unary) => match self.compute(&unary.expression)?.kind {
                DataKind::Numeric(value) => DataKind::Numeric(value.unary(unary.operator)?),
                DataKind::Bool(value) if unary.operator == O::LogicalNot => DataKind::Bool(!value),
                _ => return Err(EvaluationError::NonConstant.into()),
            },
            E::BinaryExpresison(binary) => {
                let left = self.compute(&binary.left)?;
                if matches!(
                    (&left.kind, binary.operator),
                    (DataKind::Bool(false), O::LogicalAnd) | (DataKind::Bool(true), O::LogicalOr)
                ) {
                    return Ok(DataValue { ty, ..left });
                }
                let right = self.compute(&binary.right)?;
                match (left.kind, right.kind) {
                    (DataKind::Numeric(a), DataKind::Numeric(b)) => {
                        if matches!(
                            self.types.get(self.types.canonical(ty)?)?,
                            T::Builtin(B::Boolean)
                        ) {
                            DataKind::Bool(a.compare(b, binary.operator)?)
                        } else {
                            DataKind::Numeric(a.binary(b, binary.operator)?)
                        }
                    }
                    (DataKind::Bool(a), DataKind::Bool(b)) => {
                        DataKind::Bool(match binary.operator {
                            O::LogicalAnd => a && b,
                            O::LogicalOr => a || b,
                            O::DoubleEquals => a == b,
                            O::Different => a != b,
                            _ => return Err(EvaluationError::NonConstant.into()),
                        })
                    }
                    (DataKind::Null, DataKind::Null) => {
                        DataKind::Bool(binary.operator == O::DoubleEquals)
                    }
                    _ => return Err(EvaluationError::NonConstant.into()),
                }
            }
            E::Cast { expr: value, .. } => {
                let value = self.compute(value)?;
                if let Some(numeric) = numeric_type(ty, self.types) {
                    let value = match value.kind {
                        DataKind::Numeric(value) => value,
                        DataKind::Bool(value) => NumericValue {
                            ty: NumericType::Integer {
                                width: 8,
                                signed: false,
                            },
                            bits: u128::from(value),
                        },
                        _ => return Err(EvaluationError::NonConstant.into()),
                    };
                    DataKind::Numeric(value.cast(numeric)?)
                } else if matches!(
                    self.types.get(self.types.canonical(ty)?)?,
                    T::Builtin(B::Boolean)
                ) {
                    match value.kind {
                        DataKind::Numeric(n) => DataKind::Bool(!n.is_zero()),
                        DataKind::Bool(b) => DataKind::Bool(b),
                        _ => return Err(EvaluationError::NonConstant.into()),
                    }
                } else {
                    value.kind
                }
            }
            E::ExprAcess {
                expr: object,
                member,
                ..
            } => match member
                .resolved
                .ok_or_else(|| type_error("missing constant member"))?
                .kind
            {
                MemberKind::Function(id) => DataKind::Function(id),
                MemberKind::Length => {
                    let length = match self
                        .types
                        .container(object.annotation.resolved.unwrap().ty)?
                        .0
                    {
                        T::Array { length, .. } => {
                            self.array_address(object)?;
                            *length
                        }
                        T::Slice(_) => match self.compute(object)?.kind {
                            DataKind::Slice { length, .. } => length,
                            _ => return Err(EvaluationError::NonConstant.into()),
                        },
                        _ => return Err(EvaluationError::NonConstant.into()),
                    };
                    DataKind::Numeric(NumericValue {
                        ty: numeric_type(ty, self.types).unwrap(),
                        bits: u128::from(length),
                    })
                }
                MemberKind::Field(index) if !member.implicit_deref => {
                    match self.compute(object)?.kind {
                        DataKind::Aggregate(elements) => elements
                            .get(index as usize)
                            .ok_or_else(|| type_error("constant field out of bounds"))?
                            .kind
                            .clone(),
                        _ => return Err(EvaluationError::NonConstant.into()),
                    }
                }
                MemberKind::EnumVariant(variant) => DataKind::Enum {
                    variant: variant as usize,
                    fields: vec![],
                },
                _ => return Err(EvaluationError::NonConstant.into()),
            },
            E::Call { callee, args } => {
                if let E::ExprAcess { member, .. } = &callee.kind
                    && let Some(MemberKind::EnumVariant(variant)) = member.resolved.map(|m| m.kind)
                {
                    DataKind::Enum {
                        variant: variant as usize,
                        fields: args
                            .iter()
                            .map(|e| self.compute(e))
                            .collect::<anyhow::Result<_>>()?,
                    }
                } else {
                    let function = callee
                        .annotation
                        .resolved
                        .ok_or(EvaluationError::NonConstant)?;
                    if matches!(self.types.get(self.types.canonical(function.ty)?)?, T::Function(f) if f.is_comp)
                    {
                        return Err(EvaluationError::NotImplemented.into());
                    }
                    return Err(EvaluationError::NonConstant.into());
                }
            }
            E::IndexAccess { object, index } => {
                let value = self.compute(object)?;
                let index = self.bound(index)?;
                match value.kind {
                    DataKind::Aggregate(elements) => elements
                        .get(usize::try_from(index)?)
                        .ok_or_else(|| type_error("constant index out of bounds"))?
                        .kind
                        .clone(),
                    DataKind::Bytes(bytes) => {
                        let byte = *bytes
                            .get(usize::try_from(index)?)
                            .ok_or_else(|| type_error("constant index out of bounds"))?;
                        DataKind::Numeric(NumericValue {
                            ty: numeric_type(ty, self.types).unwrap(),
                            bits: u128::from(byte),
                        })
                    }
                    _ => return Err(EvaluationError::NonConstant.into()),
                }
            }
            E::SliceLiteral {
                object,
                start,
                end,
                inclusive,
            } => {
                let (mut address, length, array) = match self
                    .types
                    .container(object.annotation.resolved.unwrap().ty)?
                    .0
                {
                    T::Array { length, .. } => (self.array_address(object)?, *length, true),
                    T::Slice(_) => match self.compute(object)?.kind {
                        DataKind::Slice { address, length } => (address, length, false),
                        _ => return Err(EvaluationError::NonConstant.into()),
                    },
                    _ => return Err(EvaluationError::NonConstant.into()),
                };
                let start = start
                    .as_ref()
                    .map(|e| self.bound(e))
                    .transpose()?
                    .unwrap_or(0);
                let end = if let Some(end) = end {
                    let value = self.bound(end)?;
                    if *inclusive {
                        if value >= length {
                            return Err(type_error("constant slice bounds out of bounds"));
                        }
                        value + 1
                    } else {
                        value
                    }
                } else {
                    length
                };
                if start > end || end > length {
                    return Err(type_error(
                        "constant slice bounds out of bounds or reversed",
                    ));
                }
                address.projections.push(if array {
                    DataProjection::ArrayElement(start)
                } else {
                    DataProjection::Offset(start)
                });
                DataKind::Slice {
                    address,
                    length: end - start,
                }
            }
            _ => return Err(EvaluationError::NonConstant.into()),
        };
        Ok(DataValue { ty, kind })
    }

    fn bound(&mut self, expr: &Expression) -> anyhow::Result<u64> {
        let DataKind::Numeric(n) = self.compute(expr)?.kind else {
            return Err(type_error("constant bound requires an integer"));
        };
        if matches!(n.ty, NumericType::Integer { width, signed: true } if crate::numeric::signed(n.bits, width) < 0)
        {
            return Err(type_error("constant bound out of bounds"));
        }
        u64::try_from(n.bits).map_err(|_| type_error("constant bound out of bounds"))
    }

    fn array_address(&mut self, expr: &Expression) -> anyhow::Result<DataAddress> {
        if let Ok(address) = self.address(expr) {
            return Ok(address);
        }
        Ok(DataAddress {
            target: DataTarget::Anonymous {
                file: self.file,
                span: expr.span,
                value: Box::new(self.compute(expr)?),
            },
            projections: vec![],
        })
    }

    fn address(&mut self, expr: &Expression) -> anyhow::Result<DataAddress> {
        if let Some(BindingId::Declaration(id)) = binding(expr)
            && self
                .definitions
                .get(&BindingId::Declaration(id))
                .is_some_and(|(comp, _, _)| !comp)
        {
            return Ok(DataAddress {
                target: DataTarget::Global(id, expr.annotation.resolved.unwrap().ty),
                projections: vec![],
            });
        }
        match &expr.kind {
            E::ExprAcess {
                expr: object,
                member,
                ..
            } if !member.implicit_deref => {
                let mut address = self.address(object)?;
                let Some(MemberKind::Field(index)) = member.resolved.map(|m| m.kind) else {
                    return Err(EvaluationError::NonConstant.into());
                };
                address
                    .projections
                    .push(DataProjection::Field(index as usize));
                Ok(address)
            }
            E::IndexAccess { object, index } => {
                let T::Array { length, .. } = self
                    .types
                    .container(object.annotation.resolved.unwrap().ty)?
                    .0
                else {
                    return Err(EvaluationError::NonConstant.into());
                };
                let length = *length;
                let mut address = self.array_address(object)?;
                let index = self.bound(index)?;
                if index >= length {
                    return Err(type_error("constant index out of bounds"));
                }
                address
                    .projections
                    .push(DataProjection::ArrayElement(index));
                Ok(address)
            }
            _ => Err(EvaluationError::NonConstant.into()),
        }
    }
}

pub fn require_initializer(value: Option<&Expression>, span: Span) -> anyhow::Result<&Expression> {
    value.ok_or_else(|| {
        at_span(
            type_error("global binding requires an explicit initializer"),
            span,
        )
    })
}
