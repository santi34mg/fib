//! Type-resolution state and declaration dependencies. Recipes are read-only
//! source snapshots; all successful annotations are written to the original AST.
use super::{TypeContext, at_span};
use crate::{
    CompilationUnit,
    diagnostics::Span,
    frontend::{
        analysis_annotation::{BindingId, ExpressionInfo, MemberInfo, MemberKind, ValueCategory},
        ast::{
            expression::ExpressionKind as E,
            function_signature::FunctionSignature,
            statement::{Statement, StatementKind as S},
            structl_field::StructField,
            type_expression::{TypeExpression, TypeExpressionKind as K},
            variable_declaration::VariableDeclaration,
        },
        identifier::{DeclId, Entity, ResolvedName},
        namespace::ProjectSymbols,
        semantic_type::{
            FieldType, FunctionType, SemanticType as T, SemanticTypes, TypeId, VariantType,
            type_error,
        },
        tokens::builtin::BuiltinType as B,
    },
    plan::FileId,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone)]
pub(super) enum Definition {
    Variable(VariableDeclaration),
    Function(FunctionSignature),
    Multi(Statement),
}
#[derive(Clone, Default)]
pub(super) struct TypeState {
    // Immutable source recipes are shared across recovery checkpoints. Mutable
    // inference state below must still be restored when a statement fails.
    pub definitions: Arc<HashMap<DeclId, Definition>>,
    pub resolving: HashSet<DeclId>,
    pub sites: HashMap<(FileId, Span), TypeId>,
    pub return_type: Option<TypeId>,
    pub comp_function: bool,
    pub depth: usize,
    pub paths: Arc<HashMap<FileId, std::path::PathBuf>>,
    pub functions:
        Arc<HashMap<DeclId, crate::frontend::ast::function_declaration::FunctionDeclaration>>,
    pub evaluation_depth: usize,

    pub evaluation_booleans: HashMap<BindingId, bool>,
    pub in_defer: bool,
    pub loop_depth: usize,
    pub specialization: HashMap<crate::frontend::identifier::LocalId, TypeId>,
    pub active_instances: HashMap<super::generics::GenericInstanceKey, TypeId>,
}

pub(super) fn resolve_project(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &mut SemanticTypes,
) -> anyhow::Result<()> {
    resolve_project_with_paths(units, symbols, types, HashMap::new())
}

pub(super) fn resolve_project_with_paths(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &mut SemanticTypes,
    paths: HashMap<FileId, std::path::PathBuf>,
) -> anyhow::Result<()> {
    resolve_project_collecting(units, symbols, types, paths, None)
}

