//! Bounded evaluation of functions returning compile-time type values.
//! This deliberately shares semantic typing rather than pretending to execute IR.
use super::TypeContext;
use crate::frontend::{
    analysis_annotation::{BindingId, ExpressionInfo},
    ast::{
        expression::{Expression, ExpressionKind as E},
        statement::{Statement, StatementKind as S},
    },
    identifier::{DeclId, Entity, ResolvedName},
    semantic_type::{TypeId, type_error},
    tokens::{Literal, Operator as O},
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Argument {
    Type(TypeId),
    Bool(bool),
}

pub(super) fn function_id(expr: &Expression) -> Option<DeclId> {
    if let E::QualifiedIdentifier(n) = &expr.kind
        && let Some(ResolvedName::Entity(Entity::Declaration(id))) = n.annotation.resolved
    {
        Some(id)
    } else {
        None
    }
}

pub(super) fn boolean(expr: &Expression, values: &HashMap<BindingId, bool>) -> Option<bool> {
    match &expr.kind {
        E::Literal(Literal::Boolean(b)) => Some(*b),
        E::QualifiedIdentifier(n) => {
            let binding = match n.annotation.resolved? {
                ResolvedName::Local(id) => BindingId::Local(id),
                ResolvedName::Entity(Entity::Declaration(id)) => BindingId::Declaration(id),
                _ => return None,
            };
            values.get(&binding).copied()
        }
        E::UnaryExpression(u) if u.operator == O::LogicalNot => {
            Some(!boolean(&u.expression, values)?)
        }
        E::BinaryExpresison(b) => {
            let a = boolean(&b.left, values)?;
            match b.operator {
                O::LogicalAnd if !a => Some(false),
                O::LogicalOr if a => Some(true),
                O::LogicalAnd | O::LogicalOr => boolean(&b.right, values),
                O::DoubleEquals => Some(a == boolean(&b.right, values)?),
                O::Different => Some(a != boolean(&b.right, values)?),
                _ => None,
            }
        }
        _ => None,
    }
}

impl TypeContext<'_> {
    pub(super) fn evaluate_type_call(
        &mut self,
        id: DeclId,
        args: &[Expression],
    ) -> anyhow::Result<TypeId> {
        if self.state.evaluation_depth >= 64 {
            return Err(type_error(
                "compile-time evaluation recursion limit exceeded",
            ));
        }
        let mut fun = self
            .state
            .functions
            .get(&id)
            .cloned()
            .ok_or_else(|| type_error("compile-time call requires a function definition"))?;
        let body = fun
            .body
            .as_mut()
            .ok_or_else(|| type_error("compile-time call requires a function body"))?;
        let arguments = args
            .iter()
            .map(|arg| {
                if let Some(ty) = arg.annotation.resolved.and_then(|i| i.type_value) {
                    Ok(Argument::Type(ty))
                } else if let Some(b) = boolean(arg, &self.state.evaluation_booleans) {
                    Ok(Argument::Bool(b))
                } else {
                    Err(type_error(
                        "compile-time type computation requires known type or bool arguments",
                    ))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let key = (id, arguments.clone());
        if let Some(ty) = self.state.evaluated_types.get(&key) {
            return Ok(*ty);
        }
        let saved_booleans = self.state.evaluation_booleans.clone();
        struct Owned(std::collections::HashSet<BindingId>);
        impl super::walk::Visitor for Owned {
            fn binding(
                &mut self,
                _: &str,
                slot: &mut crate::frontend::analysis_annotation::BindingAnnotation,
                _: crate::diagnostics::Span,
            ) -> anyhow::Result<()> {
                if let Some(id @ BindingId::Declaration(_)) = slot.binding {
                    self.0.insert(id);
                }
                Ok(())
            }
        }
        let mut owned = Owned(std::collections::HashSet::new());
        super::walk::statements(&mut owned, &mut body.statements, false)?;
        let temporary = |id: &BindingId| matches!(id, BindingId::Local(_)) || owned.0.contains(id);
        let saved_declarations = self
            .types
            .declarations
            .iter()
            .filter(|(id, _)| owned.0.contains(&BindingId::Declaration(**id)))
            .map(|(id, ty)| (*id, *ty))
            .collect::<HashMap<_, _>>();
        // Evaluation owns nominal definition sites separately from the symbolic body.
        let saved_sites = std::mem::take(&mut self.state.sites);
        let saved_locals = self.types.locals.clone();
        let saved_bindings = self
            .types
            .bindings
            .iter()
            .filter(|(id, _)| temporary(id))
            .map(|(id, info)| (*id, *info))
            .collect::<HashMap<_, _>>();
        let saved_values = self
            .types
            .type_values
            .iter()
            .filter(|(id, _)| temporary(id))
            .map(|(id, ty)| (*id, *ty))
            .collect::<HashMap<_, _>>();
        for ((param, arg), value) in fun.signature.parameters.iter().zip(args).zip(arguments) {
            let binding = param
                .annotation
                .binding
                .ok_or_else(|| type_error("pending parameter binding"))?;
            let info = arg
                .annotation
                .resolved
                .ok_or_else(|| type_error("pending compile-time argument"))?;
            self.types.bind(binding, info);
            if let Argument::Bool(value) = value {
                self.state.evaluation_booleans.insert(binding, value);
            }
        }
        self.state.evaluation_depth += 1;
        let file = self.file;
        self.file = id.file_id;
        let result = self
            .evaluate_type_block(&mut body.statements)
            .and_then(|value| {
                value.ok_or_else(|| type_error("compile-time function must return a type"))
            });
        self.file = file;
        self.state.evaluation_depth -= 1;
        self.state.evaluation_booleans = saved_booleans;
        self.state.sites = saved_sites;
        self.types.locals = saved_locals;
        self.types
            .declarations
            .retain(|id, _| !owned.0.contains(&BindingId::Declaration(*id)));
        self.types.declarations.extend(saved_declarations);
        self.types.bindings.retain(|id, _| !temporary(id));
        self.types.bindings.extend(saved_bindings);
        self.types.type_values.retain(|id, _| !temporary(id));
        self.types.type_values.extend(saved_values);
        if let Ok(ty) = &result {
            self.state.evaluated_types.insert(key, *ty);
        }
        result
    }
    fn evaluate_type_block(&mut self, nodes: &mut [Statement]) -> anyhow::Result<Option<TypeId>> {
        for node in nodes {
            let result = (|| {
                match &mut node.kind {
                    S::Return(Some(exprs)) if exprs.len() == 1 => {
                        let info = self.expression(&mut exprs[0], None)?;
                        return Ok(Some(info.type_value.ok_or_else(|| {
                            type_error("compile-time return requires a type value")
                        })?));
                    }
                    S::VariableDeclaration(v) => {
                        self.variable(v)?;
                        if let Some(expr) = &v.expression
                            && let Some(value) = boolean(expr, &self.state.evaluation_booleans)
                        {
                            self.state
                                .evaluation_booleans
                                .insert(v.annotation.binding.unwrap(), value);
                        } else if v
                            .expression
                            .as_ref()
                            .and_then(|e| e.annotation.resolved)
                            .is_none_or(|i| i.type_value.is_none())
                        {
                            return Err(type_error(
                                "compile-time local requires a known type or bool value",
                            ));
                        }
                    }
                    S::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        let branch = boolean(condition, &self.state.evaluation_booleans)
                            .ok_or_else(|| {
                                type_error("compile-time condition requires a known bool value")
                            })?;
                        let nodes = if branch {
                            then_branch.as_mut_slice()
                        } else {
                            else_branch
                                .as_mut()
                                .map(Vec::as_mut_slice)
                                .unwrap_or(&mut [])
                        };
                        if let Some(result) = self.evaluate_type_block(nodes)? {
                            return Ok(Some(result));
                        }
                    }
                    _ => {
                        return Err(type_error(
                            "statement cannot be evaluated as a compile-time type computation",
                        ));
                    }
                }
                Ok(None)
            })();
            if let Some(value) =
                result.map_err(|e| self.diagnostic(e, Some(node.span), self.file))?
            {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    pub(super) fn call(
        &mut self,
        callee: &mut Expression,
        args: &mut [Expression],
    ) -> anyhow::Result<ExpressionInfo> {
        use crate::frontend::{
            semantic_type::SemanticType as T, tokens::builtin::BuiltinType as B,
        };
        let info = self.expression(callee, None)?;
        let T::Function(mut signature) = self.types.get(info.ty)?.clone() else {
            return Err(type_error("expression is not callable"));
        };
        if signature.variadic {
            return Err(type_error("variadic calls are not implemented"));
        }
        if args.len() != signature.parameters.len() {
            return Err(type_error("call argument arity mismatch"));
        }
        let meta = signature
            .parameters
            .iter()
            .any(|t| matches!(self.types.get(*t), Ok(T::Builtin(B::MetaType))));
        let mut arguments = Vec::new();
        if meta {
            for (arg, ty) in args.iter_mut().zip(&signature.parameters) {
                if matches!(self.types.get(*ty)?, T::Builtin(B::MetaType)) {
                    arguments.push(
                        self.expression(arg, Some(*ty))?
                            .type_value
                            .ok_or_else(|| type_error("generic argument requires a type value"))?,
                    );
                }
            }
            let template = function_id(callee)
                .ok_or_else(|| type_error("generic call requires a named function template"))?;
            let instance = super::generics::resolve_generic_instance(
                &super::generics::GenericInstanceKey {
                    template,
                    arguments,
                },
                self,
            )?;
            let T::Function(f) = self.types.get(instance.signature)? else {
                return Err(type_error("invalid instance signature"));
            };
            signature = f.clone();
        }
        for (arg, ty) in args.iter_mut().zip(&signature.parameters) {
            self.expression(arg, Some(*ty))?;
        }
        if matches!(
            self.types.get(signature.return_type)?,
            T::Builtin(B::MetaType)
        ) {
            let id = function_id(callee)
                .ok_or_else(|| type_error("compile-time call requires a named function"))?;
            if self.state.evaluation_depth == 0
                && self.state.return_type.is_some()
                && args.iter().any(|arg| {
                    arg.annotation
                        .resolved
                        .is_none_or(|i| i.type_value.is_none())
                        && boolean(arg, &self.state.evaluation_booleans).is_none()
                })
            {
                let ty = self.types.intern(T::TypeResult {
                    function: id,
                    file: self.file,
                    span: callee.span,
                });
                return Ok(self.metatype(ty));
            }
            let ty = self.evaluate_type_call(id, args)?;
            return Ok(self.metatype(ty));
        }
        Ok(self.value(signature.return_type))
    }
}
