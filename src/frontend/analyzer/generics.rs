//! Explicit metatype arguments specialize signatures by semantic identity.
//! Templates stay symbolic on the source AST. Concrete instances own a rechecked
//! recipe and binding facts; in-progress signatures are never published as checked bodies.
use std::collections::{HashMap, HashSet};

use super::TypeContext;
use crate::frontend::{
    analysis_annotation::BindingId,
    identifier::{DeclId, LocalId},
    semantic_type::{FunctionType, SemanticType as T, SemanticTypes, TypeId, type_error},
    tokens::builtin::BuiltinType as B,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstanceKey {
    pub template: DeclId,
    pub arguments: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericInstance {
    pub key: GenericInstanceKey,
    pub signature: TypeId,
    pub body: Option<crate::frontend::ast::function_declaration::FunctionDeclaration>,
    pub bindings: HashMap<BindingId, crate::frontend::analysis_annotation::ExpressionInfo>,
}

pub fn resolve_generic_instance(
    key: &GenericInstanceKey,
    context: &mut TypeContext<'_>,
) -> anyhow::Result<GenericInstance> {
    // A nested instance can finish while relying on an outer in-progress
    // signature. It is only valid if that entire resolution attempt succeeds.
    let cached: HashSet<_> = context.types.generic_instances.keys().cloned().collect();
    let result = resolve_generic_instance_inner(key, context);
    if result.is_err() {
        context
            .types
            .generic_instances
            .retain(|key, _| cached.contains(key));
    }
    result
}

fn resolve_generic_instance_inner(
    key: &GenericInstanceKey,
    context: &mut TypeContext<'_>,
) -> anyhow::Result<GenericInstance> {
    if !context
        .types
        .bindings
        .contains_key(&BindingId::Declaration(key.template))
    {
        context.ensure_declaration(key.template)?;
    }
    let parameters = context
        .types
        .generic_parameters
        .get(&key.template)
        .ok_or_else(|| type_error("generic template is not a function declaration"))?
        .clone();
    if parameters.is_empty() || parameters.len() != key.arguments.len() {
        return Err(type_error("generic type argument arity mismatch"));
    }
    let arguments = key
        .arguments
        .iter()
        .map(|t| {
            context.types.validate_type(*t)?;
            let t = context.types.canonical(*t)?;
            if matches!(context.types.get(t)?, T::Builtin(B::Never | B::MetaType)) {
                return Err(type_error("generic argument must be a storable type"));
            }
            Ok(t)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let key = GenericInstanceKey {
        template: key.template,
        arguments,
    };
    if let Some(instance) = context.types.generic_instances.get(&key) {
        return Ok(instance.clone());
    }
    if let Some(signature) = context.state.active_instances.get(&key) {
        return Ok(GenericInstance {
            key,
            signature: *signature,
            body: None,
            bindings: HashMap::new(),
        });
    }
    if context.state.active_instances.len() >= 64 {
        return Err(type_error(
            "generic specialization recursion limit exceeded",
        ));
    }
    let root = context.types.bindings[&BindingId::Declaration(key.template)].ty;
    let substitutions = parameters
        .into_iter()
        .zip(key.arguments.iter().copied())
        .collect();
    let signature = substitute(root, &substitutions, context.types, &mut HashMap::new())?;
    let signature = materialize(signature, context, &mut HashMap::new())?;
    let symbolic = key
        .arguments
        .iter()
        .any(|t| context.types.is_dependent(*t).unwrap_or(true));
    let mut instance = GenericInstance {
        key: key.clone(),
        signature,
        body: None,
        bindings: HashMap::new(),
    };
    if !symbolic {
        context
            .state
            .active_instances
            .insert(key.clone(), signature);
        let result = check_body(&mut instance, &substitutions, context);
        context.state.active_instances.remove(&key);
        result?;
    }
    context
        .types
        .generic_instances
        .insert(key, instance.clone());
    Ok(instance)
}

fn check_body(
    instance: &mut GenericInstance,
    substitutions: &HashMap<LocalId, TypeId>,
    context: &mut TypeContext<'_>,
) -> anyhow::Result<()> {
    let mut fun = context
        .state
        .functions
        .get(&instance.key.template)
        .cloned()
        .ok_or_else(|| type_error("generic specialization requires a function definition"))?;
    struct Owned(HashSet<BindingId>);
    impl super::walk::Visitor for Owned {
        fn binding(
            &mut self,
            _: &str,
            slot: &mut crate::frontend::analysis_annotation::BindingAnnotation,
            _: crate::diagnostics::Span,
        ) -> anyhow::Result<()> {
            if let Some(id) = slot.binding {
                self.0.insert(id);
            }
            Ok(())
        }
    }
    let mut owned = Owned(
        fun.signature
            .parameters
            .iter()
            .filter_map(|p| p.annotation.binding)
            .collect(),
    );
    if let Some(body) = &mut fun.body {
        super::walk::statements(&mut owned, &mut body.statements, false)?;
    }
    let saved_bindings = context.types.bindings.clone();
    let saved_values = context.types.type_values.clone();
    let saved_locals = context.types.locals.clone();
    let saved_declarations = context.types.declarations.clone();
    context.types.bindings.retain(|id, _| !owned.0.contains(id));
    context
        .types
        .type_values
        .retain(|id, _| !owned.0.contains(id));
    context
        .types
        .locals
        .retain(|id, _| !owned.0.contains(&BindingId::Local(*id)));
    context
        .types
        .declarations
        .retain(|id, _| !owned.0.contains(&BindingId::Declaration(*id)));
    let saved_sites = std::mem::take(&mut context.state.sites);
    let saved_specialization =
        std::mem::replace(&mut context.state.specialization, substitutions.clone());
    let saved_return = context.state.return_type;
    let saved_comp = context.state.comp_function;
    let saved_file = std::mem::replace(&mut context.file, instance.key.template.file_id);
    let saved_loop = std::mem::replace(&mut context.state.loop_depth, 0);
    let saved_defer = std::mem::replace(&mut context.state.in_defer, false);
    let result = (|| {
        let signature = context.signature(&mut fun.signature)?;
        instance.signature = signature;
        context
            .state
            .active_instances
            .insert(instance.key.clone(), signature);
        let T::Function(f) = context.types.get(signature)? else {
            return Err(type_error("invalid concrete signature"));
        };
        if context.types.is_dependent(signature)? {
            return Err(type_error(
                "unresolved computed type in concrete function signature",
            ));
        }
        context.state.return_type = Some(f.return_type);
        context.state.comp_function = fun.signature.is_comp;
        if let Some(body) = &mut fun.body {
            context.statements(&mut body.statements)?;
        }
        instance.bindings = context
            .types
            .bindings
            .iter()
            .filter(|(id, _)| owned.0.contains(id))
            .map(|(id, info)| (*id, *info))
            .collect();
        if instance.bindings.values().any(|i| {
            context.types.is_dependent(i.ty).unwrap_or(true)
                || (!fun.signature.is_comp
                    && i.type_value
                        .is_some_and(|t| context.types.is_dependent(t).unwrap_or(true)))
        }) {
            return Err(type_error(
                "unresolved computed type in concrete instance environment",
            ));
        }
        super::flow::validate_specialization(&fun, context.types)?;
        instance.body = Some(fun);
        Ok(())
    })();
    context.types.bindings.retain(|id, _| !owned.0.contains(id));
    context.types.bindings.extend(
        saved_bindings
            .into_iter()
            .filter(|(id, _)| owned.0.contains(id)),
    );
    context
        .types
        .type_values
        .retain(|id, _| !owned.0.contains(id));
    context.types.type_values.extend(
        saved_values
            .into_iter()
            .filter(|(id, _)| owned.0.contains(id)),
    );
    context
        .types
        .locals
        .retain(|id, _| !owned.0.contains(&BindingId::Local(*id)));
    context.types.locals.extend(
        saved_locals
            .into_iter()
            .filter(|(id, _)| owned.0.contains(&BindingId::Local(*id))),
    );
    context
        .types
        .declarations
        .retain(|id, _| !owned.0.contains(&BindingId::Declaration(*id)));
    context.types.declarations.extend(
        saved_declarations
            .into_iter()
            .filter(|(id, _)| owned.0.contains(&BindingId::Declaration(*id))),
    );
    context.state.sites = saved_sites;
    context.state.specialization = saved_specialization;
    context.state.return_type = saved_return;
    context.state.comp_function = saved_comp;
    context.state.loop_depth = saved_loop;
    context.state.in_defer = saved_defer;
    context.file = saved_file;
    result
}

fn dependent(
    id: TypeId,
    substitutions: &HashMap<LocalId, TypeId>,
    types: &SemanticTypes,
    seen: &mut HashSet<TypeId>,
) -> anyhow::Result<bool> {
    if !seen.insert(id) {
        return Ok(false);
    }
    let children = match types.get(id)? {
        T::Parameter(id) => return Ok(substitutions.contains_key(id)),
        T::TypeResult { .. } | T::MemberResult { .. } | T::IndexResult { .. } => return Ok(true),
        T::Readonly(t) | T::Pointer(t) | T::Slice(t) => vec![*t],
        T::Array { element, .. } => vec![*element],
        T::Tuple(ts) => ts.clone(),
        T::Function(f) => f
            .parameters
            .iter()
            .copied()
            .chain([f.return_type])
            .collect(),
        T::Struct(fs) => fs.iter().map(|f| f.ty).collect(),
        T::Enum(vs) => vs.iter().filter_map(|v| v.payload).collect(),
        T::Declared(_) => vec![types.canonical(id)?],
        _ => vec![],
    };
    for child in children {
        if dependent(child, substitutions, types, seen)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn substitute(
    id: TypeId,
    substitutions: &HashMap<LocalId, TypeId>,
    types: &mut SemanticTypes,
    memo: &mut HashMap<TypeId, TypeId>,
) -> anyhow::Result<TypeId> {
    if let Some(t) = memo.get(&id) {
        return Ok(*t);
    }
    if !dependent(id, substitutions, types, &mut HashSet::new())? {
        return Ok(id);
    }
    let original = types.get(id)?.clone();
    if let T::Parameter(p) = original {
        return Ok(substitutions[&p]);
    }
    // Reserve nominal identities before traversing pointer-recursive fields.
    let reservation = if matches!(original, T::Struct(_) | T::Enum(_)) {
        let owner = types.intern(T::Pending);
        memo.insert(id, owner);
        Some(owner)
    } else {
        None
    };
    let new = match original {
        T::TypeResult {
            function,
            arguments,
        } => T::TypeResult {
            function,
            arguments: arguments
                .into_iter()
                .map(|arg| {
                    Ok(match arg {
                        super::evaluation::Argument::Type(t) => super::evaluation::Argument::Type(
                            substitute(t, substitutions, types, memo)?,
                        ),
                        other => other,
                    })
                })
                .collect::<anyhow::Result<_>>()?,
        },
        T::MemberResult { owner, spelling } => T::MemberResult {
            owner: substitute(owner, substitutions, types, memo)?,
            spelling,
        },
        T::IndexResult { owner } => T::IndexResult {
            owner: substitute(owner, substitutions, types, memo)?,
        },
        T::Readonly(t) => T::Readonly(substitute(t, substitutions, types, memo)?),
        T::Pointer(t) => T::Pointer(substitute(t, substitutions, types, memo)?),
        T::Slice(t) => T::Slice(substitute(t, substitutions, types, memo)?),
        T::Array { element, length } => T::Array {
            element: substitute(element, substitutions, types, memo)?,
            length,
        },
        T::Tuple(ts) => T::Tuple(
            ts.into_iter()
                .map(|t| substitute(t, substitutions, types, memo))
                .collect::<anyhow::Result<_>>()?,
        ),
        T::Function(f) => T::Function(FunctionType {
            is_comp: f.is_comp,
            parameters: f
                .parameters
                .into_iter()
                .map(|t| substitute(t, substitutions, types, memo))
                .collect::<anyhow::Result<_>>()?,
            return_type: substitute(f.return_type, substitutions, types, memo)?,
        }),
        T::Struct(mut fs) => {
            for f in &mut fs {
                f.ty = substitute(f.ty, substitutions, types, memo)?;
            }
            T::Struct(fs)
        }
        T::Enum(mut vs) => {
            for v in &mut vs {
                if let Some(t) = &mut v.payload {
                    *t = substitute(*t, substitutions, types, memo)?;
                }
            }
            T::Enum(vs)
        }
        T::Declared(_) => return substitute(types.canonical(id)?, substitutions, types, memo),
        other => other,
    };
    let result = if let Some(owner) = reservation {
        types.types[owner.0 as usize] = new;
        if let T::Enum(vs) = types.get(owner)?.clone() {
            for (index, v) in vs.iter().enumerate() {
                if let Some(payload) = v.payload {
                    let T::Struct(fs) = types.get(payload)? else {
                        return Err(type_error("invalid variant payload"));
                    };
                    let f = FunctionType {
                        is_comp: false,
                        parameters: fs.iter().map(|f| f.ty).collect(),
                        return_type: owner,
                    };
                    let constructor = types.intern(T::Function(f));
                    types
                        .variant_constructors
                        .insert((owner, index as u32), constructor);
                }
            }
        }
        owner
    } else {
        types.intern(new)
    };
    memo.insert(id, result);
    Ok(result)
}

fn materialize(
    id: TypeId,
    context: &mut TypeContext<'_>,
    memo: &mut HashMap<TypeId, TypeId>,
) -> anyhow::Result<TypeId> {
    if let Some(t) = memo.get(&id) {
        return Ok(*t);
    }
    if !context.types.is_dependent(id)? {
        return Ok(id);
    }
    let original = context.types.get(id)?.clone();
    let reservation = if matches!(original, T::Struct(_) | T::Enum(_)) {
        let owner = context.types.intern(T::Pending);
        memo.insert(id, owner);
        Some(owner)
    } else {
        None
    };
    let new = match original {
        T::TypeResult {
            function,
            arguments,
        } => {
            let mut args = Vec::new();
            for arg in arguments {
                args.push(match arg {
                    super::evaluation::Argument::Type(t) => {
                        super::evaluation::Argument::Type(materialize(t, context, memo)?)
                    }
                    super::evaluation::Argument::Expression(e) => {
                        match super::evaluation::boolean(&e, &context.state.evaluation_booleans) {
                            Some(b) => super::evaluation::Argument::Bool(b),
                            None => super::evaluation::Argument::Expression(e),
                        }
                    }
                    other => other,
                });
            }
            return context.evaluate_type_arguments(function, args);
        }
        T::MemberResult { owner, spelling } => {
            let owner = materialize(owner, context, memo)?;
            if context.types.is_dependent(owner)? {
                return Ok(id);
            }
            return Ok(super::value_typing::member(owner, &spelling, context.types)?.ty);
        }
        T::IndexResult { owner } => {
            let owner = materialize(owner, context, memo)?;
            if context.types.is_dependent(owner)? {
                return Ok(id);
            }
            let info = context.value(owner);
            return Ok(super::value_typing::indexed(info, context.types)?.0);
        }
        T::Readonly(t) => T::Readonly(materialize(t, context, memo)?),
        T::Pointer(t) => T::Pointer(materialize(t, context, memo)?),
        T::Slice(t) => T::Slice(materialize(t, context, memo)?),
        T::Array { element, length } => T::Array {
            element: materialize(element, context, memo)?,
            length,
        },
        T::Tuple(ts) => T::Tuple(
            ts.into_iter()
                .map(|t| materialize(t, context, memo))
                .collect::<anyhow::Result<_>>()?,
        ),
        T::Function(mut f) => {
            f.parameters = f
                .parameters
                .into_iter()
                .map(|t| materialize(t, context, memo))
                .collect::<anyhow::Result<_>>()?;
            f.return_type = materialize(f.return_type, context, memo)?;
            T::Function(f)
        }
        T::Struct(mut fs) => {
            for f in &mut fs {
                f.ty = materialize(f.ty, context, memo)?;
            }
            T::Struct(fs)
        }
        T::Enum(mut vs) => {
            for v in &mut vs {
                if let Some(t) = v.payload {
                    v.payload = Some(materialize(t, context, memo)?);
                }
            }
            T::Enum(vs)
        }
        T::Declared(_) => return materialize(context.types.canonical(id)?, context, memo),
        _ => return Ok(id),
    };
    let result = if let Some(owner) = reservation {
        context.types.types[owner.0 as usize] = new;
        owner
    } else {
        context.types.intern(new)
    };
    memo.insert(id, result);
    Ok(result)
}

/// Check cached substitutions without allocating types or trusting the cache.
pub(super) fn validate_instance(
    instance: &GenericInstance,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    let parameters = types
        .generic_parameters
        .get(&instance.key.template)
        .ok_or_else(|| type_error("instance refers to an unknown generic template"))?;
    if parameters.is_empty() || parameters.len() != instance.key.arguments.len() {
        return Err(type_error("generic instance argument arity mismatch"));
    }
    let substitutions: HashMap<_, _> = parameters
        .iter()
        .copied()
        .zip(instance.key.arguments.iter().copied())
        .collect();
    for ty in &instance.key.arguments {
        types.validate_type(*ty)?;
        if types.canonical(*ty)? != *ty
            || matches!(types.get(*ty)?, T::Builtin(B::Never | B::MetaType))
        {
            return Err(type_error("invalid generic instance type argument"));
        }
    }
    let concrete = !instance
        .key
        .arguments
        .iter()
        .any(|t| types.is_dependent(*t).unwrap_or(true));
    if concrete && (instance.body.is_none() || types.is_dependent(instance.signature)?) {
        return Err(type_error(
            "concrete generic instance is missing its checked body",
        ));
    }
    if concrete {
        for (parameter, argument) in parameters.iter().zip(&instance.key.arguments) {
            if instance
                .bindings
                .get(&BindingId::Local(*parameter))
                .and_then(|i| i.type_value)
                != Some(*argument)
            {
                return Err(type_error(
                    "instance metatype environment disagrees with its arguments",
                ));
            }
        }
    }
    if !concrete && (instance.body.is_some() || !instance.bindings.is_empty()) {
        return Err(type_error(
            "symbolic instance must not retain a concrete environment",
        ));
    }
    let template = types
        .bindings
        .get(&BindingId::Declaration(instance.key.template))
        .ok_or_else(|| type_error("missing generic template binding"))?
        .ty;
    let mut pending = vec![(template, instance.signature)];
    let mut seen = HashSet::new();
    while let Some((original, actual)) = pending.pop() {
        if !seen.insert((original, actual)) {
            continue;
        }
        types.validate_type(actual)?;
        if let T::Parameter(p) = types.get(original)?
            && let Some(expected) = substitutions.get(p)
        {
            if types.canonical(*expected)? != types.canonical(actual)? {
                return Err(type_error("instance type argument substitution mismatch"));
            }
            continue;
        }
        if !dependent(original, &substitutions, types, &mut HashSet::new())? {
            super::types::check_assignable(original, actual, types)?;
            super::types::check_assignable(actual, original, types)?;
            continue;
        }
        match (types.get(original)?, types.get(actual)?) {
            (
                T::TypeResult {
                    function,
                    arguments,
                },
                _,
            ) => {
                let mut args = Vec::new();
                for arg in arguments {
                    args.push(match arg {
                        super::evaluation::Argument::Type(t) => {
                            let resolved = if let T::Parameter(p) = types.get(*t)? {
                                substitutions.get(p).copied().unwrap_or(*t)
                            } else {
                                *t
                            };
                            super::evaluation::Argument::Type(resolved)
                        }
                        other => other.clone(),
                    });
                }
                if types.computation_results.get(&(*function, args)).copied() != Some(actual)
                    && original != actual
                {
                    return Err(type_error(
                        "computed instance signature disagrees with retained computation",
                    ));
                }
            }
            (T::Readonly(a), T::Readonly(b))
            | (T::Pointer(a), T::Pointer(b))
            | (T::Slice(a), T::Slice(b)) => pending.push((*a, *b)),
            (
                T::Array {
                    element: a,
                    length: al,
                },
                T::Array {
                    element: b,
                    length: bl,
                },
            ) if al == bl => pending.push((*a, *b)),
            (T::Tuple(a), T::Tuple(b)) if a.len() == b.len() => {
                pending.extend(a.iter().copied().zip(b.iter().copied()))
            }
            (T::Function(a), T::Function(b))
                if a.is_comp == b.is_comp && a.parameters.len() == b.parameters.len() =>
            {
                pending.extend(
                    a.parameters
                        .iter()
                        .copied()
                        .zip(b.parameters.iter().copied()),
                );
                pending.push((a.return_type, b.return_type));
            }
            (T::Struct(a), T::Struct(b))
                if a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.spelling == b.spelling) =>
            {
                pending.extend(a.iter().zip(b).map(|(a, b)| (a.ty, b.ty)))
            }
            (T::Enum(a), T::Enum(b))
                if a.len() == b.len()
                    && a.iter().zip(b).all(|(a, b)| {
                        a.spelling == b.spelling && a.payload.is_some() == b.payload.is_some()
                    }) =>
            {
                pending.extend(
                    a.iter()
                        .zip(b)
                        .filter_map(|(a, b)| a.payload.zip(b.payload)),
                )
            }
            _ => {
                return Err(type_error(
                    "generic instance signature disagrees with its substitution",
                ));
            }
        }
    }
    Ok(())
}
