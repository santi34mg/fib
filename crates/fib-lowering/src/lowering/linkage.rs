//! Versioned, length-delimited UTF-8 symbol components. No build-local IDs.
use super::*;
use crate::frontend::semantic_type::ComputationArgument;

fn component(value: &str) -> String {
    format!("{}_{value}", value.len())
}

pub(super) fn module(project: &AnalyzedProject, file: FileId) -> anyhow::Result<String> {
    let (package, path) = project
        .symbols
        .linkage_modules
        .get(&file)
        .ok_or_else(|| anyhow::anyhow!("missing persistent module identity"))?;
    Ok(format!("{}{}", component(package), component(path)))
}

pub(super) fn declaration(project: &AnalyzedProject, id: DeclId) -> anyhow::Result<String> {
    let namespace = project
        .symbols
        .namespaces
        .get(&id.file_id)
        .ok_or_else(|| anyhow::anyhow!("missing declaration namespace"))?;
    let decl = namespace
        .declarations
        .get(id.index as usize)
        .ok_or_else(|| anyhow::anyhow!("missing declaration identity"))?;
    let local = if namespace.names.get(&decl.name) == Some(&id) {
        String::new()
    } else {
        format!("L{}_{}_", decl.span.line, decl.span.column)
    };
    Ok(format!(
        "{}{}{}",
        module(project, id.file_id)?,
        local,
        component(&decl.name)
    ))
}

pub(super) fn symbol(project: &AnalyzedProject, kind: &str, id: DeclId) -> anyhow::Result<String> {
    Ok(format!("fib.v1.{kind}.{}", declaration(project, id)?))
}

pub(super) fn instance(
    project: &AnalyzedProject,
    key: &crate::frontend::analyzer::generics::GenericInstanceKey,
) -> anyhow::Result<String> {
    let mut name = symbol(project, "instance", key.template)?;
    name.push_str(&format!("A{}_", key.arguments.len()));
    for &ty in &key.arguments {
        name.push_str(&component(&type_key(project, ty, &mut Vec::new())?));
    }
    Ok(name)
}

pub(super) fn type_key(
    project: &AnalyzedProject,
    ty: SemanticId,
    active: &mut Vec<SemanticId>,
) -> anyhow::Result<String> {
    let types = &project.types;
    let ty = types.canonical(ty)?;
    if let Some(index) = active.iter().position(|t| *t == ty) {
        return Ok(format!("R{index}_"));
    }
    active.push(ty);
    let result = (|| {
        // Computed nominal instances keep their constructor and concrete arguments,
        // including arguments unused by their storage layout.
        if matches!(
            types.get(ty)?,
            SemanticType::Struct(_) | SemanticType::Enum(_)
        ) {
            if let Some((function, args)) = types.nominal_computations.get(&ty) {
                let mut recipe = format!("C{}A{}_", declaration(project, *function)?, args.len());
                for arg in args {
                    recipe.push_str(&component(&match arg {
                        ComputationArgument::Type(t) => type_key(project, *t, active)?,
                        ComputationArgument::Bool(b) => format!("B{b}"),
                        ComputationArgument::Expression(_) => {
                            anyhow::bail!("unresolved computation in linkage identity")
                        }
                    }));
                }
                return Ok(recipe);
            }
            let mut names = Vec::new();
            for (&binding, &value) in &types.type_values {
                if value == ty
                    && let BindingId::Declaration(id) = binding
                {
                    let decl =
                        &project.symbols.namespaces[&id.file_id].declarations[id.index as usize];
                    if decl.alias_target.is_none() {
                        names.push(declaration(project, id)?);
                    }
                }
            }
            names.sort();
            if let Some(name) = names.into_iter().next() {
                return Ok(format!("N{name}"));
            }
        }
        let nested = |t, active: &mut Vec<_>| type_key(project, t, active).map(|s| component(&s));
        Ok(match types.get(ty)? {
            SemanticType::Builtin(b) => format!("B{b:?}"),
            SemanticType::Pointer(t) => format!("P{}", nested(*t, active)?),
            SemanticType::Readonly(t) => format!("Q{}", nested(*t, active)?),
            SemanticType::Slice(t) => format!("S{}", nested(*t, active)?),
            SemanticType::Array { element, length } => {
                format!("A{length}_{}", nested(*element, active)?)
            }
            SemanticType::Tuple(elements) => {
                let mut key = format!("T{}_", elements.len());
                for &element in elements {
                    key.push_str(&nested(element, active)?);
                }
                key
            }
            SemanticType::Function(f) => {
                let mut key = format!(
                    "F{:?}_{}_{}_",
                    f.calling_convention,
                    f.is_comp,
                    f.parameters.len()
                );
                for &parameter in &f.parameters {
                    key.push_str(&nested(parameter, active)?);
                }
                key.push_str(&nested(f.return_type, active)?);
                key
            }
            SemanticType::Struct(_) | SemanticType::Enum(_) => {
                let &(file, span) = types
                    .nominal_sites
                    .get(&ty)
                    .ok_or_else(|| anyhow::anyhow!("missing nominal type source identity"))?;
                let mut key = format!("L{}{}_{}_", module(project, file)?, span.line, span.column);
                if let Some(arguments) = types.nominal_arguments.get(&ty) {
                    key.push_str(&format!("A{}_", arguments.len()));
                    for &argument in arguments {
                        key.push_str(&nested(argument, active)?);
                    }
                }
                match types.get(ty)? {
                    SemanticType::Struct(fields) => {
                        key.push('s');
                        for field in fields {
                            key.push_str(&component(&field.spelling));
                            key.push_str(&nested(field.ty, active)?);
                        }
                    }
                    SemanticType::Enum(variants) => {
                        key.push('e');
                        for variant in variants {
                            key.push_str(&component(&variant.spelling));
                            if let Some(payload) = variant.payload {
                                key.push_str(&nested(payload, active)?);
                            } else {
                                key.push('v');
                            }
                        }
                    }
                    _ => unreachable!(),
                }
                key
            }
            _ => anyhow::bail!("unresolved type in linkage identity"),
        })
    })();
    active.pop();
    result
}
