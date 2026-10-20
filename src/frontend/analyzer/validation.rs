//! Read-only validation of the type pass. It checks references and annotations,
//! without rerunning inference or claiming initialization/control-flow safety.
use super::{at_span, types::check_assignable, value_typing::member};
use crate::{
    diagnostics::Span,
    driver::CompilationUnit,
    frontend::{
        analysis_annotation::{BindingAnnotation, BindingId, ExpressionInfo, MemberAnnotation},
        ast::{
            expression::{Expression, ExpressionKind as E},
            pattern::Pattern,
            qualified_name::QualifiedIdentifier,
            statement::{Statement, StatementKind as S},
            structl_field::StructField,
            type_expression::{TypeExpression, TypeExpressionKind as K},
            variable_declaration::VariableDeclaration,
        },
        identifier::{Entity, ResolvedName},
        namespace::ProjectSymbols,
        semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
        tokens::builtin::BuiltinType as B,
    },
};
use std::collections::HashSet;

pub(super) fn validate(
    units: &[CompilationUnit],
    symbols: &ProjectSymbols,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    let mut v = Validator {
        symbols,
        types,
        bindings: HashSet::new(),
        namespaces: HashSet::new(),
    };
    let mut files = HashSet::new();
    for unit in units {
        if !files.insert(unit.source_file_id)
            || !symbols.namespaces.contains_key(&unit.source_file_id)
        {
            return Err(type_error("invalid or duplicate compilation-unit identity"));
        }
        v.collect(&unit.ast.statements)?;
    }
    for (index, ty) in types.types.iter().enumerate() {
        types.validate_type(TypeId(index as u32))?;
        if let T::Declared(id) = ty {
            symbols
                .declaration(*id)
                .map_err(|_| type_error("invalid declared-type identity"))?;
        }
        if let T::Parameter(id) = ty {
            let binding = BindingId::Local(*id);
            if !v.bindings.contains(&binding)
                || types.type_values.get(&binding) != Some(&TypeId(index as u32))
            {
                return Err(type_error("invalid type parameter identity"));
            }
        }
    }
    for (&(owner, index), &constructor) in &types.variant_constructors {
        let T::Enum(variants) = types.get(owner)? else {
            return Err(type_error("variant constructor has a non-enum owner"));
        };
        let payload = variants
            .get(index as usize)
            .and_then(|v| v.payload)
            .ok_or_else(|| type_error("variant constructor has no payload owner"))?;
        let T::Struct(fields) = types.get(payload)? else {
            return Err(type_error("variant payload has a non-struct type"));
        };
        let T::Function(signature) = types.get(constructor)? else {
            return Err(type_error("variant constructor is not callable"));
        };
        if signature.return_type != owner
            || signature.variadic
            || signature.parameters != fields.iter().map(|f| f.ty).collect::<Vec<_>>()
        {
            return Err(type_error("variant constructor signature mismatch"));
        }
    }
    for (index, ty) in types.types.iter().enumerate() {
        if let T::Enum(variants) = ty {
            for (variant, v) in variants.iter().enumerate() {
                if v.payload.is_some()
                    && !types
                        .variant_constructors
                        .contains_key(&(TypeId(index as u32), variant as u32))
                {
                    return Err(type_error("missing variant constructor signature"));
                }
            }
        }
    }
    for (binding, info) in &types.bindings {
        if !v.bindings.contains(binding) {
            return Err(type_error("semantic table contains an unknown binding"));
        }
        v.info(*info)?;
        let ty = match binding {
            BindingId::Declaration(id) => types.declarations.get(id),
            BindingId::Local(id) => types.locals.get(id),
        };
        if ty != Some(&info.ty) || types.type_values.get(binding).copied() != info.type_value {
            return Err(type_error("inconsistent semantic binding tables"));
        }
    }
    for id in types
        .declarations
        .keys()
        .map(|id| BindingId::Declaration(*id))
        .chain(types.locals.keys().map(|id| BindingId::Local(*id)))
        .chain(types.type_values.keys().copied())
    {
        if !types.bindings.contains_key(&id) {
            return Err(type_error("semantic table contains a missing binding"));
        }
    }
    for unit in units {
        v.statements(&unit.ast.statements)?;
    }
    Ok(())
}
struct Validator<'a> {
    symbols: &'a ProjectSymbols,
    types: &'a SemanticTypes,
    bindings: HashSet<BindingId>,
    namespaces: HashSet<BindingId>,
}
impl Validator<'_> {
    fn register(&mut self, slot: BindingAnnotation, span: Span) -> anyhow::Result<BindingId> {
        let id = slot
            .binding
            .ok_or_else(|| at_span(type_error("pending binding annotation"), span))?;
        if !self.bindings.insert(id) {
            return Err(at_span(type_error("duplicate binding identity"), span));
        }
        if let BindingId::Declaration(id) = id {
            self.symbols
                .declaration(id)
                .map_err(|_| at_span(type_error("invalid declaration identity"), span))?;
        }
        Ok(id)
    }
    fn namespace_value(&self, expr: &Expression) -> bool {
        matches!(&expr.kind, E::QualifiedIdentifier(n) if matches!(n.annotation.resolved,Some(ResolvedName::Entity(Entity::File(_)))))
    }
    fn collect_var(
        &mut self,
        var: &VariableDeclaration,
        value: Option<&Expression>,
        span: Span,
    ) -> anyhow::Result<()> {
        let id = self.register(var.annotation, span)?;
        if var.is_comp && value.is_some_and(|expr| self.namespace_value(expr)) {
            self.namespaces.insert(id);
        }
        Ok(())
    }
    fn collect(&mut self, nodes: &[Statement]) -> anyhow::Result<()> {
        for node in nodes {
            match &node.kind {
                S::VariableDeclaration(var) => {
                    self.collect_var(var, var.expression.as_ref(), node.span)?
                }
                S::MultiVariableDeclaration { bindings, values } => {
                    for (index, var) in bindings.iter().enumerate() {
                        self.collect_var(var, values.get(index), node.span)?;
                    }
                }
                S::FunctionDeclaration(fun) => {
                    self.register(fun.signature.annotation, node.span)?;
                    for parameter in &fun.signature.parameters {
                        self.register(parameter.annotation, parameter.parameter_type.span)?;
                    }
                    if let Some(body) = &fun.body {
                        self.collect(&body.statements)?;
                    }
                }
                S::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.collect(then_branch)?;
                    if let Some(branch) = else_branch {
                        self.collect(branch)?;
                    }
                }
                S::While { body, .. } => self.collect(body)?,
                S::Switch { arms, .. } => {
                    for arm in arms {
                        if let Pattern::EnumVariant {
                            binding: Some(binding),
                            ..
                        } = &arm.pattern
                        {
                            self.register(binding.annotation, node.span)?;
                        }
                        self.collect(&arm.body)?;
                    }
                }
                S::Defer(node) => self.collect(std::slice::from_ref(node))?,
                _ => {}
            }
        }
        Ok(())
    }
    fn info(&self, info: ExpressionInfo) -> anyhow::Result<()> {
        self.types.validate_type(info.ty)?;
        if let Some(ty) = info.type_value {
            self.types.validate_type(ty)?;
            if !matches!(self.types.get(info.ty)?, T::Builtin(B::MetaType)) {
                return Err(type_error("type value has a non-metatype expression type"));
            }
        } else if matches!(self.types.get(info.ty)?, T::Builtin(B::MetaType)) {
            return Err(type_error("metatype expression has no denoted type"));
        }
        Ok(())
    }
    fn binding(&self, slot: BindingAnnotation) -> anyhow::Result<Option<ExpressionInfo>> {
        let id = slot
            .binding
            .ok_or_else(|| type_error("pending binding annotation"))?;
        if !self.bindings.contains(&id) {
            return Err(type_error("unknown binding identity"));
        }
        if self.namespaces.contains(&id) {
            if self.types.bindings.contains_key(&id) {
                return Err(type_error("namespace has a fabricated value type"));
            }
            return Ok(None);
        }
        let info = *self
            .types
            .bindings
            .get(&id)
            .ok_or_else(|| type_error("missing binding type"))?;
        self.info(info)?;
        Ok(Some(info))
    }
    fn name(&self, name: &QualifiedIdentifier) -> anyhow::Result<Option<ExpressionInfo>> {
        let resolved = name
            .annotation
            .resolved
            .ok_or_else(|| type_error("pending name annotation"))?;
        let id = match resolved {
            ResolvedName::Local(id) => BindingId::Local(id),
            ResolvedName::Entity(Entity::Declaration(id)) => BindingId::Declaration(id),
            ResolvedName::Entity(Entity::File(id)) => {
                if !self.symbols.namespaces.contains_key(&id) {
                    return Err(type_error("unknown file identity"));
                }
                return Ok(None);
            }
        };
        self.binding(BindingAnnotation { binding: Some(id) })
    }
    fn selected(
        &self,
        slot: MemberAnnotation,
        owner: TypeId,
        spelling: &str,
    ) -> anyhow::Result<()> {
        let info = slot
            .resolved
            .ok_or_else(|| type_error("pending member annotation"))?;
        self.types.validate_type(info.owner)?;
        self.types.validate_type(info.ty)?;
        if info != member(owner, spelling, self.types)? {
            return Err(type_error(
                "member annotation does not match its owner, spelling or type",
            ));
        }
        Ok(())
    }
    fn fields(
        &self,
        fields: &[StructField],
        owner: TypeId,
        definition: bool,
    ) -> anyhow::Result<()> {
        for field in fields {
            let result = (|| {
                let value = self.expression(&field.expression)?;
                self.selected(field.annotation, owner, &field.field_identifier)?;
                let ty = field.annotation.resolved.unwrap().ty;
                if definition {
                    if value.type_value != Some(ty) {
                        return Err(type_error(
                            "field definition annotation disagrees with its type value",
                        ));
                    }
                } else {
                    check_assignable(ty, value.ty, self.types)?;
                }
                Ok(())
            })();
            result.map_err(|e| at_span(e, field.expression.span))?;
        }
        Ok(())
    }
    fn ty(&self, node: &TypeExpression) -> anyhow::Result<TypeId> {
        let result = (|| {
            let id = node
                .annotation
                .resolved
                .ok_or_else(|| type_error("pending type annotation"))?;
            self.types.validate_type(id)?;
            let consistent = match (&node.kind, self.types.get(id)?) {
                (K::Builtin(a), T::Builtin(b)) => a == b,
                (K::QualifiedIdentifier(name), _) => {
                    self.name(name)?.and_then(|info| info.type_value) == Some(id)
                }
                (K::Pointer { pointed_type }, T::Pointer(t)) => self.ty(pointed_type)? == *t,
                (K::Array { element_type, size }, T::Array { element, length }) => {
                    self.ty(element_type)? == *element && *size == *length
                }
                (K::Slice { element_type }, T::Slice(t)) => self.ty(element_type)? == *t,
                (K::Tuple { elements }, T::Tuple(ts)) => {
                    elements.len() == ts.len()
                        && elements
                            .iter()
                            .zip(ts)
                            .map(|(node, t)| self.ty(node).map(|id| id == *t))
                            .collect::<anyhow::Result<Vec<_>>>()?
                            .into_iter()
                            .all(|same| same)
                }
                (
                    K::Function {
                        argument_types,
                        return_type,
                    },
                    T::Function(f),
                ) => {
                    argument_types.len() == f.parameters.len()
                        && self.ty(return_type)? == f.return_type
                        && argument_types
                            .iter()
                            .zip(&f.parameters)
                            .map(|(node, t)| self.ty(node).map(|id| id == *t))
                            .collect::<anyhow::Result<Vec<_>>>()?
                            .into_iter()
                            .all(|same| same)
                }
                (K::Struct { fields }, T::Struct(fs)) => {
                    self.fields(fields, id, true)?;
                    fields.len() == fs.len()
                        && fields
                            .iter()
                            .zip(fs)
                            .all(|(a, b)| a.field_identifier == b.spelling)
                }
                (K::Enum { variants }, T::Enum(vs)) => {
                    if variants.len() != vs.len() {
                        return Err(type_error("enum annotation has wrong variant count"));
                    }
                    for (node, variant) in variants.iter().zip(vs) {
                        if node.identifier != variant.spelling {
                            return Err(type_error("enum annotation has wrong variant identity"));
                        }
                        match (&node.payload, variant.payload) {
                            (Some(fields), Some(payload)) => self.fields(fields, payload, true)?,
                            (None, None) => {}
                            _ => return Err(type_error("enum payload annotation mismatch")),
                        }
                    }
                    true
                }
                _ => false,
            };
            if !consistent {
                return Err(type_error("type annotation disagrees with syntax"));
            }
            Ok(id)
        })();
        result.map_err(|e| at_span(e, node.span))
    }
    fn expression(&self, node: &Expression) -> anyhow::Result<ExpressionInfo> {
        let result = (|| {
            let info = node
                .annotation
                .resolved
                .ok_or_else(|| type_error("pending expression annotation"))?;
            self.info(info)?;
            match &node.kind {
                E::QualifiedIdentifier(name) => {
                    if self.name(name)? != Some(info) {
                        return Err(type_error(
                            "expression annotation disagrees with its name binding",
                        ));
                    }
                }
                E::TypeValue(ty) => {
                    if info.type_value != Some(self.ty(ty)?) {
                        return Err(type_error("expression denotes the wrong type"));
                    }
                }
                E::ExprAcess {
                    expr,
                    accessed_identifier,
                    member,
                } => {
                    let receiver = self.expression(expr)?;
                    self.selected(
                        *member,
                        receiver.type_value.unwrap_or(receiver.ty),
                        accessed_identifier,
                    )?;
                    if member.resolved.unwrap().ty != info.ty {
                        return Err(type_error("member result type mismatch"));
                    }
                }
                E::StructConstruct {
                    struct_identifier,
                    fields,
                } => {
                    let owner = self
                        .name(struct_identifier)?
                        .and_then(|i| i.type_value)
                        .ok_or_else(|| type_error("missing struct owner type"))?;
                    if info.ty != owner {
                        return Err(type_error("struct construction owner mismatch"));
                    }
                    self.fields(fields, owner, false)?;
                }
                E::UnaryExpression(u) => {
                    self.expression(&u.expression)?;
                }
                E::BinaryExpresison(b) => {
                    self.expression(&b.left)?;
                    self.expression(&b.right)?;
                }
                E::Call { callee, args } => {
                    self.expression(callee)?;
                    for arg in args {
                        self.expression(arg)?;
                    }
                }
                E::Cast { expr, target_type } => {
                    self.expression(expr)?;
                    if self.ty(target_type)? != info.ty {
                        return Err(type_error("cast result annotation mismatch"));
                    }
                }
                E::IndexAccess { object, index } => {
                    self.expression(object)?;
                    self.expression(index)?;
                }
                E::SliceLiteral {
                    object, start, end, ..
                } => {
                    self.expression(object)?;
                    for bound in start.iter().chain(end) {
                        self.expression(bound)?;
                    }
                }
                E::ArrayLiteral { elements } => {
                    for element in elements {
                        self.expression(element)?;
                    }
                }
                E::QualifiedAccess { .. } => {
                    return Err(type_error(
                        "legacy qualified access cannot be type-complete",
                    ));
                }
                E::Literal(_) | E::VoidExpression => {}
            }
            Ok(info)
        })();
        result.map_err(|e| at_span(e, node.span))
    }
    fn variable(
        &self,
        var: &VariableDeclaration,
        value: Option<&Expression>,
    ) -> anyhow::Result<()> {
        let info = self.binding(var.annotation)?;
        if let Some(ty) = &var.ty {
            let declared = self.ty(ty)?;
            if info.is_none_or(|info| info.ty != declared) {
                return Err(type_error("binding type annotation mismatch"));
            }
        }
        if let Some(value) = value {
            if var.is_comp && self.namespace_value(value) {
                self.name(match &value.kind {
                    E::QualifiedIdentifier(n) => n,
                    _ => unreachable!(),
                })?;
                if info.is_some() || value.annotation.resolved.is_some() {
                    return Err(type_error("namespace has a fabricated expression type"));
                }
            } else {
                let value = self.expression(value)?;
                let binding = info.ok_or_else(|| type_error("missing value binding"))?;
                check_assignable(binding.ty, value.ty, self.types)?;
                if binding.type_value != value.type_value {
                    return Err(type_error("binding denotes the wrong type"));
                }
            }
        }
        Ok(())
    }
    fn statements(&self, nodes: &[Statement]) -> anyhow::Result<()> {
        for node in nodes {
            self.statement(node).map_err(|e| at_span(e, node.span))?;
        }
        Ok(())
    }
    fn statement(&self, node: &Statement) -> anyhow::Result<()> {
        match &node.kind {
            S::VariableDeclaration(var) => self.variable(var, var.expression.as_ref())?,
            S::MultiVariableDeclaration { bindings, values } => {
                for (index, var) in bindings.iter().enumerate() {
                    self.variable(
                        var,
                        if values.len() == bindings.len() {
                            values.get(index)
                        } else {
                            None
                        },
                    )?;
                }
                for value in values {
                    if !self.namespace_value(value) {
                        self.expression(value)?;
                    }
                }
            }
            S::FunctionDeclaration(fun) => {
                let info = self
                    .binding(fun.signature.annotation)?
                    .ok_or_else(|| type_error("missing function type"))?;
                let T::Function(signature) = self.types.get(info.ty)? else {
                    return Err(type_error("function binding is not callable"));
                };
                if signature.parameters.len() != fun.signature.parameters.len()
                    || signature.variadic != fun.is_variadic
                    || signature.return_type != self.ty(&fun.signature.return_type)?
                {
                    return Err(type_error("function signature annotation mismatch"));
                }
                for (parameter, ty) in fun.signature.parameters.iter().zip(&signature.parameters) {
                    if self.ty(&parameter.parameter_type)? != *ty
                        || self
                            .binding(parameter.annotation)?
                            .is_none_or(|info| info.ty != *ty)
                    {
                        return Err(type_error("parameter type annotation mismatch"));
                    }
                }
                if let Some(body) = &fun.body {
                    self.statements(&body.statements)?;
                }
            }
            S::ExpressionStatement(expr) => {
                self.expression(expr)?;
            }
            S::Assignment { left, right } => {
                self.expression(left)?;
                self.expression(right)?;
            }
            S::DerefAssign { pointer, expr } => {
                self.expression(pointer)?;
                self.expression(expr)?;
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
            S::Return(values) => {
                for expr in values.iter().flatten() {
                    self.expression(expr)?;
                }
            }
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
            S::While { condition, body } => {
                if let Some(condition) = condition {
                    self.expression(condition)?;
                }
                self.statements(body)?;
            }
            S::Switch { subject, arms } => {
                let owner = self.expression(subject)?.ty;
                for arm in arms {
                    if let Pattern::EnumVariant {
                        variant,
                        member,
                        binding,
                    } = &arm.pattern
                    {
                        self.selected(*member, owner, variant)?;
                        if let Some(binding) = binding {
                            self.binding(binding.annotation)?;
                        }
                    }
                    self.statements(&arm.body)?;
                }
            }
            S::Defer(node) => self.statement(node)?,
            S::Break | S::Continue => {}
        }
        Ok(())
    }
}
