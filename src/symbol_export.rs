//! Deterministic public declaration inventory, independent of lowering and ABI.
use crate::frontend::{
    analysis_annotation::BindingId,
    analyzer::{AnalyzedProject, validate_analysis},
    ast::statement::StatementKind,
    identifier::{DeclId, Entity},
    namespace::{DeclarationKind, Visibility},
    semantic_type::SemanticType,
};
use serde_json::{Value, json};

/// Export source-file namespaces only (including dependencies selected by the plan).
/// Scoped visibility and local type reservations are deliberately excluded.
pub fn public_symbols(project: &AnalyzedProject) -> anyhow::Result<Value> {
    validate_analysis(project)?;
    let mut symbols = Vec::new();
    for (file, path) in &project.source_paths {
        let path = path.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "symbol export requires UTF-8 source paths: {}",
                path.display()
            )
        })?;
        let namespace = project
            .symbols
            .namespaces
            .get(file)
            .ok_or_else(|| anyhow::anyhow!("missing source namespace"))?;
        for id in namespace.names.values() {
            let declaration = &namespace.declarations[id.index as usize];
            if declaration.visibility != Visibility::Public {
                continue;
            }
            let info = project.types.bindings.get(&BindingId::Declaration(*id));
            let function = project
                .units
                .iter()
                .filter(|unit| unit.source_file_id == *file)
                .flat_map(|unit| &unit.ast.statements)
                .find_map(|statement| match &statement.kind {
                    StatementKind::FunctionDeclaration(function)
                        if function.signature.annotation.binding
                            == Some(BindingId::Declaration(*id)) =>
                    {
                        Some(function)
                    }
                    _ => None,
                });
            let kind = match declaration.kind {
                DeclarationKind::Alias => "alias",
                DeclarationKind::Type => "type",
                DeclarationKind::Value if function.is_some() => "function",
                DeclarationKind::Value if info.is_some_and(|i| i.type_value.is_some()) => "type",
                DeclarationKind::Value => "value",
            };
            let mut symbol = declaration_name(project, *id)?;
            symbol["kind"] = json!(kind);
            symbol["source"] = json!({
                "path": path,
                "line": declaration.span.line,
                "column": declaration.span.column,
            });
            if let Some(target) = declaration.alias_target {
                symbol["target"] = match target {
                    Entity::Declaration(id) => declaration_name(project, id)?,
                    Entity::File(file) => module_name(project, file)?,
                };
            }
            if let Some(function) = function {
                symbol["signature"] = json!({
                    "parameters": function.signature.parameters.iter().map(|parameter| json!({
                        "name": parameter.parameter_identifier,
                        "type": parameter.parameter_type.to_string(),
                    })).collect::<Vec<_>>(),
                    "return_type": function.signature.return_type.to_string(),
                    "compile_time": function.signature.is_comp,
                    "extern": function.is_extern,
                    "calling_convention": match function.signature.calling_convention {
                        crate::frontend::semantic_type::CallConvention::Fib => "Fib",
                        crate::frontend::semantic_type::CallConvention::C => "C",
                    },
                });
            }
            // Canonical primitive types are useful even for inferred bindings.
            if let Some(info) = info
                && let SemanticType::Builtin(ty) =
                    project.types.get(project.types.canonical(info.ty)?)?
            {
                symbol["type"] = json!(ty.name());
            }
            symbols.push(symbol);
        }
    }
    symbols.sort_by(|a, b| {
        for key in ["package", "module", "name"] {
            let order = a[key].as_str().cmp(&b[key].as_str());
            if !order.is_eq() {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    });
    Ok(json!({ "schema_version": 1, "symbols": symbols }))
}

fn module_name(project: &AnalyzedProject, file: crate::plan::FileId) -> anyhow::Result<Value> {
    if let Some((package, module)) = project.symbols.linkage_modules.get(&file) {
        return Ok(json!({ "package": package, "module": module }));
    }
    anyhow::ensure!(
        !project.source_paths.contains_key(&file),
        "missing source module identity"
    );
    let module = project
        .symbols
        .files_by_name
        .iter()
        .find_map(|(name, id)| (*id == file).then_some(name.0.as_str()))
        .unwrap_or("prelude");
    Ok(json!({ "package": "compiler", "module": module }))
}

fn declaration_name(project: &AnalyzedProject, id: DeclId) -> anyhow::Result<Value> {
    let mut name = module_name(project, id.file_id)?;
    let declaration = project
        .symbols
        .namespaces
        .get(&id.file_id)
        .and_then(|namespace| namespace.declarations.get(id.index as usize))
        .ok_or_else(|| anyhow::anyhow!("missing exported declaration"))?;
    name["name"] = json!(declaration.name);
    Ok(name)
}
