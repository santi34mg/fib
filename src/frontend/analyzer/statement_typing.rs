//! Binding and statement typing. Initialization/flow completion remains separate.
use super::{
    TypeContext,
    statement::{resolve_multi_value_types, validate_assignment_target},
    types::check_assignable,
};
use crate::{
    diagnostics::Span,
    frontend::{
        analysis_annotation::{BindingId, ExpressionInfo, ValueCategory as C},
        ast::{
            expression::{Expression, ExpressionKind as E},
            pattern::Pattern,
            statement::{Statement, StatementKind as S},
            switch::SwitchArm,
            variable_declaration::VariableDeclaration,
        },
        identifier::{Entity, ResolvedName},
        semantic_type::{SemanticType as T, TypeId, type_error},
        tokens::builtin::BuiltinType as B,
    },
};

impl TypeContext<'_> {
    pub(super) fn variable(&mut self, var: &mut VariableDeclaration) -> anyhow::Result<()> {
        let binding = var
            .annotation
            .binding
            .ok_or_else(|| type_error("pending variable binding"))?;
        let expected = var
            .ty
            .as_mut()
            .map(|t| self.type_expression(t, var.is_comp, false))
            .transpose()?;
        // Namespaces are compile-time entities, not runtime expressions.
        if var.is_comp
            && let Some(expr) = &var.expression
            && let E::QualifiedIdentifier(name) = &expr.kind
            && matches!(
                name.annotation.resolved,
                Some(ResolvedName::Entity(Entity::File(_)))
            )
        {
            if expected.is_some() {
                return Err(type_error("file namespace cannot have a value type"));
            }
            return Ok(());
        }
        // Explicit value types are available independently of initialization.
        // This permits typed dependency cycles without certifying legal reads.
        if !var.is_comp
            && let Some(ty) = expected
        {
            let info = self.value(ty);
            self.store_binding(binding, false, var.is_const, info)?;
        }
        let info = if let Some(expr) = &mut var.expression {
            let info = self.expression(expr, expected)?;
            if var.is_comp && matches!(expr.kind, E::Call { .. }) && info.type_value.is_none() {
                return Err(type_error(
                    "computed comp initializer must produce a type value",
                ));
            }
            if let Some(expected) = expected
                && matches!(self.types.get(info.ty)?, T::Builtin(B::Never))
            {
                self.value(expected)
            } else {
                info
            }
        } else {
            let ty =
                expected.ok_or_else(|| type_error("binding requires a type or initializer"))?;
            self.value(ty)
        };
        self.store_binding(binding, var.is_comp, var.is_const, info)
    }
    fn store_binding(
        &mut self,
        binding: BindingId,
        comp: bool,
        immutable: bool,
        mut info: ExpressionInfo,
    ) -> anyhow::Result<()> {
        if !comp
            && (info.type_value.is_some()
                || matches!(self.types.get(info.ty)?, T::Builtin(B::MetaType)))
        {
            return Err(type_error("type value requires a comp binding"));
        }
        if matches!(self.types.get(info.ty)?, T::Builtin(B::Never)) {
            return Err(type_error("never cannot be stored in a binding"));
        }
        if !comp {
            info.category = if immutable {
                C::ImmutablePlace
            } else {
                C::MutablePlace
            };
        }
        self.types.bind(binding, info);
        Ok(())
    }
    pub(super) fn statements(&mut self, nodes: &mut [Statement]) -> anyhow::Result<()> {
        for node in nodes {
            self.statement(node)?;
        }
        Ok(())
    }
    pub(super) fn statement(&mut self, node: &mut Statement) -> anyhow::Result<()> {
        self.statement_inner(node)
            .map_err(|e| self.diagnostic(e, Some(node.span), self.file))
    }
    fn statement_inner(&mut self, node: &mut Statement) -> anyhow::Result<()> {
        if self.state.in_defer
            && matches!(
                node.kind,
                S::Defer(_) | S::Return(_) | S::Break | S::Continue
            )
        {
            return Err(type_error(
                "defer cannot contain nested defer or control transfer",
            ));
        }
        match &mut node.kind {
            S::VariableDeclaration(var) => self.variable(var)?,
            S::MultiVariableDeclaration { bindings, values } => {
                if values.is_empty() {
                    for v in bindings {
                        self.variable(v)?;
                    }
                    return Ok(());
                }
                if values.len() == bindings.len() {
                    for (v, value) in bindings.iter_mut().zip(values) {
                        // Borrow the shared initializer without storing a second AST copy.
                        let previous = v.expression.replace(value.clone());
                        let result = self.variable(v);
                        *value = v.expression.take().unwrap();
                        v.expression = previous;
                        result?;
                    }
                } else {
                    let infos = values
                        .iter_mut()
                        .map(|v| self.expression(v, None))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    let result_types = resolve_multi_value_types(
                        bindings.len(),
                        &infos.iter().map(|i| i.ty).collect::<Vec<_>>(),
                        self.types,
                    )?;
                    for (v, ty) in bindings.iter_mut().zip(result_types) {
                        if v.is_comp {
                            return Err(type_error(
                                "comp bindings require one initializer per binding",
                            ));
                        }
                        if let Some(node) = &mut v.ty {
                            let expected = self.type_expression(node, false, false)?;
                            check_assignable(expected, ty, self.types)?;
                        }
                        let binding = v
                            .annotation
                            .binding
                            .ok_or_else(|| type_error("pending binding"))?;
                        let info = self.value(ty);
                        self.store_binding(binding, false, v.is_const, info)?;
                    }
                }
            }
            S::FunctionDeclaration(fun) => {
                let signature = self.signature(&mut fun.signature)?;
                fun.resolve_type(self, signature)?;
            }
            S::ExpressionStatement(expr) => {
                self.expression(expr, None)?;
            }
            S::Assignment { left, right } => {
                let target = self.expression(left, None)?;
                validate_assignment_target(&target)?;
                self.expression(right, Some(target.ty))?;
            }
            S::DerefAssign { pointer, expr } => {
                let p = self.expression(pointer, None)?;
                let (container, readonly) = self.types.container(p.ty)?;
                if readonly {
                    return Err(type_error("cannot assign through readonly pointer"));
                }
                let T::Pointer(ty) = container else {
                    return Err(type_error("assignment requires a pointer"));
                };
                if self.types.is_opaque(*ty)? {
                    return Err(type_error("cannot write through opaque pointer"));
                }
                self.expression(expr, Some(*ty))?;
            }
            S::IndexAssign {
                object,
                index,
                expr,
            } => {
                let mut target = Expression::at(
                    E::IndexAccess {
                        object: Box::new(object.clone()),
                        index: Box::new(index.clone()),
                    },
                    node.span,
                );
                let info = self.expression(&mut target, None)?;
                let E::IndexAccess {
                    object: typed_object,
                    index: typed_index,
                } = target.kind
                else {
                    unreachable!()
                };
                *object = *typed_object;
                *index = *typed_index;
                validate_assignment_target(&info)?;
                self.expression(expr, Some(info.ty))?;
            }
            S::MultiAssignment { targets, values } => {
                let destinations = targets
                    .iter_mut()
                    .map(|t| {
                        let info = self.expression(t, None)?;
                        validate_assignment_target(&info)?;
                        Ok(info.ty)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                if destinations.len() == values.len() {
                    for (value, ty) in values.iter_mut().zip(destinations) {
                        self.expression(value, Some(ty))?;
                    }
                } else {
                    let values = values
                        .iter_mut()
                        .map(|v| self.expression(v, None).map(|i| i.ty))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    let values =
                        resolve_multi_value_types(destinations.len(), &values, self.types)?;
                    for (a, b) in destinations.into_iter().zip(values) {
                        check_assignable(a, b, self.types)?;
                    }
                }
            }
            S::Return(values) => {
                let expected = self
                    .state
                    .return_type
                    .ok_or_else(|| type_error("return outside function"))?;
                let values = values.as_mut().map(Vec::as_mut_slice).unwrap_or(&mut []);
                if values.is_empty() {
                    let void = self.types.builtin(B::Void);
                    check_assignable(expected, void, self.types)?;
                } else if values.len() == 1 {
                    self.expression(&mut values[0], Some(expected))?;
                } else {
                    let T::Tuple(elements) = self.types.get(expected)?.clone() else {
                        return Err(type_error("return arity mismatch"));
                    };
                    if elements.len() != values.len() {
                        return Err(type_error("return arity mismatch"));
                    }
                    for (value, ty) in values.iter_mut().zip(elements) {
                        self.expression(value, Some(ty))?;
                    }
                }
            }
            S::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let bool_type = self.types.builtin(B::Boolean);
                self.expression(condition, Some(bool_type))?;
                self.statements(then_branch)?;
                if let Some(nodes) = else_branch {
                    self.statements(nodes)?;
                }
            }
            S::While { condition, body } => {
                if let Some(condition) = condition {
                    let ty = self.types.builtin(B::Boolean);
                    self.expression(condition, Some(ty))?;
                }
                self.state.loop_depth += 1;
                let result = self.statements(body);
                self.state.loop_depth -= 1;
                result?;
            }
            S::Switch { subject, arms } => {
                let info = self.expression(subject, None)?;
                for arm in arms {
                    self.switch_arm(arm, info.ty, Some(node.span))?;
                }
            }
            S::Defer(node) => {
                if self.state.return_type.is_none() {
                    return Err(type_error("defer requires a function scope"));
                }
                self.state.in_defer = true;
                let result = self.statement(node);
                self.state.in_defer = false;
                result?;
            }
            S::Break | S::Continue => {
                if self.state.loop_depth == 0 {
                    return Err(type_error("break/continue requires an enclosing loop"));
                }
            }
        }
        Ok(())
    }
    pub(super) fn switch_arm(
        &mut self,
        arm: &mut SwitchArm,
        subject: TypeId,
        span: Option<Span>,
    ) -> anyhow::Result<()> {
        if !matches!(self.types.get(subject)?, T::Enum(_)) {
            return Err(self.diagnostic(
                type_error("switch subject requires an enum type"),
                span,
                self.file,
            ));
        }
        if let Pattern::EnumVariant {
            variant,
            member,
            binding,
        } = &mut arm.pattern
        {
            let selected = super::value_typing::member(subject, variant, self.types)?;
            member.resolved = Some(selected);
            if let Some(binding) = binding {
                let crate::frontend::analysis_annotation::MemberKind::EnumVariant(index) =
                    selected.kind
                else {
                    unreachable!()
                };
                let T::Enum(variants) = self.types.get(subject)? else {
                    unreachable!()
                };
                let payload = variants[index as usize]
                    .payload
                    .ok_or_else(|| type_error("variant has no payload to bind"))?;
                let id = binding
                    .annotation
                    .binding
                    .ok_or_else(|| type_error("pending pattern binding"))?;
                self.types.bind(
                    id,
                    ExpressionInfo {
                        ty: payload,
                        category: C::MutablePlace,
                        type_value: None,
                    },
                );
            }
        }
        self.statements(&mut arm.body)
    }
}
