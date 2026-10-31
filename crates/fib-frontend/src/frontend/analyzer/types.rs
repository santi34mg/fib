//! Pure compatibility of already resolved types; no AST, inference or conversions.
use crate::frontend::semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error};
use std::collections::HashSet;

/// Render resolved types for diagnostics rather than exposing table indices.
fn describe(id: TypeId, types: &SemanticTypes, depth: usize) -> anyhow::Result<String> {
    if depth == 32 {
        return Ok("...".into());
    }
    let child = |id| describe(id, types, depth + 1);
    Ok(match types.get(types.canonical(id)?)? {
        T::Builtin(builtin) => builtin.to_string(),
        T::Pointer(inner) => format!("*{}", child(*inner)?),
        T::Readonly(inner) => format!("const {}", child(*inner)?),
        T::Sentinel { inner, value } => match types.get(*inner)? {
            T::Pointer(t) => format!("[*:{value}]{}", child(*t)?),
            T::Slice(t) => format!("[:{value}]{}", child(*t)?),
            T::Array { element, length } => format!("[{length}:{value}]{}", child(*element)?),
            _ => return Err(type_error("invalid sentinel container")),
        },
        T::Slice(inner) => format!("[]{}", child(*inner)?),
        T::Array {
            element: inner,
            length,
        } => format!("[{length}]{}", child(*inner)?),
        T::Tuple(elements) => format!(
            "({})",
            elements
                .iter()
                .map(|id| child(*id))
                .collect::<anyhow::Result<Vec<_>>>()?
                .join(", ")
        ),
        T::Function(function) => format!(
            "{}{}fn({}) -> {}",
            if function.is_comp { "comp " } else { "" },
            if function.calling_convention == crate::frontend::semantic_type::CallConvention::C {
                "extern C "
            } else {
                ""
            },
            function
                .parameters
                .iter()
                .map(|id| child(*id))
                .collect::<anyhow::Result<Vec<_>>>()?
                .join(", "),
            child(function.return_type)?
        ),
        T::Struct(_) => "struct".into(),
        T::Enum(_) => "enum".into(),
        T::IntrinsicFunction => "compiler intrinsic".into(),
        T::Parameter(_) => "type parameter".into(),
        T::TypeResult { .. } => "computed type".into(),
        T::MemberResult { owner, spelling } => format!("{}::{spelling}", child(*owner)?),
        T::IndexResult { owner } => format!("element of {}", child(*owner)?),
        T::Pending | T::Declared(_) => unreachable!("get/canonical reject unresolved types"),
    })
}

pub fn check_assignable(
    expected: TypeId,
    found: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    types.validate_type(expected)?;
    types.validate_type(found)?;
    // Bottom coercion is directional and applies only to the value itself,
    // never recursively to pointees, elements or function signatures.
    if matches!(
        types.get(types.canonical(found)?)?,
        T::Builtin(crate::frontend::tokens::builtin::BuiltinType::Never)
    ) {
        return Ok(());
    }
    // Only the outer container layer may gain readonly permission.
    let comparison_expected = if let T::Readonly(inner) = types.get(types.canonical(expected)?)?
        && !matches!(types.get(types.canonical(found)?)?, T::Readonly(_))
        && matches!(
            types.get(types.canonical(*inner)?)?,
            T::Pointer(_) | T::Slice(_) | T::Array { .. } | T::Sentinel { .. }
        ) {
        *inner
    } else {
        expected
    };
    let mut pending = vec![(comparison_expected, found, true)];
    let mut seen = HashSet::new();
    while let Some((a, b, outer)) = pending.pop() {
        let (a, b) = (types.canonical(a)?, types.canonical(b)?);
        if a == b || !seen.insert((a, b, outer)) {
            continue;
        }
        match (types.get(a)?, types.get(b)?) {
            (T::Builtin(a), T::Builtin(b)) if a == b => {}
            (
                T::Parameter(_)
                | T::TypeResult { .. }
                | T::MemberResult { .. }
                | T::IndexResult { .. },
                _,
            )
            | (
                _,
                T::Parameter(_)
                | T::TypeResult { .. }
                | T::MemberResult { .. }
                | T::IndexResult { .. },
            ) => {}
            (T::Readonly(a), T::Readonly(b)) => pending.push((*a, *b, outer)),
            (
                T::Sentinel {
                    inner: a,
                    value: av,
                },
                T::Sentinel {
                    inner: b,
                    value: bv,
                },
            ) if av == bv => pending.push((*a, *b, false)),
            (T::Pointer(_), T::Sentinel { inner, .. })
            | (T::Slice(_), T::Sentinel { inner, .. })
                if outer =>
            {
                pending.push((a, *inner, false))
            }
            (T::Pointer(a), T::Pointer(b)) | (T::Slice(a), T::Slice(b)) => {
                pending.push((*a, *b, false))
            }
            (
                T::Array {
                    element: a,
                    length: an,
                },
                T::Array {
                    element: b,
                    length: bn,
                },
            ) if an == bn => pending.push((*a, *b, false)),
            (T::Tuple(a), T::Tuple(b)) if a.len() == b.len() => pending.extend(
                a.iter()
                    .copied()
                    .zip(b.iter().copied())
                    .map(|(a, b)| (a, b, false)),
            ),
            (T::Function(a), T::Function(b))
                if a.is_comp == b.is_comp
                    && a.calling_convention == b.calling_convention
                    && a.parameters.len() == b.parameters.len() =>
            {
                pending.push((a.return_type, b.return_type, false));
                pending.extend(
                    a.parameters
                        .iter()
                        .copied()
                        .zip(b.parameters.iter().copied())
                        .map(|(a, b)| (a, b, false)),
                );
            }
            _ => {
                return Err(type_error(format!(
                    "incompatible types: expected {}, found {}",
                    describe(expected, types, 0)?,
                    describe(found, types, 0)?
                )));
            }
        }
    }
    Ok(())
}
