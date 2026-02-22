use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fs, process};

use crate::ast::Ast;
use crate::ast::ast::DeclarationNode;
use crate::lowering;
use crate::parser::Parser;
use crate::parser::parser::ParseResult;
use crate::semantic_analysis;
use crate::{lexer::Lexer, token::Token};

use petgraph::Directed;
use petgraph::algo::toposort;
use petgraph::graph::Graph;
use walkdir::WalkDir;

/// Library-friendly compile function. Returns Err with a message on failure.
pub fn compile_project(file: &Path, is_debug_mode: bool) -> Result<(), String> {
    // Accept either a single .fib file or a directory containing multiple .fib modules.
    let mut fib_files: Vec<PathBuf> = Vec::new();
    if file.is_dir() {
        fib_files = discover_fib_files(file);
        if fib_files.is_empty() {
            return Err(format!("No .fib files found in directory {:?}", file));
        }
    } else if file.is_file() {
        if file.extension().and_then(|s| s.to_str()) != Some("fib") {
            return Err(format!(
                "Will not compile file that is not '.fib'. Found {:?}",
                file.extension()
            ));
        }
        fib_files.push(file.to_path_buf());
    } else {
        return Err(format!("Path {:?} is not a file or directory", file));
    }

    // First pass: parse all files into ASTs and discover module names and imports from AST.
    let mut module_to_path: HashMap<String, PathBuf> = HashMap::new();
    let mut module_deps: HashMap<String, Vec<String>> = HashMap::new();
    let mut ast_map: HashMap<String, Ast> = HashMap::new();

    for path in &fib_files {
        let src =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read '{:?}': {}", path, e))?;
        let tokens = run_lexer(&src);
        let ast = match run_parser(
            tokens,
            &path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
            src.clone(),
        ) {
            Ok(a) => a,
            Err(e) => return Err(format!("Parsing failed for {:?}: {}", path, e)),
        };

        // extract module decl from AST
        let module_node = ast
            .program
            .modules
            .get(0)
            .ok_or_else(|| format!("File {:?} contains no module", path))?;
        let mut module_name_opt: Option<String> = None;
        let mut uses: Vec<String> = Vec::new();
        for decl in &module_node.declarations {
            if let DeclarationNode::ModuleDeclaration(md) = decl {
                module_name_opt = Some(md.name.clone());
                uses = md.uses.clone();
                break;
            }
        }

        let module_name = module_name_opt.ok_or_else(|| {
            format!("File {:?} is missing a top-level `module` declaration; every file must declare a module", path)
        })?;

        if module_to_path.contains_key(&module_name) {
            return Err(format!(
                "Duplicate module '{}' found in {:?}",
                module_name, path
            ));
        }
        module_to_path.insert(module_name.clone(), path.to_path_buf());
        module_deps.insert(module_name.clone(), uses);
        ast_map.insert(module_name, ast);
    }

    // Build dependency graph and topologically sort
    let mut graph: Graph<String, (), Directed> = Graph::new();
    let mut node_map: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();

    // Add nodes
    for module in module_to_path.keys() {
        let idx = graph.add_node(module.clone());
        node_map.insert(module.clone(), idx);
    }

    // Add edges: dependency -> module (so dependencies come before dependents)
    for (module, deps) in &module_deps {
        for dep in deps {
            if !module_to_path.contains_key(dep) {
                return Err(format!(
                    "Module '{}' depends on missing module '{}'",
                    module, dep
                ));
            }
            let from = node_map.get(dep).unwrap();
            let to = node_map.get(module).unwrap();
            graph.add_edge(*from, *to, ());
        }
    }

    let ordered = match toposort(&graph, None) {
        Ok(nodes) => nodes,
        Err(cycle) => {
            let node = cycle.node_id();
            let name = graph.node_weight(node).cloned().unwrap_or_default();
            return Err(format!(
                "Cycle detected in module dependencies at module '{}'",
                name
            ));
        }
    };

    // Compile modules in topological order
    for node_idx in ordered {
        let module_name = graph.node_weight(node_idx).unwrap();
        let path = module_to_path.get(module_name).unwrap();
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("Could not read filename from {:?}", path))?
            .to_string();

        let src =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read '{:?}': {}", path, e))?;
        let tokens = run_lexer(&src);
        // Optionally display tokens during development
        if is_debug_mode {
            show_tokens(&tokens);
        }

        let ast = match run_parser(tokens, &filename, src.clone()) {
            Ok(ast) => ast,
            Err(e) => {
                eprintln!("Parsing failed: {}", e);
                process::exit(1);
            }
        };

        if is_debug_mode {
            show_ast(&ast);
        }

        // Run semantic analysis (stub)
        let hirs = semantic_analysis::analyze(&ast)
            .map_err(|e| format!("Semantic analysis failed for {}: {}", module_name, e))?;

        // Lowering (stub): produce C source string
        let c_src = lowering::lower(&hirs).map_err(|e| format!("Lowering failed: {}", e))?;

        // Write LLVM IR to a temporary file and attempt to compile with clang
        let out_ll = format!("{}.ll", filename);
        fs::write(&out_ll, &c_src)
            .map_err(|e| format!("Failed to write LLVM IR to {}: {}", out_ll, e))?;
        // Try to compile with clang
        let out_bin = filename.clone();
        let output = std::process::Command::new("clang")
            .arg(out_ll)
            .arg("-o")
            .arg(out_bin)
            .output();
        match output {
            Ok(o) if o.status.success() => {
                println!("Built binary: {}", filename);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                return Err(format!(
                    "clang failed with status {}:\n{}",
                    o.status, stderr
                ));
            }
            Err(e) => return Err(format!("Failed to run clang: {}", e)),
        }
    }

    Ok(())
}

/// Legacy binary entrypoint wrapper that exits the process on error.
pub fn run_pipeline(file: &Path, is_debug_mode: bool) {
    match compile_project(file, is_debug_mode) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn run_lexer(src: &String) -> Vec<Token> {
    let lexer = Lexer::new(&src);
    let tokens = lexer.collect();
    tokens
}

/// Discover `.fib` files recursively under `root`.
fn discover_fib_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("fib") {
            files.push(p.to_path_buf());
        }
    }
    files
}

/// Extract the top-level `module` declaration and `use` imports from a token stream.
/// Returns (module_name_opt, uses_vec).

fn run_parser<'a>(tokens: Vec<Token>, filename: &str, source: String) -> ParseResult<Ast> {
    // TODO: improve error handling
    let mut parser = Parser::new(tokens.into_iter(), filename, source);
    parser.parse_module()
}

#[allow(dead_code)]
pub(crate) fn show_tokens(tokens: &Vec<Token>) {
    println!("====START TOKENS=======");
    for token in tokens {
        println!("{:?}", token);
    }
    println!("====END TOKENS=========");
}

#[allow(dead_code)]
pub(crate) fn show_ast(ast: &Ast) {
    println!("====START AST==========");
    println!("{:#?}", ast);
    println!("====END AST============");
}
