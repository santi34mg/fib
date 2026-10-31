//! Definite initialization and structured flow over the annotated source AST.
//! Each lexical block executes its defers, in reverse order, on every exit.
use super::{AnalyzedProject, at_span};
use crate::frontend::{
    analysis_annotation::{BindingId, MemberKind, ValueCategory},
    ast::{
        expression::{Expression, ExpressionKind as E},
        pattern::Pattern,
        statement::{Statement, StatementKind as S},
        variable_declaration::VariableDeclaration,
    },
    identifier::{Entity, ResolvedName},
    semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
    tokens::{Operator as O, builtin::BuiltinType as B},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Projection {
    Field(u32),
    Index(u64),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Place {
    binding: BindingId,
    path: Vec<Projection>,
}
type State = HashSet<Place>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExitKind {
    Next,
    Return,
    Break,
    Continue,
    Diverge,
}
struct Exit {
    kind: ExitKind,
    state: State,
}
struct Flow<'a> {
    types: &'a SemanticTypes,
    constant_data: Option<super::constant_data::DataEvaluator<'a>>,
    loops: usize,
    template: bool,
    recovery: Option<super::Recovery>,
    file: crate::plan::FileId,
    paths: HashMap<crate::plan::FileId, std::path::PathBuf>,
}

pub(super) fn validate(
    project: &AnalyzedProject,
    paths: Option<&HashMap<crate::plan::FileId, std::path::PathBuf>>,
) -> anyhow::Result<()> {
    validate_parts(&project.units, &project.types, paths, None)
}

pub(super) fn validate_collecting(
    units: &[crate::CompilationUnit],
    types: &SemanticTypes,
    paths: &HashMap<crate::plan::FileId, std::path::PathBuf>,
    recovery: &super::Recovery,
) -> anyhow::Result<()> {
    validate_parts(units, types, Some(paths), Some(recovery.clone()))
}

fn validate_parts(
    units: &[crate::CompilationUnit],
    types: &SemanticTypes,
    paths: Option<&HashMap<crate::plan::FileId, std::path::PathBuf>>,
    recovery: Option<super::Recovery>,
) -> anyhow::Result<()> {
    let mut flow = Flow {
        types,
        constant_data: Some(super::constant_data::DataEvaluator::new(units, types)),
        loops: 0,
        template: false,
        recovery: None,
        file: crate::plan::FileId(0),
        paths: HashMap::new(),
    };
    flow.recovery = recovery;
    flow.paths = paths.cloned().unwrap_or_default();
    let mut globals = State::new();
    for unit in units {
        for node in &unit.ast.statements {
            if flow
                .recovery
                .as_ref()
                .is_some_and(|r| r.statement_failed(unit.source_file_id, node))
            {
                continue;
            }
            match &node.kind {
                S::VariableDeclaration(v) if v.expression.is_some() || v.is_comp => {
                    flow.initialize(v, &mut globals)?;
                }
                S::MultiVariableDeclaration { bindings, values } if !values.is_empty() => {
                    for v in bindings {
                        flow.initialize(v, &mut globals)?;
                    }
                }
                _ => {}
            }
        }
    }
    let phases = if flow.recovery.is_some() {
        vec![Some(false), Some(true)]
    } else {
        vec![None]
    };
    for phase in phases {
        for unit in units {
            flow.file = unit.source_file_id;
            flow.constant_data
                .as_mut()
                .unwrap()
                .set_file(unit.source_file_id);
            for node in &unit.ast.statements {
                if phase.is_some_and(|functions| {
                    matches!(node.kind, S::FunctionDeclaration(_)) != functions
                }) {
                    continue;
                }
                if flow
                    .recovery
                    .as_ref()
                    .is_some_and(|r| r.statement_failed(unit.source_file_id, node))
                {
                    continue;
                }
                let result = (|| {
                    if let S::FunctionDeclaration(fun) = &node.kind {
                        if let Some(body) = &fun.body {
                            let mut state = globals.clone();
                            for param in &fun.signature.parameters {
                                state.insert(root(
                                    param
                                        .annotation
                                        .binding
                                        .ok_or_else(|| type_error("pending parameter"))?,
                                ));
                            }
                            let previous_template =
                                std::mem::replace(&mut flow.template, is_template(fun, flow.types));
                            let result = flow.block(&body.statements, state);
                            flow.template = previous_template;
                            let exits = result?;
                            if flow
                                .recovery
                                .as_ref()
                                .is_some_and(|r| r.region_failed(flow.file, &body.statements))
                            {
                                return Ok(());
                            }
                            let ret = fun
                                .signature
                                .return_type
                                .annotation
                                .resolved
                                .ok_or_else(|| type_error("pending return type"))?;
                            if matches!(flow.types.get(ret)?, T::Builtin(B::Never))
                                && exits
                                    .iter()
                                    .any(|e| matches!(e.kind, ExitKind::Next | ExitKind::Return))
                            {
                                return Err(type_error("never function cannot return normally"));
                            }
                            if !(is_template(fun, flow.types) && flow.types.is_symbolic(ret)?)
                                && !matches!(flow.types.get(ret)?, T::Builtin(B::Void))
                                && exits.iter().any(|e| e.kind == ExitKind::Next)
                            {
                                return Err(type_error(
                                    "function does not return a value on all paths",
                                ));
                            }
                        }
                    } else {
                        use super::constant_data::require_initializer;
                        match &node.kind {
                            S::VariableDeclaration(v) if !v.is_comp => {
                                flow.constant_data.as_mut().unwrap().evaluate(
                                    require_initializer(v.expression.as_ref(), node.span)?,
                                )?;
                            }
                            S::VariableDeclaration(v)
                                if v.expression.as_ref().is_some_and(|e| {
                                    e.annotation
                                        .resolved
                                        .is_some_and(|i| i.type_value.is_none())
                                        && !matches!(e.kind, E::QualifiedIdentifier(_))
                                }) =>
                            {
                                flow.constant_data
                                    .as_mut()
                                    .unwrap()
                                    .evaluate(v.expression.as_ref().unwrap())?;
                            }
                            S::MultiVariableDeclaration { bindings, values } => {
                                for (i, binding) in bindings.iter().enumerate() {
                                    if !binding.is_comp
                                        || values.get(i).is_some_and(|e| {
                                            e.annotation
                                                .resolved
                                                .is_some_and(|i| i.type_value.is_none())
                                                && !matches!(e.kind, E::QualifiedIdentifier(_))
                                        })
                                    {
                                        flow.constant_data.as_mut().unwrap().evaluate(
                                            require_initializer(values.get(i), node.span)?,
                                        )?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                })();
                let result = result.map_err(|e| {
                    super::with_path(
                        at_span(e, node.span),
                        paths.and_then(|paths| paths.get(&unit.source_file_id)),
                    )
                });
                if let Err(error) = result {
                    if let Some(recovery) = &flow.recovery {
                        recovery.record(error, flow.file, node.span, flow.paths.get(&flow.file));
                        recovery.block_statement(node);
                    } else {
                        return Err(error);
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate_specialization(
    fun: &crate::frontend::ast::function_declaration::FunctionDeclaration,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    let Some(body) = &fun.body else {
        return Ok(());
    };
    let mut flow = Flow {
        types,
        constant_data: None,
        loops: 0,
        template: false,
        recovery: None,
        file: crate::plan::FileId(0),
        paths: HashMap::new(),
    };
    let mut state: State = types
        .declarations
        .keys()
        .map(|id| root(BindingId::Declaration(*id)))
        .collect();
    for p in &fun.signature.parameters {
        state.insert(root(
            p.annotation
                .binding
                .ok_or_else(|| type_error("pending instance parameter"))?,
        ));
    }
    let exits = flow.block(&body.statements, state)?;
    let ret = fun
        .signature
        .return_type
        .annotation
        .resolved
        .ok_or_else(|| type_error("pending instance return type"))?;
    if matches!(types.get(ret)?, T::Builtin(B::Never))
        && exits
            .iter()
            .any(|e| matches!(e.kind, ExitKind::Next | ExitKind::Return))
    {
        return Err(type_error("never function cannot return normally"));
    }
    if !matches!(types.get(ret)?, T::Builtin(B::Void))
        && exits.iter().any(|e| e.kind == ExitKind::Next)
    {
        return Err(type_error("function does not return a value on all paths"));
    }
    Ok(())
}

fn is_template(
    fun: &crate::frontend::ast::function_declaration::FunctionDeclaration,
    types: &SemanticTypes,
) -> bool {
    let Some(BindingId::Declaration(id)) = fun.signature.annotation.binding else {
        return false;
    };
    types
        .generic_parameters
        .get(&id)
        .is_some_and(|parameters| !parameters.is_empty())
}

fn root(binding: BindingId) -> Place {
    Place {
        binding,
        path: vec![],
    }
}
fn binding(expr: &Expression) -> Option<BindingId> {
    if let E::QualifiedIdentifier(n) = &expr.kind {
        match n.annotation.resolved? {
            ResolvedName::Local(id) => Some(BindingId::Local(id)),
            ResolvedName::Entity(Entity::Declaration(id)) => Some(BindingId::Declaration(id)),
            _ => None,
        }
    } else {
        None
    }
}
fn place(expr: &Expression, types: &SemanticTypes) -> anyhow::Result<Option<Place>> {
    if let Some(id) = binding(expr) {
        return Ok(Some(root(id)));
    }
    match &expr.kind {
        E::ExprAcess { expr, member, .. } => {
            if member.implicit_deref {
                return Ok(None);
            }
            if let Some(mut p) = place(expr, types)?
                && let Some(info) = member.resolved
                && let MemberKind::Field(index) = info.kind
            {
                p.path.push(Projection::Field(index));
                return Ok(Some(p));
            }
        }
        E::IndexAccess { object, index } => {
            if matches!(
                object
                    .annotation
                    .resolved
                    .and_then(|i| types.container(i.ty).ok()),
                Some((T::Array { .. }, _))
            ) && let Some(mut p) = place(object, types)?
                && let Some(index) = super::constant::integer_as_i128(index, types)?
                && let Ok(index) = u64::try_from(index)
            {
                p.path.push(Projection::Index(index));
                return Ok(Some(p));
            }
        }
        _ => {}
    }
    Ok(None)
}
impl Flow<'_> {
    fn known_bound(
        &mut self,
        expr: &Expression,
        length: Option<u64>,
        allow_end: bool,
    ) -> anyhow::Result<Option<u128>> {
        use super::constant_data::DataKind;
        let Some(evaluator) = &mut self.constant_data else {
            return Ok(None);
        };
        let Some(value) = evaluator.try_evaluate(expr)? else {
            return Ok(None);
        };
        let DataKind::Numeric(value) = value.kind else {
            return Ok(None);
        };
        if matches!(value.ty, crate::numeric::NumericType::Integer { width, signed: true } if crate::numeric::signed(value.bits, width) < 0)
            || length.is_some_and(|n| {
                if allow_end {
                    value.bits > u128::from(n)
                } else {
                    value.bits >= u128::from(n)
                }
            })
        {
            return Err(at_span(
                type_error("index or slice bounds out of bounds"),
                expr.span,
            ));
        }
        Ok(Some(value.bits))
    }

    fn check_bounds(&mut self, expr: &Expression) -> anyhow::Result<()> {
        let object = match &expr.kind {
            E::IndexAccess { object, .. } | E::SliceLiteral { object, .. } => object,
            _ => return Ok(()),
        };
        let Some(info) = object.annotation.resolved else {
            return Ok(());
        };
        if info.type_value.is_some() {
            return Ok(());
        }
        let length = match self.types.container(info.ty)?.0 {
            T::Array { length, .. } => Some(*length),
            _ => None,
        };
        match &expr.kind {
            E::IndexAccess { index, .. } => {
                self.known_bound(index, length, false)?;
            }
            E::SliceLiteral {
                start,
                end,
                inclusive,
                ..
            } => {
                let from = if let Some(start) = start {
                    self.known_bound(start, length, true)?
                } else {
                    Some(0)
                };
                let to = if let Some(end) = end {
                    self.known_bound(end, length, !inclusive)?
                        .map(|n| {
                            if *inclusive {
                                n.checked_add(1)
                                    .ok_or_else(|| type_error("inclusive slice bound overflows"))
                            } else {
                                Ok(n)
                            }
                        })
                        .transpose()?
                } else {
                    length.map(u128::from)
                };
                if let (Some(from), Some(to)) = (from, to)
                    && from > to
                {
                    return Err(at_span(type_error("slice bounds are reversed"), expr.span));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn initialize(&self, v: &VariableDeclaration, state: &mut State) -> anyhow::Result<()> {
        state.insert(root(
            v.annotation
                .binding
                .ok_or_else(|| type_error("pending binding"))?,
        ));
        Ok(())
    }
    fn ready(&self, p: &Place, ty: TypeId, state: &State) -> anyhow::Result<bool> {
        if (0..=p.path.len()).any(|n| {
            state.contains(&Place {
                binding: p.binding,
                path: p.path[..n].to_vec(),
            })
        }) {
            return Ok(true);
        }
        // Unknown shape can be void, empty, or require projected writes. Do not
        // turn that deferred obligation into a failure on the source template;
        // concrete instances run this same analysis with template mode disabled.
        if self.template && self.types.is_symbolic(ty)? {
            return Ok(true);
        }
        match self.types.container(ty)?.0 {
            T::Struct(fs) => {
                for (i, f) in fs.iter().enumerate() {
                    let mut field = p.clone();
                    field.path.push(Projection::Field(i as u32));
                    if !self.ready(&field, f.ty, state)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            T::Array { element, length } => {
                // Every element needs a written descendant; avoid iterating huge arrays.
                let indices = state
                    .iter()
                    .filter(|q| q.binding == p.binding && q.path.starts_with(&p.path))
                    .filter_map(|q| match q.path.get(p.path.len()) {
                        Some(Projection::Index(i)) => Some(*i),
                        _ => None,
                    })
                    .collect::<HashSet<_>>();
                if indices.len() as u64 != *length {
                    return Ok(false);
                }
                for index in indices {
                    let mut item = p.clone();
                    item.path.push(Projection::Index(index));
                    if !self.ready(&item, *element, state)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn read(&mut self, expr: &Expression, state: &State) -> anyhow::Result<()> {
        self.check_bounds(expr)?;
        if binding(expr).is_some_and(|id| self.recovery.as_ref().is_some_and(|r| r.blocked(id))) {
            return Err(super::recovery::Blocked.into());
        }
        let result = (|| {
            // Do not read operands that cannot be reached after an earlier one diverges.
            macro_rules! read {
                ($e:expr) => {{
                    let e = $e;
                    self.read(e, state)?;
                    if super::value_typing::diverges(e, self.types) {
                        return Ok(());
                    }
                }};
            }
            if let Some(info) = expr.annotation.resolved
                && matches!(
                    info.category,
                    ValueCategory::MutablePlace
                        | ValueCategory::ImmutablePlace
                        | ValueCategory::ReadonlyPlace
                )
                && let Some(p) = place(expr, self.types)?
            {
                self.address(expr, state)?;
                if !self.ready(&p, info.ty, state)? {
                    return Err(type_error("read of uninitialized binding or member"));
                }
                return Ok(());
            }
            match &expr.kind {
                E::UnaryExpression(u) if u.operator == O::AddressOf => {
                    self.address(&u.expression, state)?
                }
                E::UnaryExpression(u) => self.read(&u.expression, state)?,
                E::BinaryExpresison(b) => {
                    read!(&b.left);
                    let known = super::evaluation::boolean(&b.left, &HashMap::new());
                    if !matches!(
                        (b.operator, known),
                        (O::LogicalAnd, Some(false)) | (O::LogicalOr, Some(true))
                    ) {
                        self.read(&b.right, state)?;
                    }
                }
                E::Call { callee, args } => {
                    read!(callee);
                    for a in args {
                        read!(a);
                    }
                }
                E::ExprAcess { expr, .. } => self.read(expr, state)?,
                E::StructConstruct { fields, .. } => {
                    for f in fields {
                        read!(&f.expression);
                    }
                }
                E::Cast { expr, .. } => self.read(expr, state)?,
                E::IndexAccess { object, index } => {
                    if object.annotation.resolved.is_some_and(|i| {
                        matches!(self.types.container(i.ty), Ok((T::Array { .. }, _)))
                    }) {
                        self.address(object, state)?;
                        // The actual element is read only after the index has
                        // completed. A diverging index reads no array elements.
                        read!(index);
                        self.read(object, state)?;
                    } else {
                        read!(object);
                        read!(index);
                    }
                }
                E::SliceLiteral {
                    object, start, end, ..
                } => {
                    read!(object);
                    for e in start.iter().chain(end) {
                        read!(e);
                    }
                }
                E::ArrayLiteral { elements } => {
                    for e in elements {
                        read!(e);
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        result.map_err(|e| at_span(e, expr.span))
    }
    fn address(&mut self, expr: &Expression, state: &State) -> anyhow::Result<()> {
        self.check_bounds(expr)?;
        match &expr.kind {
            E::QualifiedIdentifier(_) => Ok(()),
            E::ExprAcess { expr, member, .. } => {
                if member.implicit_deref {
                    self.read(expr, state)
                } else {
                    self.address(expr, state)
                }
            }
            E::IndexAccess { object, index } => {
                if matches!(
                    object
                        .annotation
                        .resolved
                        .and_then(|i| self.types.container(i.ty).ok()),
                    Some((T::Array { .. }, _))
                ) {
                    self.address(object, state)?;
                } else {
                    self.read(object, state)?;
                }
                self.read(index, state)
            }
            E::Call { callee, args }
                if callee
                    .annotation
                    .resolved
                    .is_some_and(|i| matches!(self.types.get(i.ty), Ok(T::IntrinsicFunction))) =>
            {
                for arg in args {
                    self.read(arg, state)?;
                }
                Ok(())
            }
            E::UnaryExpression(u) if u.operator == O::Dereference => {
                self.read(&u.expression, state)
            }
            _ => self.read(expr, state),
        }
    }
    fn write(&mut self, expr: &Expression, state: &mut State) -> anyhow::Result<()> {
        self.address(expr, state)?;
        if let Some(p) = place(expr, self.types)? {
            state.insert(p);
        }
        Ok(())
    }
    fn block(&mut self, nodes: &[Statement], state: State) -> anyhow::Result<Vec<Exit>> {
        let mut active = Some(state);
        let mut exits = vec![];
        let mut defers = vec![];
        for node in nodes {
            let Some(state) = active.take() else {
                break;
            };
            if self
                .recovery
                .as_ref()
                .is_some_and(|r| r.statement_failed(self.file, node))
            {
                active = Some(state);
                continue;
            }
            if let S::Defer(deferred) = &node.kind {
                defers.push(deferred.as_slice());
                active = Some(state);
                continue;
            }
            let result = self
                .statement(node, state.clone())
                .map_err(|e| at_span(e, node.span));
            let statement_exits = match result {
                Ok(exits) => exits,
                Err(error) => {
                    if let Some(recovery) = &self.recovery {
                        recovery.record(error, self.file, node.span, self.paths.get(&self.file));
                        recovery.block_statement(node);
                        active = Some(state);
                        continue;
                    }
                    return Err(error);
                }
            };
            let mut next = vec![];
            for mut exit in statement_exits {
                if exit.kind == ExitKind::Next {
                    next.push(exit.state);
                } else {
                    if exit.kind != ExitKind::Diverge {
                        self.defers(&defers, &mut exit.state)?;
                    }
                    exits.push(exit);
                }
            }
            active = self.merge(next)?;
        }
        if let Some(mut state) = active {
            self.defers(&defers, &mut state)?;
            exits.push(Exit {
                kind: ExitKind::Next,
                state,
            });
        }
        Ok(exits)
    }
    fn defers(&mut self, nodes: &[&[Statement]], state: &mut State) -> anyhow::Result<()> {
        for node in nodes.iter().rev() {
            let exits = self.block(node, state.clone())?;
            *state = self
                .merge(
                    exits
                        .into_iter()
                        .filter(|e| e.kind == ExitKind::Next)
                        .map(|e| e.state)
                        .collect(),
                )?
                .ok_or_else(|| type_error("defer must complete normally"))?;
        }
        Ok(())
    }
    fn merge(&self, states: Vec<State>) -> anyhow::Result<Option<State>> {
        if states.is_empty() {
            return Ok(None);
        }
        let candidates = states
            .iter()
            .flat_map(|s| s.iter())
            .flat_map(|p| {
                (0..=p.path.len()).map(|n| Place {
                    binding: p.binding,
                    path: p.path[..n].to_vec(),
                })
            })
            .collect::<HashSet<_>>();
        let mut result = State::new();
        for place in candidates {
            let mut ty = self.types.bindings.get(&place.binding).map(|i| i.ty);
            for projection in &place.path {
                ty = match (
                    ty.map(|t| self.types.container(t).map(|(ty, _)| ty))
                        .transpose()?,
                    projection,
                ) {
                    (Some(T::Struct(fs)), Projection::Field(index)) => {
                        fs.get(*index as usize).map(|f| f.ty)
                    }
                    (Some(T::Array { element, .. }), Projection::Index(_)) => Some(*element),
                    _ => None,
                };
            }
            let mut initialized = true;
            for state in &states {
                if !(if let Some(ty) = ty {
                    self.ready(&place, ty, state)?
                } else {
                    state.contains(&place)
                }) {
                    initialized = false;
                    break;
                }
            }
            if initialized {
                result.insert(place);
            }
        }
        Ok(Some(result))
    }
    fn statement(&mut self, node: &Statement, mut state: State) -> anyhow::Result<Vec<Exit>> {
        let comp_values: Vec<_> = match &node.kind {
            S::VariableDeclaration(v) if v.is_comp => v.expression.iter().collect(),
            S::MultiVariableDeclaration { bindings, values } => bindings
                .iter()
                .zip(values)
                .filter_map(|(v, e)| v.is_comp.then_some(e))
                .collect(),
            _ => vec![],
        };
        for expr in comp_values {
            if expr
                .annotation
                .resolved
                .is_some_and(|i| i.type_value.is_none())
                && !matches!(expr.kind, E::QualifiedIdentifier(_))
                && !self
                    .types
                    .is_symbolic(expr.annotation.resolved.unwrap().ty)?
                && let Some(evaluator) = &mut self.constant_data
            {
                evaluator.evaluate(expr)?;
            }
        }
        let mut kind = ExitKind::Next;
        // Evaluate mandatory inputs in the same order as the statement below.
        // Divergence is an exit, not a successful write, return, or branch join.
        let inputs: Vec<&Expression> = match &node.kind {
            S::VariableDeclaration(v) => v.expression.iter().collect(),
            S::MultiVariableDeclaration { values, .. } | S::MultiAssignment { values, .. } => {
                values.iter().collect()
            }
            S::ExpressionStatement(e) => vec![e],
            S::Assignment { right, .. } => vec![right],
            S::DerefAssign { pointer, expr } => vec![pointer, expr],
            S::IndexAssign { expr, .. } => vec![expr],
            S::Return(values) => values.iter().flatten().collect(),
            S::If { condition, .. } => vec![condition],
            S::While { condition, .. } => condition.iter().collect(),
            S::Switch { subject, .. } => vec![subject],
            _ => vec![],
        };
        if inputs
            .iter()
            .any(|e| super::value_typing::diverges(e, self.types))
        {
            for expr in inputs {
                self.read(expr, &state)?;
                if super::value_typing::diverges(expr, self.types) {
                    return Ok(vec![Exit {
                        kind: ExitKind::Diverge,
                        state,
                    }]);
                }
            }
        }
        match &node.kind {
            S::IndexedCompoundAssignment { .. } => {
                return Err(type_error(
                    "indexed compound assignment analysis is not implemented",
                ));
            }
            S::VariableDeclaration(v) => {
                if let Some(expr) = &v.expression {
                    self.read(expr, &state)?;
                    self.initialize(v, &mut state)?;
                }
            }
            S::MultiVariableDeclaration { bindings, values } => {
                for value in values {
                    self.read(value, &state)?;
                }
                if !values.is_empty() {
                    for v in bindings {
                        self.initialize(v, &mut state)?;
                    }
                }
            }
            S::ExpressionStatement(e) => {
                self.read(e, &state)?;
                if e.annotation
                    .resolved
                    .is_some_and(|i| matches!(self.types.get(i.ty), Ok(T::Builtin(B::Never))))
                {
                    kind = ExitKind::Diverge;
                }
            }
            S::Assignment { left, right } => {
                self.read(right, &state)?;
                self.write(left, &mut state)?;
            }
            S::MultiAssignment { targets, values } => {
                for e in values {
                    self.read(e, &state)?;
                }
                // All destination addresses are evaluated before any writes.
                for t in targets {
                    self.address(t, &state)?;
                }
                for t in targets {
                    self.write(t, &mut state)?;
                }
            }
            S::DerefAssign { pointer, expr } => {
                self.read(pointer, &state)?;
                self.read(expr, &state)?;
            }
            S::IndexAssign {
                object,
                index,
                expr,
            } => {
                self.read(expr, &state)?;
                let target = Expression::at(
                    E::IndexAccess {
                        object: Box::new(object.clone()),
                        index: Box::new(index.clone()),
                    },
                    node.span,
                );
                self.write(&target, &mut state)?;
            }
            S::Return(values) => {
                for e in values.iter().flatten() {
                    self.read(e, &state)?;
                }
                kind = ExitKind::Return;
            }
            S::Break | S::Continue => {
                if self.loops == 0 {
                    return Err(type_error("break/continue requires an enclosing loop"));
                }
                kind = if matches!(node.kind, S::Break) {
                    ExitKind::Break
                } else {
                    ExitKind::Continue
                };
            }
            S::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.read(condition, &state)?;
                let known = super::known_boolean(condition);
                let mut exits = vec![];
                if known != Some(false) {
                    exits.extend(self.block(then_branch, state.clone())?);
                }
                if known != Some(true) {
                    exits.extend(self.block(else_branch.as_deref().unwrap_or(&[]), state)?);
                }
                return Ok(exits);
            }
            S::While { condition, body } => {
                if let Some(e) = condition {
                    self.read(e, &state)?;
                }
                let known = condition
                    .as_ref()
                    .map(|e| super::evaluation::boolean(e, &HashMap::new()))
                    .unwrap_or(Some(true));
                if known == Some(false) {
                    return Ok(vec![Exit { kind, state }]);
                }
                self.loops += 1;
                let result = self.block(body, state.clone());
                self.loops -= 1;
                let mut exits = vec![];
                if known != Some(true) {
                    exits.push(Exit { kind, state });
                }
                for mut exit in result? {
                    match exit.kind {
                        ExitKind::Break => {
                            exit.kind = ExitKind::Next;
                            exits.push(exit);
                        }
                        ExitKind::Next | ExitKind::Continue => {}
                        _ => exits.push(exit),
                    }
                }
                return Ok(exits);
            }
            S::Switch { subject, arms } => {
                self.read(subject, &state)?;
                let mut exits = vec![];
                let mut seen = HashSet::new();
                let mut wildcard = false;
                for arm in arms {
                    let mut state = state.clone();
                    if wildcard {
                        return Err(type_error("switch arm after wildcard is unreachable"));
                    }
                    match &arm.pattern {
                        Pattern::EnumVariant {
                            variant, binding, ..
                        } => {
                            if !seen.insert(variant) {
                                return Err(type_error("duplicate switch variant"));
                            }
                            if let Some(b) = binding {
                                state.insert(root(
                                    b.annotation
                                        .binding
                                        .ok_or_else(|| type_error("pending pattern binding"))?,
                                ));
                            }
                        }
                        Pattern::Wildcard => wildcard = true,
                    }
                    exits.extend(self.block(&arm.body, state)?);
                }
                let ty = subject
                    .annotation
                    .resolved
                    .ok_or_else(|| type_error("pending switch subject"))?
                    .ty;
                let complete = matches!(self.types.get(ty)?, T::Enum(vs) if vs.len() == seen.len());
                if !wildcard && !complete {
                    return Err(type_error("non-exhaustive enum switch"));
                }
                return Ok(exits);
            }
            S::FunctionDeclaration(_) => {}
            S::Defer(_) => return Err(type_error("defer requires a lexical function block")),
        }
        Ok(vec![Exit { kind, state }])
    }
}