pub(super) fn resolve_project_collecting(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &mut SemanticTypes,
    paths: HashMap<FileId, std::path::PathBuf>,
    recovery: Option<super::Recovery>,
) -> anyhow::Result<()> {
    if types.started {
        return Err(type_error(
            "type resolution may only run once; failed passes cannot be retried",
        ));
    }
    types.started = true;
    if let Some(prelude) = symbols
        .namespaces
        .get(&crate::frontend::namespace::PRELUDE_FILE)
    {
        let meta = types.builtin(B::MetaType);
        for (index, declaration) in prelude.declarations.iter().enumerate() {
            let ty = types.builtin(B::from_name(&declaration.name).expect("prelude type"));
            types.bind(
                BindingId::Declaration(DeclId {
                    file_id: prelude.file_id,
                    index: index as u32,
                }),
                ExpressionInfo {
                    ty: meta,
                    category: ValueCategory::Value,
                    type_value: Some(ty),
                },
            );
        }
    }
    if recovery.is_none() {
        for unit in units.iter_mut() {
            super::validate_names(&mut unit.ast)?;
        }
    }
    let mut context = TypeContext::new(FileId(0), symbols, types);
    context.state.paths = Arc::new(paths);
    context.recovery = recovery;
    fn collect(nodes: &[Statement], definitions: &mut HashMap<DeclId, Definition>) {
        fn variable(
            var: &VariableDeclaration,
            value: Option<&crate::frontend::ast::expression::Expression>,
            definitions: &mut HashMap<DeclId, Definition>,
        ) {
            if let Some(BindingId::Declaration(id)) = var.annotation.binding {
                let mut recipe = var.clone();
                if let Some(value) = value {
                    recipe.expression = Some(value.clone());
                }
                definitions.insert(id, Definition::Variable(recipe));
            }
        }
        for node in nodes {
            match &node.kind {
                S::VariableDeclaration(v) => variable(v, None, definitions),
                S::MultiVariableDeclaration { bindings, values } => {
                    for (i, v) in bindings.iter().enumerate() {
                        if !values.is_empty() && values.len() != bindings.len() {
                            if let Some(BindingId::Declaration(id)) = v.annotation.binding {
                                definitions.insert(id, Definition::Multi(node.clone()));
                            }
                        } else {
                            variable(v, values.get(i), definitions);
                        }
                    }
                }
                S::FunctionDeclaration(f) => {
                    if let Some(BindingId::Declaration(id)) = f.signature.annotation.binding {
                        definitions.insert(id, Definition::Function(f.signature.clone()));
                    }
                    if let Some(body) = &f.body {
                        collect(&body.statements, definitions);
                    }
                }
                S::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect(then_branch, definitions);
                    if let Some(b) = else_branch {
                        collect(b, definitions);
                    }
                }
                S::While { body, .. } => collect(body, definitions),
                S::Switch { arms, .. } => {
                    for arm in arms {
                        collect(&arm.body, definitions);
                    }
                }
                S::Defer(node) => collect(node, definitions),
                _ => {}
            }
        }
    }
    let mut definitions = HashMap::new();
    let mut functions = HashMap::new();
    for unit in units.iter() {
        collect(&unit.ast.statements, &mut definitions);
        for node in &unit.ast.statements {
            if let S::FunctionDeclaration(fun) = &node.kind
                && let Some(BindingId::Declaration(id)) = fun.signature.annotation.binding
            {
                functions.insert(id, fun.clone());
            }
        }
    }
    context.state.definitions = Arc::new(definitions);
    context.state.functions = Arc::new(functions);
    // Function signatures and named type definitions are available before bodies.
    let mut ids: Vec<_> = context.state.definitions.keys().copied().collect();
    ids.sort_by_key(|id| (id.file_id.0, id.index));
    for id in ids {
        if (matches!(&context.state.definitions[&id], Definition::Function(..))
            || symbols.declaration(id)?.kind == crate::frontend::namespace::DeclarationKind::Type)
            && let Err(error) = context.ensure_declaration(id)
        {
            if let Some(recovery) = &context.recovery {
                recovery.block_binding(BindingId::Declaration(id));
                recovery.record(
                    error,
                    id.file_id,
                    symbols.declaration(id)?.span,
                    context.state.paths.get(&id.file_id),
                );
            } else {
                return Err(error);
            }
        }
    }
    for unit in units.iter_mut() {
        context.file = unit.source_file_id;
        context.statements(&mut unit.ast.statements)?;
    }
    if let Err(error) = context.validate_layouts() {
        if let Some(recovery) = &context.recovery {
            recovery.record(
                error,
                context.file,
                Span::new(1, 1),
                context.state.paths.get(&context.file),
            );
        } else {
            return Err(error);
        }
    }
    if context.recovery.as_ref().is_none_or(|r| !r.has_errors()) {
        validate_annotations(units, symbols, context.types)?;
        context.types.complete = true;
    }
    Ok(())
}

