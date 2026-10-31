//! Read-only validation of the type pass. It checks references and annotations,
//! without rerunning inference or claiming initialization/control-flow safety.

use super::{at_span, types::check_assignable, value_typing::member};
use crate::{
    CompilationUnit,
    diagnostics::Span,
    frontend::{
        analysis_annotation::{
            BindingAnnotation, BindingId, ExpressionInfo, MemberAnnotation, MemberKind,
            ValueCategory as C,
        },
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
        tokens::{Literal, Operator as O, builtin::BuiltinType as B},
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
        return_type: None,
    };
    let mut files = HashSet::new();
    for unit in units {
        if !files.insert(unit.source_file_id)
            || !symbols.namespaces.contains_key(&unit.source_file_id)
        {
            return Err(type_error("invalid or duplicate compilation-unit identity"));
        }
        v.collect(&unit.ast.statements)?;
        for member in crate::frontend::ast::members::declarations(&unit.ast.statements) {
            v.collect(std::slice::from_ref(member))?;
        }
    }
    // Compiler-provided namespaces have no source AST to walk. Their declarations
    // are nevertheless canonical bindings and must still pass the semantic-table
    // consistency checks below.
    for id in symbols.compiler_declarations.keys() {
        symbols.declaration(*id)?;
        v.bindings.insert(BindingId::Declaration(*id));
    }
    for namespace in symbols
        .namespaces
        .values()
        .filter(|namespace| !files.contains(&namespace.file_id))
    {
        for index in 0..namespace.declarations.len() {
            let index =
                u32::try_from(index).map_err(|_| type_error("declaration ID space exhausted"))?;
            v.bindings.insert(BindingId::Declaration(
                crate::frontend::identifier::DeclId {
                    file_id: namespace.file_id,
                    index,
                },
            ));
        }
    }
    for (index, ty) in types.types.iter().enumerate() {
        types.validate_type(TypeId(index as u32))?;
        if let T::Declared(id) = ty {
            symbols
                .declaration(*id)
                .map_err(|_| type_error("invalid declared-type identity"))?;
        }
        if let T::TypeResult { function, .. } = ty
            && (!types.generic_parameters.contains_key(function)
                || !matches!(types.declarations.get(function).and_then(|id| types.get(*id).ok()), Some(T::Function(f)) if f.is_comp))
        {
            return Err(type_error("invalid symbolic type computation"));
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
    for ty in &types.types {
        if let T::TypeResult {
            function,
            arguments,
        } = ty
        {
            let T::Function(signature) = types.get(
                *types
                    .declarations
                    .get(function)
                    .ok_or_else(|| type_error("missing computation function"))?,
            )?
            else {
                return Err(type_error("type computation is not callable"));
            };
            if arguments.len() != signature.parameters.len() {
                return Err(type_error("type computation argument arity mismatch"));
            }
            for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
                use crate::frontend::semantic_type::ComputationArgument as A;
                let valid = match argument {
                    A::Type(t) => {
                        types.validate_type(*t)?;
                        matches!(types.get(*parameter)?, T::Builtin(B::MetaType))
                    }
                    A::Bool(_) => matches!(types.get(*parameter)?, T::Builtin(B::Boolean)),
                    A::Expression(e) => {
                        matches!(types.get(*parameter)?, T::Builtin(B::Boolean))
                            && matches!(types.get(v.expression(e)?.ty)?, T::Builtin(B::Boolean))
                    }
                };
                if !valid {
                    return Err(type_error("invalid retained computation argument"));
                }
            }
        }
    }
    for ((function, arguments), result) in &types.computation_results {
        let Some(signature) = types.declarations.get(function) else {
            return Err(type_error("unknown computed result function"));
        };
        let T::Function(signature) = types.get(*signature)? else {
            return Err(type_error("invalid computed result function"));
        };
        if !signature.is_comp
            || arguments.len() != signature.parameters.len()
            || (types.is_dependent(*result)? && !arguments.iter().any(|a| matches!(a, crate::frontend::semantic_type::ComputationArgument::Type(t) if types.is_dependent(*t).unwrap_or(false))))
        {
            return Err(type_error("invalid concrete computation result"));
        }
        for argument in arguments {
            use crate::frontend::semantic_type::ComputationArgument as A;
            match argument {
                A::Type(t) => types.validate_type(*t)?,
                A::Bool(_) => {}
                _ => return Err(type_error("unresolved concrete computation argument")),
            }
        }
    }
    for (key, instance) in &types.generic_instances {
        if key != &instance.key {
            return Err(type_error("generic instance cache key mismatch"));
        }
        super::generics::validate_instance(instance, types)?;
        if let Some(fun) = &instance.body {
            if fun.signature.annotation.binding != Some(BindingId::Declaration(key.template)) {
                return Err(type_error("instance body has the wrong template identity"));
            }
            let template = units.iter().flat_map(|u| u.ast.statements.iter().chain(crate::frontend::ast::members::declarations(&u.ast.statements))).find(|node| matches!(&node.kind, S::FunctionDeclaration(f) if f.signature.annotation.binding == fun.signature.annotation.binding))
                .ok_or_else(|| type_error("instance has no source template"))?;
            let source = Statement {
                kind: S::FunctionDeclaration(fun.clone()),
                span: template.span,
            };
            let (source, owned) = instance_source(source)?;
            if source != instance_source(template.clone())?.0 {
                return Err(type_error(
                    "instance body disagrees with its source template",
                ));
            }
            let expected: HashSet<_> = owned
                .into_iter()
                .filter(|id| {
                    *id != BindingId::Declaration(key.template) && !v.namespaces.contains(id)
                })
                .collect();
            if instance.bindings.keys().copied().collect::<HashSet<_>>() != expected {
                return Err(type_error(
                    "instance environment does not match its owned bindings",
                ));
            }
            let mut environment = types.clone();
            for (id, info) in &instance.bindings {
                if !v.bindings.contains(id)
                    || *id == BindingId::Declaration(key.template)
                    || environment.is_dependent(info.ty)?
                    || (!fun.signature.is_comp
                        && info
                            .type_value
                            .is_some_and(|t| environment.is_dependent(t).unwrap_or(true)))
                {
                    return Err(type_error("invalid concrete instance binding"));
                }
                environment.type_values.remove(id);
                environment.bind(*id, *info);
            }

            let validator = Validator {
                symbols,
                types: &environment,
                bindings: v.bindings.clone(),
                namespaces: v.namespaces.clone(),
                return_type: None,
            };
            let T::Function(signature) = types.get(instance.signature)? else {
                return Err(type_error("invalid instance signature"));
            };
            if signature.calling_convention != fun.signature.calling_convention
                || signature.is_comp != fun.signature.is_comp
                || signature.parameters.len() != fun.signature.parameters.len()
                || signature.return_type != validator.ty(&fun.signature.return_type)?
            {
                return Err(type_error("instance signature annotation mismatch"));
            }
            for (parameter, ty) in fun.signature.parameters.iter().zip(&signature.parameters) {
                let id = parameter
                    .annotation
                    .binding
                    .ok_or_else(|| type_error("pending instance parameter"))?;
                if !instance.bindings.contains_key(&id)
                    || validator.ty(&parameter.parameter_type)? != *ty
                    || validator
                        .binding(parameter.annotation)?
                        .is_none_or(|info| info.ty != *ty)
                {
                    return Err(type_error("instance parameter environment mismatch"));
                }
            }
            if let Some(body) = &fun.body {
                validator
                    .with_return_type(signature.return_type)
                    .statements(&body.statements)?;
            }
            super::flow::validate_specialization(fun, &environment)?;
        }
    }
    for id in types.generic_parameters.keys() {
        if !v.bindings.contains(&BindingId::Declaration(*id)) {
            return Err(type_error("unknown generic template identity"));
        }
    }
    for ((owner, _), declaration) in &types.comp_members {
        if !matches!(types.get(*owner)?, T::Struct(_))
            || !v.bindings.contains(&BindingId::Declaration(*declaration))
            || !matches!(
                types
                    .declarations
                    .get(declaration)
                    .and_then(|ty| types.get(*ty).ok()),
                Some(T::Function(_))
            )
        {
            return Err(type_error("invalid comp member owner or declaration"));
        }
    }
    for (&(owner, index), &constructor) in &types.variant_constructors {
        let T::Enum(variants) = types.get(owner)? else {
            return Err(type_error("variant constructor has a non-enum owner"));
        };
        let payload = types
            .enum_payload(variants.get(index as usize).and_then(|v| v.payload))?
            .ok_or_else(|| type_error("variant constructor has no payload owner"))?;
        let T::Function(signature) = types.get(constructor)? else {
            return Err(type_error("variant constructor is not callable"));
        };
        if signature.return_type != owner || signature.parameters != vec![payload] {
            return Err(type_error("variant constructor signature mismatch"));
        }
    }
    for (index, ty) in types.types.iter().enumerate() {
        if let T::Enum(variants) = ty {
            for (variant, v) in variants.iter().enumerate() {
                if types.enum_payload(v.payload)?.is_some()
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
/// Compare instance recipes with source while ignoring only type-pass output.
fn instance_source(mut node: Statement) -> anyhow::Result<(Statement, HashSet<BindingId>)> {
    struct Source(HashSet<BindingId>);
    impl super::walk::Visitor for Source {
        fn binding(
            &mut self,
            _: &str,
            slot: &mut BindingAnnotation,
            _: Span,
        ) -> anyhow::Result<()> {
            if let Some(id) = slot.binding {
                self.0.insert(id);
            }
            Ok(())
        }
        fn statement(&mut self, node: &mut Statement) -> anyhow::Result<()> {
            if let S::Switch { arms, .. } = &mut node.kind {
                for arm in arms {
                    if let Pattern::EnumVariant { member, .. } = &mut arm.pattern {
                        *member = MemberAnnotation::default();
                    }
                }
            }
            Ok(())
        }
        fn expression(&mut self, node: &mut Expression) -> anyhow::Result<()> {
            node.annotation.resolved = None;
            match &mut node.kind {
                E::ExprAcess { member, .. } => *member = MemberAnnotation::default(),
                E::StructConstruct { fields, .. } => {
                    for f in fields {
                        f.annotation = MemberAnnotation::default();
                    }
                }
                _ => {}
            }
            Ok(())
        }
        fn ty(&mut self, node: &mut TypeExpression) -> anyhow::Result<()> {
            node.annotation.resolved = None;
            if let K::Struct { fields, .. } = &mut node.kind {
                for f in fields {
                    f.annotation = MemberAnnotation::default();
                }
            }
            Ok(())
        }
    }
    let mut source = Source(HashSet::new());
    super::walk::statements(&mut source, std::slice::from_mut(&mut node), false)?;
    Ok((node, source.0))
}

#[derive(Clone)]
struct Validator<'a> {
    symbols: &'a ProjectSymbols,
    types: &'a SemanticTypes,
    bindings: HashSet<BindingId>,
    namespaces: HashSet<BindingId>,
    return_type: Option<TypeId>,
}
impl Validator<'_> {
    fn with_return_type(&self, ty: TypeId) -> Self {
        Self {
            return_type: Some(ty),
            ..self.clone()
        }
    }

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
                S::Defer(node) => self.collect(node)?,
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
        self.binding(BindingAnnotation {
            binding: Some(id),
            ..Default::default()
        })
    }
    fn selected(
        &self,
        slot: MemberAnnotation,
        owner: TypeId,
        spelling: &str,
    ) -> anyhow::Result<()> {
        if slot.implicit_deref {
            return Err(type_error(
                "unexpected implicit dereference on a direct member",
            ));
        }
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
        let T::Struct(declared) = self.types.get(self.types.canonical(owner)?)? else {
            return Err(type_error("struct fields require a struct owner"));
        };
        let unique = fields
            .iter()
            .map(|f| &f.field_identifier)
            .collect::<HashSet<_>>();
        if fields.len() != declared.len() || unique.len() != fields.len() {
            return Err(type_error("struct fields must occur exactly once"));
        }
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
                (K::Computed(expression), _) => self.expression(expression)?.type_value == Some(id),
                (K::Builtin(a), T::Builtin(b)) => a == b,
                (K::QualifiedIdentifier(name), _) => {
                    self.name(name)?.and_then(|info| info.type_value) == Some(id)
                }
                (K::Readonly(inner), T::Readonly(t)) => self.ty(inner)? == *t,
                (K::Sentinel { inner, value }, T::Sentinel { inner: t, value: v }) => {
                    self.ty(inner)? == *t && value == v
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
                        calling_convention,
                        is_comp,
                        argument_types,
                        return_type,
                    },
                    T::Function(f),
                ) => {
                    *is_comp == f.is_comp
                        && *calling_convention == f.calling_convention
                        && argument_types.len() == f.parameters.len()
                        && self.ty(return_type)? == f.return_type
                        && argument_types
                            .iter()
                            .zip(&f.parameters)
                            .map(|(node, t)| self.ty(node).map(|id| id == *t))
                            .collect::<anyhow::Result<Vec<_>>>()?
                            .into_iter()
                            .all(|same| same)
                }
                (K::Struct { fields, members }, T::Struct(fs)) => {
                    self.fields(fields, id, true)?;
                    if self
                        .types
                        .comp_members
                        .keys()
                        .filter(|(owner, _)| *owner == id)
                        .count()
                        != members.len()
                    {
                        return Err(type_error("comp member count mismatch"));
                    }
                    let mut seen: HashSet<_> =
                        fields.iter().map(|f| f.field_identifier.as_str()).collect();
                    for member in members {
                        let S::FunctionDeclaration(function) = &member.kind else {
                            return Err(type_error("invalid comp member declaration"));
                        };
                        if !seen.insert(function.signature.identifier.as_str()) {
                            return Err(type_error("duplicate struct member"));
                        }
                        let Some(BindingId::Declaration(declaration)) =
                            function.signature.annotation.binding
                        else {
                            return Err(type_error("pending comp member identity"));
                        };
                        if self
                            .types
                            .comp_members
                            .get(&(id, function.signature.identifier.clone()))
                            != Some(&declaration)
                        {
                            return Err(type_error("comp member identity mismatch"));
                        }
                        super::function_declaration::validate_member_captures(
                            function, self.types,
                        )?;
                    }
                    self.statements(members)?;
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
                            (Some(expression), Some(payload)) => {
                                if self.expression(expression)?.type_value != Some(payload) {
                                    return Err(type_error("enum payload annotation mismatch"));
                                }
                            }
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
    fn contextual_expression(
        &self,
        node: &Expression,
        expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        let info = self.expression(node)?;
        let literal = match &node.kind {
            E::Literal(literal) => Some(literal),
            E::UnaryExpression(u) if u.operator == O::Minus => match &u.expression.kind {
                E::Literal(literal @ Literal::Integer(_)) => Some(literal),
                _ => None,
            },
            _ => None,
        };
        let numeric_context =
            expected.filter(|ty| self.types.get(*ty).is_ok_and(super::value_typing::numeric));
        let float_context = numeric_context.filter(|ty| {
            matches!(
                self.types.get(*ty),
                Ok(T::Builtin(
                    B::Float16 | B::Float32 | B::Float64 | B::Float128
                ))
            )
        });
        let consistent = match literal {
            Some(Literal::Integer(_)) => numeric_context.map_or_else(
                || matches!(self.types.get(info.ty), Ok(T::Builtin(B::Int32))),
                |ty| info.ty == ty,
            ),
            Some(Literal::Float(_)) => float_context.map_or_else(
                || matches!(self.types.get(info.ty), Ok(T::Builtin(B::Float64))),
                |ty| info.ty == ty,
            ),
            Some(Literal::Null) => expected.is_some_and(|ty| {
                ty == info.ty && matches!(self.types.container(ty), Ok((T::Pointer(_), _)))
            }),
            _ => true,
        };
        if !consistent {
            return Err(at_span(
                type_error("literal context annotation mismatch"),
                node.span,
            ));
        }
        if let Some(expected) = expected {
            check_assignable(expected, info.ty, self.types).map_err(|e| at_span(e, node.span))?;
        }
        Ok(info)
    }

    fn expression(&self, node: &Expression) -> anyhow::Result<ExpressionInfo> {
        self.expression_checked(node, false)
    }

    // The positive token in -128 is annotated int8 by typing, but only the
    // complete negation must fit int8. Do not range-check that token in isolation.
    fn expression_checked(
        &self,
        node: &Expression,
        negated_literal: bool,
    ) -> anyhow::Result<ExpressionInfo> {
        let result = (|| {
            let info = node
                .annotation
                .resolved
                .ok_or_else(|| type_error("pending expression annotation"))?;
            self.info(info)?;
            if super::value_typing::diverges(node, self.types)
                && (!matches!(self.types.get(info.ty)?, T::Builtin(B::Never))
                    || info.category != crate::frontend::analysis_annotation::ValueCategory::Value
                    || info.type_value.is_some())
            {
                return Err(type_error(
                    "diverging expression must have never value type",
                ));
            }
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
                    let (receiver, implicit_deref) =
                        super::value_typing::field_receiver(receiver, self.types)?;
                    match member.resolved.map(|m| m.kind) {
                        Some(MemberKind::Function(_) | MemberKind::EnumVariant(_))
                            if receiver.type_value.is_none() =>
                        {
                            return Err(type_error("member requires a type value"));
                        }
                        Some(MemberKind::Field(_) | MemberKind::Length)
                            if receiver.type_value.is_some() =>
                        {
                            return Err(type_error("member requires a runtime receiver"));
                        }
                        _ => {}
                    }
                    if member.implicit_deref != implicit_deref {
                        return Err(type_error("implicit dereference annotation mismatch"));
                    }
                    self.selected(
                        MemberAnnotation {
                            source_span: None,
                            resolved: member.resolved,
                            implicit_deref: false,
                        },
                        receiver.type_value.unwrap_or(receiver.ty),
                        accessed_identifier,
                    )?;
                    if member.resolved.unwrap().ty != info.ty {
                        return Err(type_error("member result type mismatch"));
                    }
                    let category = if matches!(
                        member.resolved.unwrap().kind,
                        MemberKind::Field(_) | MemberKind::Dependent
                    ) {
                        receiver.category
                    } else {
                        C::Value
                    };
                    if !super::value_typing::diverges(node, self.types) && info.category != category
                    {
                        return Err(type_error("member mutability annotation mismatch"));
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
                    if info.ty != owner && !super::value_typing::diverges(node, self.types) {
                        return Err(type_error("struct construction owner mismatch"));
                    }
                    self.fields(fields, owner, false)?;
                    if info.category != C::Value || info.type_value.is_some() {
                        return Err(type_error("struct construction value annotation mismatch"));
                    }
                }
                E::UnaryExpression(u) => {
                    let negative_literal = u.operator == O::Minus
                        && matches!(u.expression.kind, E::Literal(Literal::Integer(_)));
                    let operand = self.expression_checked(&u.expression, negative_literal)?;
                    if negative_literal
                        && (info.ty != operand.ty
                            || info.category != C::Value
                            || info.type_value.is_some())
                    {
                        return Err(type_error("negated literal annotation mismatch"));
                    }
                    if !super::value_typing::diverges(node, self.types) {
                        match u.operator {
                            O::Dereference => {
                                let (T::Pointer(pointee), readonly) =
                                    self.types.container(operand.ty)?
                                else {
                                    return Err(type_error(
                                        "dereference annotation requires a pointer",
                                    ));
                                };
                                let category = if readonly {
                                    C::ReadonlyPlace
                                } else {
                                    C::MutablePlace
                                };
                                if info.ty != *pointee || info.category != category {
                                    return Err(type_error(
                                        "dereference mutability annotation mismatch",
                                    ));
                                }
                            }
                            O::AddressOf => {
                                let (T::Pointer(pointee), readonly) =
                                    self.types.container(info.ty)?
                                else {
                                    return Err(type_error(
                                        "address annotation requires a pointer",
                                    ));
                                };
                                if operand.category == C::Value
                                    || *pointee != operand.ty
                                    || readonly != (operand.category != C::MutablePlace)
                                    || info.category != C::Value
                                {
                                    return Err(type_error(
                                        "address mutability annotation mismatch",
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                E::BinaryExpresison(b) => {
                    let left = self.expression(&b.left)?;
                    let right = self.expression(&b.right)?;
                    if !super::value_typing::diverges(node, self.types) {
                        let comparison = matches!(
                            b.operator,
                            O::DoubleEquals
                                | O::Different
                                | O::GreaterThan
                                | O::GreaterEqual
                                | O::LesserThan
                                | O::LesserEqual
                        );
                        let logical = matches!(b.operator, O::LogicalAnd | O::LogicalOr);
                        let shift = matches!(b.operator, O::LeftShift | O::RightShift);
                        for operand in [left.ty, right.ty] {
                            if self.types.is_symbolic(operand)?
                                || matches!(self.types.get(operand)?, T::Builtin(B::Never))
                            {
                                continue;
                            }
                            let ty = self.types.get(operand)?;
                            let valid = if logical {
                                matches!(ty, T::Builtin(B::Boolean))
                            } else if shift
                                || matches!(
                                    b.operator,
                                    O::Percent | O::Ampersand | O::Pipe | O::Caret
                                )
                            {
                                super::value_typing::is_integral_type(ty)
                            } else if matches!(b.operator, O::DoubleEquals | O::Different) {
                                super::value_typing::numeric(ty)
                                    || matches!(ty, T::Builtin(B::Boolean | B::String | B::Void))
                                    || matches!(self.types.container(operand)?, (T::Pointer(_), _))
                            } else {
                                super::value_typing::numeric(ty)
                            };
                            if !valid {
                                return Err(type_error("invalid binary operand annotation"));
                            }
                        }
                        if comparison || logical {
                            if !matches!(self.types.get(info.ty)?, T::Builtin(B::Boolean)) {
                                return Err(type_error("binary result annotation mismatch"));
                            }
                        } else {
                            let expected = if self.types.is_symbolic(left.ty)? {
                                left.ty
                            } else if self.types.is_symbolic(right.ty)? {
                                right.ty
                            } else {
                                left.ty
                            };
                            if expected != info.ty {
                                return Err(type_error("binary result annotation mismatch"));
                            }
                        }
                        if !shift {
                            check_assignable(left.ty, right.ty, self.types)?;
                        }
                        if info.category != C::Value || info.type_value.is_some() {
                            return Err(type_error("binary value annotation mismatch"));
                        }
                    }
                }
                E::Call { callee, args } => {
                    let callable = self.expression(callee)?;
                    if matches!(self.types.get(callable.ty)?, T::IntrinsicFunction) {
                        let Some(id) = super::evaluation::function_id(callee) else {
                            return Err(type_error("intrinsic requires a named declaration"));
                        };
                        let builtin = self
                            .symbols
                            .compiler_declarations
                            .get(&id)
                            .copied()
                            .filter(|b| b.is_intrinsic())
                            .ok_or_else(|| type_error("invalid intrinsic call identity"))?;
                        let arguments = args
                            .iter()
                            .map(|arg| self.expression(arg))
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let expected =
                            super::value_typing::intrinsic_info(builtin, &arguments, self.types)?;
                        if info != expected {
                            return Err(type_error("intrinsic result annotation mismatch"));
                        }
                        return Ok(info);
                    }
                    let T::Function(mut signature) = self.types.get(callable.ty)?.clone() else {
                        return Err(type_error("call annotation has a non-function callee"));
                    };
                    if args.len() != signature.parameters.len() {
                        return Err(type_error("call annotation argument arity mismatch"));
                    }
                    let mut arguments = vec![];
                    for (arg, ty) in args.iter().zip(&signature.parameters) {
                        let value = self.expression(arg)?;
                        if matches!(self.types.get(*ty)?, T::Builtin(B::MetaType)) {
                            arguments.push(self.types.canonical(value.type_value.ok_or_else(
                                || type_error("generic argument has no denoted type"),
                            )?)?);
                        }
                    }
                    if !arguments.is_empty() {
                        let template = super::evaluation::function_id(callee)
                            .ok_or_else(|| type_error("missing generic call template"))?;
                        let key = super::generics::GenericInstanceKey {
                            template,
                            arguments,
                        };
                        let instance = self
                            .types
                            .generic_instances
                            .get(&key)
                            .ok_or_else(|| type_error("missing generic call instance"))?;
                        let T::Function(f) = self.types.get(instance.signature)? else {
                            return Err(type_error("invalid generic call signature"));
                        };
                        signature = f.clone();
                    }
                    if matches!(
                        self.types.get(signature.return_type)?,
                        T::Builtin(B::MetaType)
                    ) && (!signature.is_comp || super::evaluation::function_id(callee).is_none())
                    {
                        return Err(type_error(
                            "type computation requires a known comp function",
                        ));
                    }
                    if matches!(
                        self.types.get(signature.return_type)?,
                        T::Builtin(B::MetaType)
                    ) {
                        use crate::frontend::semantic_type::ComputationArgument as A;
                        let function = super::evaluation::function_id(callee)
                            .ok_or_else(|| type_error("missing computation identity"))?;
                        let arguments: Vec<_> = args
                            .iter()
                            .map(|arg| {
                                if let Some(t) = arg.annotation.resolved.and_then(|i| i.type_value)
                                {
                                    A::Type(t)
                                } else if let Some(b) = super::evaluation::boolean(
                                    arg,
                                    &std::collections::HashMap::new(),
                                ) {
                                    A::Bool(b)
                                } else {
                                    A::Expression(Box::new(arg.clone()))
                                }
                            })
                            .collect();
                        let symbolic = arguments.iter().any(|a| matches!(a, A::Expression(_)));
                        let result = info
                            .type_value
                            .ok_or_else(|| type_error("computed call has no denoted type"))?;
                        if symbolic {
                            if self.types.get(result)?
                                != &(T::TypeResult {
                                    function,
                                    arguments,
                                })
                            {
                                return Err(type_error(
                                    "symbolic computed call lost its argument dependencies",
                                ));
                            }
                        } else if self
                            .types
                            .computation_results
                            .get(&(function, arguments))
                            .copied()
                            != Some(result)
                        {
                            return Err(type_error(
                                "computed call result disagrees with retained computation",
                            ));
                        }
                    }
                    for (arg, ty) in args.iter().zip(&signature.parameters) {
                        check_assignable(*ty, self.expression(arg)?.ty, self.types)?;
                    }
                    let result_matches = if args
                        .iter()
                        .any(|a| super::value_typing::diverges(a, self.types))
                    {
                        matches!(self.types.get(info.ty)?, T::Builtin(B::Never))
                    } else {
                        info.ty == signature.return_type
                    };
                    if !result_matches {
                        return Err(type_error("call result annotation mismatch"));
                    }
                }
                E::Cast { expr, target_type } => {
                    let target = self.ty(target_type)?;
                    let source = self.contextual_expression(
                        expr,
                        super::value_typing::cast_context(expr, target, self.types),
                    )?;
                    if !super::value_typing::is_cast_allowed(source.ty, target, self.types)? {
                        return Err(type_error("unsupported cast annotation"));
                    }
                    super::constant::check_cast(expr, target, self.types)?;
                    if info.category != C::Value || info.type_value.is_some() {
                        return Err(type_error("cast value annotation mismatch"));
                    }
                    if target != info.ty && !super::value_typing::diverges(expr, self.types) {
                        return Err(type_error("cast result annotation mismatch"));
                    }
                }
                E::IndexAccess { object, index } => {
                    let receiver = self.expression(object)?;
                    if receiver.type_value.is_some() {
                        return Err(type_error("array types use prefix brackets: [N]T"));
                    }
                    self.expression(index)?;
                    if !super::value_typing::diverges(node, self.types) {
                        let (element, _, category) =
                            super::value_typing::indexed(receiver, self.types)?;
                        if info.ty != element || info.category != category {
                            return Err(type_error("index mutability annotation mismatch"));
                        }
                    }
                }
                E::SliceLiteral {
                    object,
                    start,
                    end,
                    inclusive: _,
                } => {
                    let receiver = self.expression(object)?;
                    if receiver.type_value.is_some() {
                        return Err(type_error("slice types use prefix brackets: []T"));
                    }
                    for bound in start.iter().chain(end) {
                        self.expression(bound)?;
                    }
                    if !super::value_typing::diverges(node, self.types) {
                        let (element, _, category) =
                            super::value_typing::indexed(receiver, self.types)?;
                        let (T::Slice(actual), readonly) = self.types.container(info.ty)? else {
                            return Err(type_error("slice annotation requires a slice"));
                        };
                        if *actual != element
                            || readonly != (category == C::ReadonlyPlace)
                            || info.category != C::Value
                        {
                            return Err(type_error("slice mutability annotation mismatch"));
                        }
                        let sentinel = if end.is_none() {
                            self.types.sentinel(receiver.ty)?
                        } else {
                            None
                        };
                        if self.types.sentinel(info.ty)? != sentinel {
                            return Err(type_error("slice sentinel annotation mismatch"));
                        }
                    }
                }
                E::ArrayLiteral { elements } => {
                    if super::value_typing::diverges(node, self.types) {
                        for element in elements {
                            self.expression(element)?;
                        }
                    } else {
                        let (T::Array { element, length }, _) = self.types.container(info.ty)?
                        else {
                            return Err(type_error("array literal annotation requires an array"));
                        };
                        if *length != elements.len() as u64
                            || info.category != C::Value
                            || info.type_value.is_some()
                        {
                            return Err(type_error("array literal annotation mismatch"));
                        }
                        for value in elements {
                            self.contextual_expression(value, Some(*element))?;
                        }
                    }
                }
                E::Literal(literal) => {
                    let ty = self.types.get(info.ty)?;
                    let valid = match literal {
                        Literal::Integer(_) => super::value_typing::numeric(ty),
                        Literal::Float(_) => matches!(
                            ty,
                            T::Builtin(B::Float16 | B::Float32 | B::Float64 | B::Float128)
                        ),
                        Literal::Boolean(_) => matches!(ty, T::Builtin(B::Boolean)),
                        Literal::Character(value) => {
                            u32::from(*value) <= 255 && matches!(ty, T::Builtin(B::UInt8))
                        }
                        Literal::String(value) => match self.types.container(info.ty)? {
                            (T::Array { element, length }, true) => {
                                *length == value.len() as u64
                                    && matches!(self.types.get(*element)?, T::Builtin(B::UInt8))
                            }
                            (T::Slice(element), true) => {
                                matches!(self.types.get(*element)?, T::Builtin(B::UInt8))
                            }
                            _ => false,
                        },
                        Literal::CString(_) => {
                            self.types.sentinel(info.ty)? == Some(0)
                                && matches!(self.types.container(info.ty)?, (T::Pointer(element), true) if matches!(self.types.get(*element)?, T::Builtin(B::UInt8)))
                        }
                        Literal::Null => {
                            self.types.sentinel(info.ty)?.is_none()
                                && matches!(
                                    self.types.container(info.ty)?.0,
                                    T::Pointer(_) | T::Function(_)
                                )
                        }
                    };
                    if !valid || info.category != C::Value || info.type_value.is_some() {
                        return Err(type_error("literal kind annotation mismatch"));
                    }
                }
                E::VoidExpression => {
                    if !matches!(self.types.get(info.ty)?, T::Builtin(B::Void))
                        || info.category != C::Value
                        || info.type_value.is_some()
                    {
                        return Err(type_error("void expression annotation mismatch"));
                    }
                }
            }
            if !negated_literal {
                super::constant::validate(node, info.ty, self.types)?;
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
        if !var.is_comp
            && let Some(info) = info
        {
            let category = if var.is_const {
                C::ImmutablePlace
            } else {
                C::MutablePlace
            };
            if matches!(self.types.get(info.ty)?, T::IntrinsicFunction) {
                return Err(type_error("intrinsic is not a runtime value"));
            }
            if info.category != category {
                return Err(type_error("binding mutability annotation mismatch"));
            }
        }
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
                let Some(BindingId::Declaration(id)) = fun.signature.annotation.binding else {
                    return Err(type_error("function has no declaration identity"));
                };
                let expected_parameters: Vec<_> = fun
                    .signature
                    .parameters
                    .iter()
                    .filter_map(|p| {
                        if matches!(
                            p.parameter_type
                                .annotation
                                .resolved
                                .and_then(|t| self.types.get(t).ok()),
                            Some(T::Builtin(B::MetaType))
                        ) {
                            match p.annotation.binding {
                                Some(BindingId::Local(id)) => Some(id),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                    .collect();
                if self.types.generic_parameters.get(&id) != Some(&expected_parameters) {
                    return Err(type_error(
                        "generic parameter table disagrees with function signature",
                    ));
                }
                let T::Function(signature) = self.types.get(info.ty)? else {
                    return Err(type_error("function binding is not callable"));
                };
                if signature.is_comp {
                    let body = fun
                        .body
                        .as_ref()
                        .ok_or_else(|| type_error("comp function requires a body"))?;
                    if fun.is_extern {
                        return Err(type_error("comp function must be non-extern"));
                    }
                    if matches!(
                        self.types.get(signature.return_type)?,
                        T::Builtin(B::MetaType)
                    ) {
                        super::evaluation::validate_comp_body(&body.statements)?;
                    }
                }
                if signature.calling_convention != fun.signature.calling_convention
                    || signature.is_comp != fun.signature.is_comp
                    || signature.parameters.len() != fun.signature.parameters.len()
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
                    self.with_return_type(signature.return_type)
                        .statements(&body.statements)?;
                }
            }
            S::ExpressionStatement(expr) => {
                self.expression(expr)?;
            }
            S::Assignment { left, right } => {
                let target = self.expression(left)?;
                super::statement::validate_assignment_target(&target)?;
                check_assignable(target.ty, self.expression(right)?.ty, self.types)?;
            }
            S::DerefAssign { pointer, expr } => {
                let pointer = self.expression(pointer)?;
                let (T::Pointer(pointee), false) = self.types.container(pointer.ty)? else {
                    return Err(type_error("assignment requires a mutable pointer"));
                };
                check_assignable(*pointee, self.expression(expr)?.ty, self.types)?;
            }
            S::IndexAssign {
                object,
                index,
                expr,
            } => {
                let receiver = self.expression(object)?;
                self.expression(index)?;
                let (element, _, category) = super::value_typing::indexed(receiver, self.types)?;
                if category != C::MutablePlace {
                    return Err(type_error("assignment requires mutable elements"));
                }
                check_assignable(element, self.expression(expr)?.ty, self.types)?;
            }
            S::MultiAssignment { targets, values } => {
                for target in targets {
                    super::statement::validate_assignment_target(&self.expression(target)?)?;
                }
                for value in values {
                    self.expression(value)?;
                }
            }
            S::Return(values) => {
                let expected = self
                    .return_type
                    .ok_or_else(|| type_error("return outside function"))?;
                let values = values.as_deref().unwrap_or(&[]);
                if values.is_empty() {
                    if !matches!(
                        self.types.get(self.types.canonical(expected)?)?,
                        T::Builtin(B::Void)
                    ) && !self.types.is_symbolic(expected)?
                    {
                        return Err(type_error("empty return requires void return type"));
                    }
                } else if values.len() == 1 {
                    self.contextual_expression(&values[0], Some(expected))?;
                } else {
                    let T::Tuple(elements) = self.types.get(expected)? else {
                        return Err(type_error("return arity mismatch"));
                    };
                    if elements.len() != values.len() {
                        return Err(type_error("return arity mismatch"));
                    }
                    for (value, ty) in values.iter().zip(elements) {
                        self.contextual_expression(value, Some(*ty))?;
                    }
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
                super::statement_typing::validate_switch(owner, arms, self.types)?;
                for arm in arms {
                    if let Pattern::EnumVariant {
                        variant,
                        member,
                        binding,
                    } = &arm.pattern
                    {
                        self.selected(*member, owner, variant)?;
                        if let Some(binding) = binding {
                            let info = self.binding(binding.annotation)?.ok_or_else(|| {
                                type_error("switch payload binding requires a value")
                            })?;
                            let T::Enum(variants) = self.types.get(owner)? else {
                                unreachable!()
                            };
                            let MemberKind::EnumVariant(index) =
                                member.resolved.expect("validated variant").kind
                            else {
                                unreachable!()
                            };
                            let expected =
                                self.types.enum_payload(variants[index as usize].payload)?;
                            let correct_type = match expected {
                                Some(payload) => info.ty == payload,
                                None => matches!(self.types.get(info.ty)?, T::Builtin(B::Void)),
                            };
                            if !correct_type
                                || info.category != C::MutablePlace
                                || info.type_value.is_some()
                            {
                                return Err(type_error("switch payload binding type mismatch"));
                            }
                        }
                    }
                    self.statements(&arm.body)?;
                }
            }
            S::Defer(nodes) => self.statements(nodes)?,
            S::Break | S::Continue => {}
        }
        Ok(())
    }
}
