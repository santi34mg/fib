use std::collections::HashMap;
#[cfg(feature = "llvm")]
use std::error::Error;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::{fs, io};

use std::fmt;

use crate::backend::lowering;
use crate::frontend::analyze::{AnalysisError, analyze};
use crate::frontend::ast::{Ast, declaration::DeclarationNode};
use crate::frontend::identifier::Identifier;
use crate::frontend::ir::{CompilationUnit, HIRModule};
use crate::frontend::parser::ParseError;
use crate::frontend::parser::Parser;
use crate::frontend::{lexer::Lexer, tokens::Token};

/// Result of running the compiler frontend (lex + parse + analyze) without LLVM lowering.
/// Always contains as much data as could be produced before the first fatal error.
pub struct FrontendResponse {
    pub tokens: Vec<Token>,
    pub parse_errors: Vec<ParseError>,
    pub analysis_errors: Vec<String>,
    pub ast: Option<Ast>,
    pub hir: Option<CompilationUnit>,
}

#[derive(Debug)]
pub struct CompilationOptions<'a> {
    pub project_path: PathBuf,
    pub source: Option<&'a str>,
    /// Extra directories to search when resolving imports, in addition to the source file's directory.
    pub include_paths: Vec<PathBuf>,
}

impl<'a> CompilationOptions<'a> {
    #[cfg(feature = "llvm")]
    pub fn new(args: crate::cli::Args) -> Self {
        CompilationOptions {
            project_path: args.file,
            source: None,
            include_paths: args.include_path,
        }
    }
}

#[derive(Debug)]
struct DriverError {
    msg: String,
}

impl Error for DriverError {}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DriverError: {}", self.msg)
    }
}

impl From<String> for DriverError {
    fn from(value: String) -> Self {
        Self { msg: value }
    }
}

impl From<io::Error> for DriverError {
    fn from(value: io::Error) -> Self {
        Self {
            msg: value.to_string(),
        }
    }
}

impl From<AnalysisError> for DriverError {
    fn from(value: AnalysisError) -> Self {
        Self { msg: value.msg }
    }
}

/// Lexes, parses, analyzes and lowers
#[cfg(feature = "llvm")]
pub fn compile(compilation_options: CompilationOptions) -> Result<(), Box<dyn Error>> {
    let file = &compilation_options.project_path;
    if !file.is_file() {
        return Err(format!("Not a file. Found {:?}", file).into());
    }

    if file.extension().and_then(|s| s.to_str()) != Some("fib") {
        return Err(format!("Not a fib file. Found {:?}", file).into());
    }

    let file_contents = fs::read_to_string(file).map_err(DriverError::from)?;
    let filename = file.to_string_lossy().to_string();
    let src_root = file.parent().unwrap_or(Path::new("."));

    let tokens: Vec<Token> = Lexer::new(&file_contents).collect();

    let mut parser = Parser::new(tokens.into_iter(), file, file_contents);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(pe) => {
            eprintln!("{}", pe);
            return Err("Parser error.".to_string().into());
        }
    };

    // Build resolved module map (stdlib + user imports)
    let mut resolved_modules = HashMap::new();
    let mut resolving = Vec::new();
    let mut search_roots = vec![src_root];
    search_roots.extend(
        compilation_options
            .include_paths
            .iter()
            .map(|p| p.as_path()),
    );

    for decl in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import_decl) = decl {
            let import_paths: Vec<String> = import_decl
                .path
                .iter()
                .map(|id| id.value.clone())
                .collect();
            if !resolved_modules.contains_key(&import_paths) {
                resolve_module(
                    &import_paths,
                    &search_roots,
                    &mut resolved_modules,
                    &mut resolving,
                )
                .map_err(|e| format!("Import error: {}", e))?;
            }
        }
    }

    let mut hir = analyze(ast, &resolved_modules).map_err(|e| format!("Analysis failed: {}", e))?;

    // Merge imported declarations into the main compilation unit for lowering
    let all_decls: Vec<_> = hir
        .imported_declarations
        .drain(..)
        .chain(hir.declarations.drain(..))
        .collect();
    hir.declarations = all_decls;

    let c_src = lowering::lower(hir, &filename).map_err(|e| format!("Lowering failed: {}", e))?;

    // Write LLVM IR and compile with clang
    // TODO: don't hard code out path
    fs::create_dir_all("out").map_err(|e| format!("Failed to create out directory: {}", e))?;
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid file path: {:?}", file))?;
    let out_ll = format!("out/{}.ll", stem);
    let out_bin = format!("out/{}", stem);
    fs::write(&out_ll, &c_src)
        .map_err(|e| format!("Failed to write LLVM IR to {}: {}", out_ll, e))?;
    let mut output = std::process::Command::new("clang-17")
        .arg(&out_ll)
        .arg("-o")
        .arg(&out_bin)
        .output();
    // Fall back to an unversioned clang when clang-17 isn't installed.
    if matches!(&output, Err(e) if e.kind() == io::ErrorKind::NotFound) {
        output = std::process::Command::new("clang")
            .arg(&out_ll)
            .arg("-o")
            .arg(&out_bin)
            .output();
    }
    match output {
        Ok(o) if o.status.success() => {
            println!("Built binary: {}", filename);
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            return Err(format!("clang failed with status {}:\n{}", o.status, stderr).into());
        }
        Err(e) => return Err(format!("Failed to run clang: {}", e).into()),
    }

    Ok(())
}

