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
    paths: &HashMap<crate::build_plan::FileId, std::path::PathBuf>,
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
            if matches!(expr.kind, ExpressionKind::TypeValue(_)) {
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
        let result = (|| {
            for statement in &unit.ast.statements {
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
            }
            Ok(())
        })();
        result.map_err(|e| with_path(e, paths.get(&unit.source_file_id)))?;
    }
    Ok(definitions)
}

pub(super) fn resolve(
    symbols: &mut ProjectSymbols,
    definitions: &HashMap<DeclId, Definition>,
    paths: &HashMap<crate::build_plan::FileId, std::path::PathBuf>,
) -> anyhow::Result<()> {
    struct Dependencies<'a> {
        symbols: &'a ProjectSymbols,
        definitions: &'a HashMap<DeclId, Definition>,
        // Absent = unvisited, stack = resolving, targets = resolved.
        stack: Vec<DeclId>,
        targets: HashMap<DeclId, Entity>,
        paths: &'a HashMap<crate::build_plan::FileId, std::path::PathBuf>,
    }
    impl Dependencies<'_> {
        fn normalize(&mut self, entity: Entity) -> anyhow::Result<Entity> {
            let Entity::Declaration(id) = entity else {
                return Ok(entity);
            };
            let Some(definition) = self.definitions.get(&id) else {
                return Ok(entity);
            };
            if let Some(target) = self.targets.get(&id) {
                return Ok(*target);
            }
            if let Some(start) = self.stack.iter().position(|other| *other == id) {
                let chain = self.stack[start..]
                    .iter()
                    .chain(std::iter::once(&id))
                    .map(|id| {
                        let file = self
                            .symbols
                            .files_by_name
                            .iter()
                            .find_map(|(name, file)| {
                                (*file == id.file_id).then_some(name.0.as_str())
                            })
                            .unwrap_or("<unknown file>");
                        format!("{file}::{}", self.symbols.declaration(*id).unwrap().name)
                    })
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(with_path(
                    at_span(name_error(format!("alias cycle: {chain}")), definition.span),
                    self.paths.get(&id.file_id),
                ));
            }
            self.stack.push(id);
            let symbols = self.symbols;
            let result = symbols
                .resolve_path_using(id.file_id, &definition.path, &mut |e| self.normalize(e));
            self.stack.pop();
            let target = result.map_err(|error| {
                with_path(at_span(error, definition.span), self.paths.get(&id.file_id))
            })?;
            self.targets.insert(id, target);
            Ok(target)
        }
    }
    let mut dependencies = Dependencies {
        symbols,
        definitions,
        stack: Vec::new(),
        targets: HashMap::new(),
        paths,
    };
    // Stable traversal keeps diagnostics independent of HashMap iteration order.
    let mut ids = definitions.keys().copied().collect::<Vec<_>>();
    ids.sort_by_key(|id| (id.file_id, id.index));
    for id in ids {
        dependencies.normalize(Entity::Declaration(id))?;
    }
    let targets = dependencies.targets;
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
