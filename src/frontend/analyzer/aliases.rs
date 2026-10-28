//! File alias dependencies are resolved before the immutable per-file name walk.
use super::{at_span, with_path};
use crate::{
    diagnostics::Span,
    driver::CompilationUnit,
    frontend::{
        analysis_annotation::BindingId,
        ast::{
            expression::ExpressionKind, statement::StatementKind,
            variable_declaration::VariableDeclaration,
        },
        namespace::{DeclId, Entity, ProjectSymbols, name_error},
    },
};
use std::collections::HashMap;

pub(super) struct Definition {
    path: Vec<String>,
    span: Span,
}

pub(super) fn definitions(
    units: &[CompilationUnit],
    paths: &HashMap<crate::plan::FileId, std::path::PathBuf>,
    recovery: Option<&super::Recovery>,
) -> anyhow::Result<HashMap<DeclId, Definition>> {
    let mut definitions = HashMap::new();
    fn add(
        definitions: &mut HashMap<DeclId, Definition>,
        var: &VariableDeclaration,
        expr: Option<&crate::frontend::ast::expression::Expression>,
        span: Span,
    ) -> anyhow::Result<()> {
        if !var.is_comp {
            return Ok(());
        }
        let Some(expr) = expr else {
            return Err(at_span(
                name_error(format!("comp '{}' requires an initializer", var.identifier)),
                span,
            ));
        };
        let ExpressionKind::QualifiedIdentifier(name) = &expr.kind else {
            if matches!(
                expr.kind,
                ExpressionKind::TypeValue(_)
                    | ExpressionKind::Call { .. }
                    | ExpressionKind::IndexAccess { .. }
                    | ExpressionKind::SliceLiteral { .. }
            ) {
                return Ok(());
            }
            return Err(at_span(
                name_error(format!(
                    "unsupported computed comp initializer for '{}'",
                    var.identifier
                )),
                expr.span,
            ));
        };
        let Some(BindingId::Declaration(id)) = var.annotation.binding else {
            return Err(name_error("missing file declaration identity"));
        };
        definitions.insert(
            id,
            Definition {
                path: name.segments.clone(),
                span: expr.span,
            },
        );
        Ok(())
    }
    for unit in units {
        for statement in &unit.ast.statements {
            if recovery.is_some_and(|r| r.failed(unit.source_file_id, statement.span)) {
                continue;
            }
            let result = (|| {
                match &statement.kind {
                    StatementKind::VariableDeclaration(var) => add(
                        &mut definitions,
                        var,
                        var.expression.as_ref(),
                        statement.span,
                    )?,
                    StatementKind::MultiVariableDeclaration { bindings, values } => {
                        if bindings.iter().any(|b| b.is_comp) && bindings.len() != values.len() {
                            return Err(at_span(
                                name_error(
                                    "comp multi-bindings require one initializer per binding",
                                ),
                                statement.span,
                            ));
                        }
                        for (i, binding) in bindings.iter().enumerate() {
                            add(&mut definitions, binding, values.get(i), statement.span)?;
                        }
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                if let Some(recovery) = recovery {
                    recovery.record(
                        error,
                        unit.source_file_id,
                        statement.span,
                        paths.get(&unit.source_file_id),
                    );
                    recovery.block_statement(statement);
                } else {
                    return Err(with_path(error, paths.get(&unit.source_file_id)));
                }
            }
        }
    }
    Ok(definitions)
}

pub(super) fn resolve(
    symbols: &mut ProjectSymbols,
    definitions: &HashMap<DeclId, Definition>,
    paths: &HashMap<crate::plan::FileId, std::path::PathBuf>,
    recovery: Option<&super::Recovery>,
) -> anyhow::Result<()> {
    // Suspend a path lookup at its first unresolved alias, resolve that alias
    // with an explicit stack, then resume the original path. No callback recurses.
    #[derive(Debug)]
    struct NeedAlias(DeclId);
    impl std::fmt::Display for NeedAlias {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "pending alias {:?}", self.0)
        }
    }
    impl std::error::Error for NeedAlias {}

    let mut targets = HashMap::new();
    let mut active = HashMap::new();
    let mut stack = Vec::new();
    let mut ids = definitions.keys().copied().collect::<Vec<_>>();
    ids.sort_by_key(|id| (id.file_id, id.index));
    for root in ids {
        if targets.contains_key(&root)
            || recovery.is_some_and(|r| r.blocked(BindingId::Declaration(root)))
        {
            continue;
        }
        let result = (|| {
            active.insert(root, 0);
            stack.push(root);
            while let Some(&id) = stack.last() {
                let definition = &definitions[&id];
                let result = if symbols
                    .file_prefix(id.file_id, &definition.path)
                    .is_some_and(|(file, _)| recovery.is_some_and(|r| r.unavailable(file)))
                {
                    Err(super::recovery::Blocked.into())
                } else {
                    symbols.resolve_path_using(id.file_id, &definition.path, &mut |entity| {
                        match entity {
                            Entity::Declaration(dependency)
                                if definitions.contains_key(&dependency) =>
                            {
                                if recovery
                                    .is_some_and(|r| r.blocked(BindingId::Declaration(dependency)))
                                {
                                    return Err(super::recovery::Blocked.into());
                                }
                                targets
                                    .get(&dependency)
                                    .copied()
                                    .ok_or_else(|| NeedAlias(dependency).into())
                            }
                            _ => Ok(entity),
                        }
                    })
                };
                match result {
                    Ok(target) => {
                        targets.insert(id, target);
                        active.remove(&id);
                        stack.pop();
                    }
                    Err(error) => {
                        if let Some(NeedAlias(dependency)) = error.downcast_ref::<NeedAlias>() {
                            if let Some(&start) = active.get(dependency) {
                                let chain = stack[start..]
                                    .iter()
                                    .chain(std::iter::once(dependency))
                                    .map(|id| {
                                        let file = symbols
                                            .files_by_name
                                            .iter()
                                            .find_map(|(name, file)| {
                                                (*file == id.file_id).then_some(name.0.as_str())
                                            })
                                            .unwrap_or("<unknown file>");
                                        format!(
                                            "{file}::{}",
                                            symbols.declaration(*id).unwrap().name
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" -> ");
                                return Err(with_path(
                                    at_span(
                                        name_error(format!("alias cycle: {chain}")),
                                        definitions[dependency].span,
                                    ),
                                    paths.get(&root.file_id),
                                ));
                            }
                            active.insert(*dependency, stack.len());
                            stack.push(*dependency);
                        } else {
                            return Err(with_path(
                                at_span(error, definition.span),
                                paths.get(&root.file_id),
                            ));
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(recovery) = recovery {
                recovery.record(
                    error,
                    root.file_id,
                    definitions[&root].span,
                    paths.get(&root.file_id),
                );
                for id in stack.drain(..) {
                    recovery.block_binding(BindingId::Declaration(id));
                }
                active.clear();
            } else {
                return Err(error);
            }
        }
    }
    for (id, target) in targets {
        symbols
            .namespaces
            .get_mut(&id.file_id)
            .unwrap()
            .declarations[id.index as usize]
            .alias_target = Some(target);
    }
    Ok(())
}
