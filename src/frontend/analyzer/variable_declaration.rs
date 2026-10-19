use super::{ResolveContext, TypeContext, inline_type, not_implemented};
use crate::frontend::{
    analysis_annotation::BindingId,
    analyzer::Analyzer,
    ast::{
        expression::{Expression, ExpressionKind},
        type_expression::TypeExpressionKind,
        variable_declaration::VariableDeclaration,
    },
    namespace::{Entity, ResolvedName, name_error},
    symbol_table::LocalBinding,
    tokens::builtin::BuiltinType,
};
use std::collections::HashSet;

impl Analyzer {
    pub fn resolve_name_variable_declaration(
        var: &mut VariableDeclaration,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        Self::check_local_name(&var.identifier, var.is_comp, context)?;
        if let Some(ty) = &mut var.ty {
            ty.resolve_name(context)?;
        }
        if let Some(expr) = &mut var.expression {
            Self::resolve_name_expression(expr, context)?;
        }
        let target = Self::check_binding(var, var.expression.as_ref(), context)?;
        Self::insert_local(var, target, context)
    }

    pub(super) fn resolve_multi_bindings(
        bindings: &mut [VariableDeclaration],
        values: &mut [Expression],
        context: &mut ResolveContext<'_>,
        file: bool,
    ) -> anyhow::Result<()> {
        let mut names = HashSet::new();
        for binding in bindings.iter_mut() {
            if !file {
                Self::check_local_name(&binding.identifier, binding.is_comp, context)?;
                if !names.insert(&binding.identifier) {
                    return Err(name_error(format!(
                        "duplicate identifier '{}'",
                        binding.identifier
                    )));
                }
            }
            if let Some(ty) = &mut binding.ty {
                ty.resolve_name(context)?;
            }
        }
        for value in values.iter_mut() {
            Self::resolve_name_expression(value, context)?;
        }
        if bindings.iter().any(|b| b.is_comp) && bindings.len() != values.len() {
            return Err(name_error(
                "comp multi-bindings require one initializer per binding",
            ));
        }
        // A shared runtime initializer's arity is decided by typing, not lookup.
        let targets = bindings
            .iter()
            .enumerate()
            .map(|(i, binding)| {
                Self::check_binding(
                    binding,
                    if values.len() == bindings.len() {
                        values.get(i)
                    } else {
                        None
                    },
                    context,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        if !file {
            for (binding, target) in bindings.iter_mut().zip(targets) {
                Self::insert_local(binding, target, context)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_local_name(
        name: &str,
        alias: bool,
        context: &ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        if context.lexical.resolve_lexical(name).is_some() {
            return Err(name_error(format!("duplicate lexical identifier '{name}'")));
        }
        if alias && context.symbols.reserved_file_root(name) {
            return Err(name_error(format!(
                "local alias '{name}' cannot reuse a build-plan file name or file root"
            )));
        }
        Ok(())
    }

    /// A compile-time reference aliases an entity; an inline type owns an identity.
    /// Runtime bindings cannot store a statically recognized type value.
    pub(super) fn check_binding(
        var: &VariableDeclaration,
        expr: Option<&Expression>,
        context: &ResolveContext<'_>,
    ) -> anyhow::Result<Option<Entity>> {
        let type_annotation = matches!(
            var.ty.as_ref().map(|t| &t.kind),
            Some(TypeExpressionKind::Builtin(BuiltinType::MetaType))
        );
        let reference = match expr.map(|e| &e.kind) {
            Some(ExpressionKind::QualifiedIdentifier(name)) => name.annotation.resolved,
            _ => None,
        };
        let type_reference = matches!(reference, Some(ResolvedName::Entity(entity)) if context.symbols.is_type(entity));
        if !var.is_comp {
            if type_annotation || inline_type(expr) || type_reference {
                return Err(name_error(format!(
                    "type binding '{}' requires explicit comp",
                    var.identifier
                )));
            }
            return Ok(None);
        }
        if inline_type(expr) {
            let Some(BindingId::Declaration(id)) = var.annotation.binding else {
                return Err(name_error(format!(
                    "missing compile-time type identity for '{}'",
                    var.identifier
                )));
            };
            return Ok(Some(Entity::Declaration(id)));
        }
        match reference {
            Some(ResolvedName::Entity(entity)) => Ok(Some(entity)),
            Some(ResolvedName::Local(_)) => Err(name_error(format!(
                "comp '{}' cannot alias runtime local or parameter '{}'",
                var.identifier,
                match &expr.unwrap().kind {
                    ExpressionKind::QualifiedIdentifier(name) => name.segments.join("::"),
                    _ => unreachable!(),
                }
            ))),
            None => Err(name_error(format!(
                "comp '{}' requires an entity reference or inline type initializer",
                var.identifier
            ))),
        }
    }

    fn insert_local(
        var: &mut VariableDeclaration,
        target: Option<Entity>,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        let binding = match target {
            Some(entity) => {
                if !matches!(var.annotation.binding, Some(BindingId::Declaration(_))) {
                    var.annotation.binding = Some(BindingId::Local(context.local_ids.allocate()?));
                }
                LocalBinding::CompAlias(entity)
            }
            None => {
                let id = context.local_ids.allocate()?;
                var.annotation.binding = Some(BindingId::Local(id));
                LocalBinding::Local(id)
            }
        };
        context
            .lexical
            .insert_binding(var.identifier.clone(), binding)
    }

    pub fn resolve_type_variable_declaration(
        &mut self,
        _context: &mut TypeContext<'_>,
    ) -> anyhow::Result<()> {
        not_implemented("variable declaration type resolution")
    }
}
