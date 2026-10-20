//! Explicit metatype arguments specialize signatures by semantic identity.
//! Templates are checked symbolically; instances retain the original source body.
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
}

pub fn resolve_generic_instance(
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
    let root = context.types.bindings[&BindingId::Declaration(key.template)].ty;
    let substitutions = parameters
        .into_iter()
        .zip(key.arguments.iter().copied())
        .collect();
    let signature = substitute(root, &substitutions, context.types, &mut HashMap::new())?;
    let instance = GenericInstance {
        key: key.clone(),
        signature,
    };
    context
        .types
        .generic_instances
        .insert(key, instance.clone());
    Ok(instance)
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
        T::Pointer(t) | T::Slice(t) => vec![*t],
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
            parameters: f
                .parameters
                .into_iter()
                .map(|t| substitute(t, substitutions, types, memo))
                .collect::<anyhow::Result<_>>()?,
            return_type: substitute(f.return_type, substitutions, types, memo)?,
            variadic: f.variadic,
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
                        parameters: fs.iter().map(|f| f.ty).collect(),
                        return_type: owner,
                        variadic: false,
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
            super::types::check_assignable(*expected, actual, types)?;
            continue;
        }
        if !dependent(original, &substitutions, types, &mut HashSet::new())? {
            super::types::check_assignable(original, actual, types)?;
            continue;
        }
        match (types.get(original)?, types.get(actual)?) {
            (T::Pointer(a), T::Pointer(b)) | (T::Slice(a), T::Slice(b)) => pending.push((*a, *b)),
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
                if a.variadic == b.variadic && a.parameters.len() == b.parameters.len() =>
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