/// Resolve an imported module from disk, recursively resolving its own imports.
/// `search_roots` is tried in order; the first root containing the module file wins.
fn resolve_module(
    path: &[String],
    search_roots: &[&Path],
    resolved: &mut HashMap<Vec<String>, HIRModule>,
    resolving: &mut Vec<Vec<String>>,
) -> Result<(), DriverError> {
    if resolved.contains_key(path) {
        return Ok(()); // already resolved
    }
    if resolving.contains(&path.to_vec()) {
        return Err(format!("circular import detected: {}", path.join("::")).into());
    }
    resolving.push(path.to_vec());

    // Try each search root in order: ["math", "vec3"] -> <root>/math/vec3.fib.
    // Fall back to dropping the first segment for roots that *are* the top
    // namespace (e.g. `import std::io` with `-I std` -> <std>/io.fib).
    let (file_path, source) = search_roots
        .iter()
        .find_map(|root| {
            let mut full = root.to_path_buf();
            for segment in path {
                full.push(segment);
            }
            full.set_extension("fib");
            if let Ok(s) = std::fs::read_to_string(&full) {
                return Some((full, s));
            }
            if path.len() > 1 {
                let mut p = root.to_path_buf();
                for segment in &path[1..] {
                    p.push(segment);
                }
                p.set_extension("fib");
                if let Ok(s) = std::fs::read_to_string(&p) {
                    return Some((p, s));
                }
            }
            None
        })
        .ok_or_else(|| {
            format!(
                "cannot read module '{}': No such file or directory",
                path.join("::")
            )
        })?;

    // FIXME: run compilation for imported modules instead of doing lexing and parsing manually
    let tokens: Vec<Token> = Lexer::new(&source).collect();
    let mut parser = Parser::new(tokens.into_iter(), file_path.as_path(), source);
    let ast = parser
        .parse()
        .map_err(|e| format!("parse error in module '{}': {}", path.join("::"), e))?;

    // Recursively resolve this module's imports first
    for decl in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = decl {
            let import_path: Vec<String> =
                import.path.iter().map(|id| id.value.clone()).collect();
            if !resolved.contains_key(&import_path) {
                resolve_module(&import_path, search_roots, resolved, resolving)?;
            }
        }
    }

    let cu = analyze(ast, resolved)?;
    let module_name = path.last().cloned().unwrap_or_default();
    let module = HIRModule {
        name: module_name,
        path: path
            .iter()
            .map(|s| Identifier { value: s.clone() })
            .collect(),
        exports: cu.scope_root.symbols,
        declarations: [cu.declarations, cu.imported_declarations].concat(),
    };
    resolved.insert(path.to_vec(), module);
    resolving.retain(|p| p != path);
    Ok(())
}
