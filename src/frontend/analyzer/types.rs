//! Pure compatibility of already resolved types; no AST, inference or conversions.
use crate::frontend::semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error};
use std::collections::HashSet;

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
    let mut pending = vec![(expected, found)];
    let mut seen = HashSet::new();
    while let Some((a, b)) = pending.pop() {
        let (a, b) = (types.canonical(a)?, types.canonical(b)?);
        if a == b || !seen.insert((a, b)) {
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
            (T::Readonly(a), T::Readonly(b))
            | (T::Pointer(a), T::Pointer(b))
            | (T::Slice(a), T::Slice(b)) => pending.push((*a, *b)),
            (
                T::Array {
                    element: a,
                    length: an,
                },
                T::Array {
                    element: b,
                    length: bn,
                },
            ) if an == bn => pending.push((*a, *b)),
            (T::Tuple(a), T::Tuple(b)) if a.len() == b.len() => {
                pending.extend(a.iter().copied().zip(b.iter().copied()))
            }
            (T::Function(a), T::Function(b))
                if a.is_comp == b.is_comp && a.parameters.len() == b.parameters.len() =>
            {
                pending.push((a.return_type, b.return_type));
                pending.extend(
                    a.parameters
                        .iter()
                        .copied()
                        .zip(b.parameters.iter().copied()),
                );
            }
            _ => {
                return Err(type_error(format!(
                    "incompatible types: expected {:?}, found {:?}",
                    types.get(expected)?,
                    types.get(found)?
                )));
            }
        }
    }
    Ok(())
}