impl TypeContext<'_> {
    pub(super) fn value(&mut self, ty: TypeId) -> ExpressionInfo {
        ExpressionInfo {
            ty,
            category: ValueCategory::Value,
            type_value: None,
        }
    }
    pub(super) fn metatype(&mut self, ty: TypeId) -> ExpressionInfo {
        ExpressionInfo {
            ty: self.types.builtin(B::MetaType),
            category: ValueCategory::Value,
            type_value: Some(ty),
        }
    }
    pub(super) fn ensure_declaration(&mut self, id: DeclId) -> anyhow::Result<()> {
        if self
            .recovery
            .as_ref()
            .is_some_and(|r| r.blocked(BindingId::Declaration(id)))
        {
            return Err(super::recovery::Blocked.into());
        }
        if self
            .types
            .bindings
            .contains_key(&BindingId::Declaration(id))
        {
            return Ok(());
        }
        if !self.state.resolving.insert(id) {
            if self
                .types
                .type_values
                .contains_key(&BindingId::Declaration(id))
            {
                return Ok(());
            }
            return Err(type_error("type inference dependency cycle"));
        }
        let recipe = self
            .state
            .definitions
            .get(&id)
            .cloned()
            .ok_or_else(|| type_error(format!("missing type definition for {id:?}")))?;
        let previous = self.file;
        self.file = id.file_id;
        let result = match recipe {
            Definition::Variable(mut v) => {
                if v.is_comp
                    && let Some(expr) = &v.expression
                    && let E::TypeValue(ty) = &expr.kind
                    && matches!(ty.kind, K::Struct { .. } | K::Enum { .. })
                {
                    let key = (self.file, ty.span);
                    let owner = *self
                        .state
                        .sites
                        .entry(key)
                        .or_insert_with(|| self.types.intern(T::Pending));
                    self.types
                        .type_values
                        .insert(BindingId::Declaration(id), owner);
                }
                self.variable(&mut v)
            }
            Definition::Function(mut signature) => self.signature(&mut signature).map(|_| ()),
            Definition::Multi(mut statement) => self.statement(&mut statement),
        };
        self.file = previous;
        self.state.resolving.remove(&id);
        result.map_err(|e| {
            self.diagnostic(
                e,
                self.symbols.declaration(id).ok().map(|d| d.span),
                id.file_id,
            )
        })
    }
    pub(super) fn diagnostic(
        &self,
        error: anyhow::Error,
        span: Option<Span>,
        file: FileId,
    ) -> anyhow::Error {
        let error = if let Some(span) = span {
            at_span(error, span)
        } else {
            error
        };
        match error.downcast::<crate::diagnostics::CompilerError>() {
            Ok(mut error) => {
                if error.filename.is_none() {
                    error.filename = self.state.paths.get(&file).cloned();
                }
                error.into()
            }
            Err(error) => error,
        }
    }
    pub(super) fn name_info(&mut self, name: ResolvedName) -> anyhow::Result<ExpressionInfo> {
        let binding = match name {
            ResolvedName::Local(id) => BindingId::Local(id),
            ResolvedName::Entity(Entity::Declaration(id)) => {
                self.ensure_declaration(id)?;
                BindingId::Declaration(id)
            }
            ResolvedName::Entity(Entity::File(_)) => {
                return Err(type_error("file namespace is not a value or type"));
            }
        };
        if self.recovery.as_ref().is_some_and(|r| r.blocked(binding)) {
            return Err(super::recovery::Blocked.into());
        }
        if let Some(info) = self.types.bindings.get(&binding) {
            return Ok(*info);
        }
        if let Some(&ty) = self.types.type_values.get(&binding) {
            return Ok(self.metatype(ty));
        }
        Err(type_error(format!("missing type for binding {binding:?}")))
    }
    pub(super) fn signature(
        &mut self,
        signature: &mut FunctionSignature,
    ) -> anyhow::Result<TypeId> {
        for p in &signature.parameters {
            if self
                .symbols
                .is_metatype_syntax(self.file, &p.parameter_type)
            {
                let Some(BindingId::Local(id)) = p.annotation.binding else {
                    return Err(type_error("pending parameter binding"));
                };
                let denoted = if let Some(ty) = self.state.specialization.get(&id) {
                    *ty
                } else {
                    self.types.intern(T::Parameter(id))
                };
                let info = self.metatype(denoted);
                self.types.bind(BindingId::Local(id), info);
            }
        }
        let mut parameters = Vec::new();
        for p in &mut signature.parameters {
            let ty = self.type_expression(&mut p.parameter_type, true, false)?;
            let id = p
                .annotation
                .binding
                .ok_or_else(|| type_error("pending parameter binding"))?;
            if !matches!(self.types.get(ty)?, T::Builtin(B::MetaType)) {
                self.types.bind(
                    id,
                    ExpressionInfo {
                        ty,
                        category: ValueCategory::MutablePlace,
                        type_value: None,
                    },
                );
            }
            parameters.push(ty);
        }
        let return_type = self.type_expression(&mut signature.return_type, true, true)?;
        let ty = self.types.intern(T::Function(FunctionType {
            is_comp: signature.is_comp,
            calling_convention: signature.calling_convention,
            parameters,
            return_type,
        }));
        let binding = signature
            .annotation
            .binding
            .ok_or_else(|| type_error("pending function binding"))?;
        if let BindingId::Declaration(id) = binding {
            let parameters = signature
                .parameters
                .iter()
                .filter_map(|p| {
                    if self
                        .symbols
                        .is_metatype_syntax(self.file, &p.parameter_type)
                    {
                        match p.annotation.binding {
                            Some(BindingId::Local(id)) => Some(id),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .collect();
            self.types.generic_parameters.insert(id, parameters);
        }
        if self.state.specialization.is_empty() {
            let info = self.value(ty);
            self.types.bind(binding, info);
        }
        Ok(ty)
    }
    pub(super) fn type_expression(
        &mut self,
        node: &mut TypeExpression,
        allow_meta: bool,
        allow_never: bool,
    ) -> anyhow::Result<TypeId> {
        let result = self.type_expression_inner(node, allow_meta, allow_never);
        result.map_err(|e| self.diagnostic(e, Some(node.span), self.file))
    }
    fn type_expression_inner(
        &mut self,
        node: &mut TypeExpression,
        allow_meta: bool,
        allow_never: bool,
    ) -> anyhow::Result<TypeId> {
        // Revisit cached syntax to validate position-specific restrictions as well.
        let ty = match &mut node.kind {
            K::Computed(expression) => self
                .expression(expression, None)?
                .type_value
                .ok_or_else(|| type_error("computed type annotation requires a type value"))?,
            K::Builtin(b) => self.types.builtin(b.clone()),
            K::QualifiedIdentifier(name) => {
                let info = self.name_info(
                    name.annotation
                        .resolved
                        .ok_or_else(|| type_error("pending type name"))?,
                )?;
                info.type_value.ok_or_else(|| {
                    type_error(format!(
                        "value '{}' used as a type",
                        name.segments.join("::")
                    ))
                })?
            }
            K::Readonly(inner) => {
                let t = self.type_expression(inner, false, false)?;
                if !matches!(
                    self.types.get(t)?,
                    T::Pointer(_) | T::Slice(_) | T::Array { .. }
                ) {
                    return Err(type_error(
                        "const type qualifier requires a pointer, slice or array",
                    ));
                }
                self.types.intern(T::Readonly(t))
            }
            K::Pointer { pointed_type } => {
                let t = self.type_expression(pointed_type, false, false)?;
                self.types.intern(T::Pointer(t))
            }
            K::Array { element_type, size } => {
                let element = self.type_expression(element_type, false, false)?;
                self.types.intern(T::Array {
                    element,
                    length: *size,
                })
            }
            K::Slice { element_type } => {
                let t = self.type_expression(element_type, false, false)?;
                self.types.intern(T::Slice(t))
            }
            K::Tuple { elements } => {
                let ts = elements
                    .iter_mut()
                    .map(|t| self.type_expression(t, false, false))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                self.types.intern(T::Tuple(ts))
            }
            K::Function {
                calling_convention,
                is_comp,
                argument_types,
                return_type,
            } => {
                let parameters = argument_types
                    .iter_mut()
                    .map(|t| self.type_expression(t, true, false))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let return_type = self.type_expression(return_type, true, true)?;
                self.types.intern(T::Function(FunctionType {
                    is_comp: *is_comp,
                    calling_convention: *calling_convention,
                    parameters,
                    return_type,
                }))
            }
            K::Struct { fields } => {
                let owner = self.aggregate_site(node.span);
                let fs = self.field_definitions(fields, owner)?;
                self.types.types[owner.0 as usize] = T::Struct(fs);
                owner
            }
            K::Enum { variants } => {
                let owner = self.aggregate_site(node.span);
                let mut seen = HashSet::new();
                let mut vs = Vec::new();
                for v in variants.iter_mut() {
                    if !seen.insert(v.identifier.clone()) {
                        return Err(type_error(format!("duplicate variant '{}'", v.identifier)));
                    }
                    let payload = if let Some(expression) = &mut v.payload {
                        let payload = self
                            .expression(expression, None)?
                            .type_value
                            .ok_or_else(|| type_error("enum payload requires a type value"))?;
                        if matches!(
                            self.types.types.get(payload.0 as usize),
                            Some(T::Builtin(B::Never | B::MetaType))
                        ) {
                            return Err(type_error(
                                "never/type cannot be stored in an enum payload",
                            ));
                        }
                        Some(payload)
                    } else {
                        None
                    };
                    vs.push(VariantType {
                        spelling: v.identifier.clone(),
                        payload,
                    });
                }
                self.types.types[owner.0 as usize] = T::Enum(vs);
                self.types.refresh_variant_constructors(owner)?;
                owner
            }
        };
        if let Some(T::Builtin(b)) = self.types.types.get(ty.0 as usize) {
            if *b == B::Never && !allow_never {
                return Err(type_error("never is only permitted as a return type"));
            }
            if *b == B::MetaType && !allow_meta {
                return Err(type_error(
                    "type requires a comp binding, parameter or return position",
                ));
            }
        }
        node.annotation.resolved = Some(ty);
        Ok(ty)
    }
    fn aggregate_site(&mut self, span: Span) -> TypeId {
        let id = *self
            .state
            .sites
            .entry((self.file, span))
            .or_insert_with(|| self.types.intern(T::Pending));
        self.types.nominal_sites.insert(id, (self.file, span));
        let mut arguments: Vec<_> = self.state.specialization.iter().collect();
        arguments.sort_by_key(|(binding, _)| binding.0);
        self.types
            .nominal_arguments
            .insert(id, arguments.into_iter().map(|(_, ty)| *ty).collect());
        id
    }
    fn field_definitions(
        &mut self,
        fields: &mut [StructField],
        owner: TypeId,
    ) -> anyhow::Result<Vec<FieldType>> {
        let mut seen = HashSet::new();
        fields
            .iter_mut()
            .enumerate()
            .map(|(index, field)| {
                if !seen.insert(field.field_identifier.clone()) {
                    return Err(at_span(
                        type_error(format!("duplicate field '{}'", field.field_identifier)),
                        field.expression.span,
                    ));
                }
                let info = self.expression(&mut field.expression, None)?;
                let ty = info
                    .type_value
                    .ok_or_else(|| type_error("field definition requires a type value"))?;
                if matches!(
                    self.types.types.get(ty.0 as usize),
                    Some(T::Builtin(B::Never | B::MetaType))
                ) {
                    return Err(type_error("never/type cannot be stored in a field"));
                }
                field.annotation.resolved = Some(MemberInfo {
                    owner,
                    kind: MemberKind::Field(index as u32),
                    ty,
                });
                Ok(FieldType {
                    spelling: field.field_identifier.clone(),
                    ty,
                })
            })
            .collect()
    }
    fn validate_layouts(&self) -> anyhow::Result<()> {
        let mut done = HashSet::new();
        let mut invalid = HashSet::new();
        for index in 0..self.types.types.len() {
            let mut active = HashSet::new();
            let mut stack = vec![(TypeId(index as u32), false)];
            while let Some((id, exit)) = stack.pop() {
                if exit {
                    active.remove(&id);
                    done.insert(id);
                    continue;
                }
                if invalid.contains(&id) {
                    invalid.extend(active.iter().copied());
                    done.extend(active);
                    break;
                }
                if done.contains(&id) {
                    continue;
                }
                if !active.insert(id) {
                    let error = type_error("illegal by-value type size cycle");
                    let diagnostic = if let Some(((file, span), _)) = self
                        .state
                        .sites
                        .iter()
                        .filter(|(_, ty)| **ty == id)
                        .min_by_key(|((file, span), _)| (file.0, span.line, span.column))
                    {
                        self.diagnostic(error, Some(*span), *file)
                    } else {
                        error
                    };
                    if let Some(recovery) = &self.recovery {
                        recovery.record_global(
                            diagnostic,
                            self.file,
                            Span::new(1, 1),
                            self.state.paths.get(&self.file),
                        );
                        invalid.extend(active.iter().copied());
                        done.extend(active);
                        break;
                    }
                    return Err(diagnostic);
                }
                stack.push((id, true));
                let children: Vec<_> = match self.types.get(id)? {
                    T::Struct(fs) => fs.iter().map(|f| f.ty).collect(),
                    T::Enum(vs) => vs.iter().filter_map(|v| v.payload).collect(),
                    T::Readonly(t) => vec![*t],
                    T::Array { element, .. } => vec![*element],
                    T::Tuple(ts) => ts.clone(),
                    _ => vec![],
                };
                stack.extend(children.into_iter().map(|id| (id, false)));
            }
        }
        if let Some(recovery) = &self.recovery {
            for (&binding, info) in &self.types.bindings {
                if self
                    .types
                    .canonical(info.ty)
                    .is_ok_and(|ty| invalid.contains(&ty))
                    || info.type_value.is_some_and(|ty| {
                        self.types
                            .canonical(ty)
                            .is_ok_and(|ty| invalid.contains(&ty))
                    })
                {
                    recovery.block_binding(binding);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn validate_annotations(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    super::validation::validate(units, symbols, types)
}
