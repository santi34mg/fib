//! Process entry validation and source startup adaptation. The wrapper goes
//! through the same analyzer, SSA verifier and ABI classifier as user functions.
use crate::frontend::{
    analysis_annotation::BindingId,
    analyzer::AnalyzedProject,
    ast::statement::StatementKind,
    semantic_type::{ComputationArgument, SemanticType, TypeId},
    tokens::builtin::BuiltinType,
};
use crate::plan::BuildPlan;

fn builtin(project: &AnalyzedProject, ty: TypeId, expected: BuiltinType) -> anyhow::Result<bool> {
    Ok(
        matches!(project.types.get(project.types.canonical(ty)?)?, SemanticType::Builtin(b) if *b == expected),
    )
}

fn std_declaration(
    project: &AnalyzedProject,
    plan: &BuildPlan,
    module: &str,
    name: &str,
) -> Option<crate::frontend::identifier::DeclId> {
    let package = plan.standard_library.package()?;
    let file = project.symbols.package_file(package, module)?;
    project
        .symbols
        .namespaces
        .get(&file)?
        .names
        .get(name)
        .copied()
}

fn std_type(
    project: &AnalyzedProject,
    plan: &BuildPlan,
    module: &str,
    name: &str,
) -> Option<TypeId> {
    let declaration = std_declaration(project, plan, module, name)?;
    project
        .types
        .type_values
        .get(&BindingId::Declaration(declaration))
        .copied()
}

pub(super) fn startup_wrapper(
    project: &AnalyzedProject,
    plan: &BuildPlan,
) -> anyhow::Result<Option<(String, String)>> {
    let entries: Vec<_> = project
        .units
        .iter()
        .filter(|unit| unit.source_file_id == project.entry)
        .flat_map(|unit| &unit.ast.statements)
        .filter_map(|statement| match &statement.kind {
            StatementKind::FunctionDeclaration(f) if f.signature.identifier == "main" => Some(f),
            _ => None,
        })
        .collect();
    anyhow::ensure!(
        entries.len() == 1,
        "expected exactly one 'main' entry function"
    );
    let entry = entries[0];
    let signature = &entry.signature;
    anyhow::ensure!(
        entry.body.is_some() && !entry.is_extern && !signature.is_comp,
        "entry function 'main' must be an ordinary Fib definition"
    );
    let result = signature
        .return_type
        .annotation
        .resolved
        .ok_or_else(|| anyhow::anyhow!("entry function 'main' has no resolved return type"))?;
    let integer = builtin(project, result, BuiltinType::Int32)?;
    let void = builtin(project, result, BuiltinType::Void)?;
    let mut ok_integer = false;
    if !integer && !void {
        let template = std_declaration(project, plan, "result", "Result");
        let success =
            project
                .types
                .computation_results
                .iter()
                .find_map(|((function, args), ty)| {
                    if Some(*function) != template
                        || project.types.canonical(*ty).ok() != project.types.canonical(result).ok()
                    {
                        return None;
                    }
                    match args.as_slice() {
                        [ComputationArgument::Type(ok), ComputationArgument::Type(_)] => Some(*ok),
                        _ => None,
                    }
                });
        let Some(success) = success else {
            anyhow::bail!(
                "entry function 'main' must return int32, void, Result(void, E) or Result(int32, E)"
            );
        };
        ok_integer = builtin(project, success, BuiltinType::Int32)?;
        anyhow::ensure!(
            ok_integer || project.types.enum_payload(Some(success))?.is_none(),
            "entry Result success type must be int32 or unit"
        );
    }
    anyhow::ensure!(
        signature.parameters.len() <= 2,
        "main accepts optional arguments and environment parameters"
    );
    let args_type = std_type(project, plan, "process", "Arguments");
    let env_type = std_type(project, plan, "process", "Environment");
    let mut args = false;
    let mut env = false;
    let mut parameters = Vec::new();
    for parameter in &signature.parameters {
        let ty = parameter
            .parameter_type
            .annotation
            .resolved
            .ok_or_else(|| anyhow::anyhow!("unresolved main parameter type"))?;
        let same = |expected: Option<TypeId>| -> anyhow::Result<bool> {
            Ok(expected.is_some_and(|e| {
                project.types.canonical(e).ok() == project.types.canonical(ty).ok()
            }))
        };
        if same(args_type)? && !args && !env {
            args = true;
            parameters.push("fib_arguments");
        } else if same(env_type)? && !env {
            env = true;
            parameters.push("fib_environment");
        } else {
            anyhow::bail!(
                "main parameters must be std::process::Arguments followed by std::process::Environment (either may be omitted)"
            );
        }
    }
    if integer && parameters.is_empty() {
        return Ok(None);
    }
    let namespace = &project.symbols.namespaces[&project.entry];
    let mut name = "__fib_process_entry".to_owned();
    while namespace.names.contains_key(&name) {
        name.push('_');
    }
    let mut source =
        format!("fn {name}(fib_argc: int32, fib_argv: **uint8, fib_envp: **uint8) int32 {{\n");
    if args {
        source.push_str("fib_arguments := std::process::arguments(fib_argc, fib_argv);\n");
        source.push_str("defer std::memory::free(std::process::Bytes, std::memory::slice_data(fib_arguments));\n");
    }
    if env {
        source.push_str("fib_environment := std::process::environment(fib_envp);\n");
        // Transfer ownership to main: growing or deinitializing its map must not
        // leave a stale shallow copy to free in the wrapper. Process exit
        // reclaims storage if main elects not to deinitialize it.
    }
    let call = format!("main({})", parameters.join(", "));
    if integer {
        source.push_str(&format!("return {call};\n"));
    } else if void {
        source.push_str(&format!("{call}; return 0;\n"));
    } else {
        source.push_str(&format!("switch ({call}) {{ when .Ok(value) {{ return {}; }} when .Err(error) {{ return 1; }} }};\n",
            if ok_integer { "value" } else { "0" }));
    }
    source.push('}');
    Ok(Some((name, source)))
}
