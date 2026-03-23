#[cfg(feature = "llvm")]
use std::error::Error;
use std::path::Path;

use crate::ast::Ast;
use crate::parser::Parser;
use crate::parser::parser::{ParseError, ParseResult};
use crate::analysis::analyze;
use crate::hir::CompilationUnit;
use crate::{lexer::Lexer, token::{Token, TokenKind}};

const STDLIB_PRELUDE: &str = include_str!("../../std/libc.fib");

/// Result of running the compiler frontend (lex + parse + analyze) without LLVM lowering.
/// Always contains as much data as could be produced before the first fatal error.
pub struct FrontendResult {
    pub tokens: Vec<Token>,
    pub parse_errors: Vec<ParseError>,
    pub analysis_errors: Vec<String>,
    pub ast: Option<Ast>,
    pub hir: Option<CompilationUnit>,
}

/// Run lex → parse → analyze on `source` and return all results, even on partial failure.
/// This is the entry point for the LSP server.
pub fn compile_frontend(source: &str, filename: &Path) -> FrontendResult {
    // Lex user source; collect Error tokens as diagnostics
    let raw_tokens = run_lexer(source);
    let mut parse_errors: Vec<ParseError> = Vec::new();

    let tokens: Vec<Token> = raw_tokens
        .into_iter()
        .filter(|t| {
            if let TokenKind::Error(msg) = &t.kind {
                parse_errors.push(ParseError {
                    filename: filename.into(),
                    message: msg.clone(),
                    line: t.line,
                    column: t.column,
                    source_line: String::new(),
                });
                false
            } else {
                true
            }
        })
        .collect();

    // Parse stdlib prelude (should never fail; errors here are a compiler bug)
    let prelude_tokens = run_lexer(STDLIB_PRELUDE);
    let prelude_ast = match run_parser(prelude_tokens, Path::new("<stdlib>"), STDLIB_PRELUDE.to_string()) {
        Ok(ast) => ast,
        Err(_) => return FrontendResult { tokens, parse_errors, analysis_errors: vec![], ast: None, hir: None },
    };

    // Parse user source
    let user_ast = match run_parser(tokens.clone(), filename, source.to_string()) {
        Ok(ast) => ast,
        Err(pe) => {
            parse_errors.push(pe);
            return FrontendResult { tokens, parse_errors, analysis_errors: vec![], ast: None, hir: None };
        }
    };

    // Combine prelude + user declarations
    let mut combined_declarations = prelude_ast.declarations;
    combined_declarations.extend(user_ast.declarations);
    let ast = Ast { declarations: combined_declarations };

    // Semantic analysis
    let mut analysis_errors: Vec<String> = Vec::new();
    let hir = match analyze(ast.clone()) {
        Ok(hir) => Some(hir),
        Err(e) => {
            analysis_errors.push(e.to_string());
            None
        }
    };

    FrontendResult { tokens, parse_errors, analysis_errors, ast: Some(ast), hir }
}

#[cfg(feature = "llvm")]
use std::fs;
#[cfg(feature = "llvm")]
use crate::lowering;

/// Library-friendly compile function. Returns Err with a message on failure.
#[cfg(feature = "llvm")]
pub fn compile(file: &Path, is_debug_mode: bool) -> Result<(), Box<dyn Error>> {
    if !file.is_file() {
        return Err(format!("Not a file. Found {:?}", file).into());
    }
    if file.extension().and_then(|s| s.to_str()) != Some("fib") {
        return Err(format!("Not a fib file. Found {:?}", file).into());
    }

    let file_contents = fs::read_to_string(file)?;
    let filename = file.to_string_lossy().to_string();

    // Parse the stdlib prelude and prepend its declarations to the user's AST.
    let prelude_tokens = run_lexer(&STDLIB_PRELUDE.to_string());
    let prelude_ast = match run_parser(prelude_tokens, Path::new("<stdlib>"), STDLIB_PRELUDE.to_string()) {
        Ok(ast) => ast,
        Err(pe) => {
            return Err(format!("Stdlib parse error: {}", pe).into());
        }
    };

    let tokens = run_lexer(&file_contents);
    if is_debug_mode {
        show_tokens(&tokens);
    }
    let user_ast = match run_parser(tokens, file, file_contents) {
        Ok(ast) => ast,
        Err(pe) => {
            eprintln!("{}", pe);
            return Err(format!("Parser error.").into());
        }
    };

    // Combine prelude declarations (first) with user declarations.
    let mut combined_declarations = prelude_ast.declarations;
    combined_declarations.extend(user_ast.declarations);
    let ast = Ast { declarations: combined_declarations };

    if is_debug_mode {
        show_ast(&ast);
    }

    let hir = analyze(ast).map_err(|e| format!("Analysis failed: {}", e))?;

    let c_src = lowering::lower(hir, &filename).map_err(|e| format!("Lowering failed: {}", e))?;

    // Write LLVM IR to a temporary file and attempt to compile with clang
    // TODO: dont hard code out path
    fs::create_dir_all("out").map_err(|e| format!("Failed to create out directory: {}", e))?;
    let out_ll = format!("out/{}.ll", file.file_stem().unwrap().to_string_lossy().to_string());
    fs::write(&out_ll, &c_src)
        .map_err(|e| format!("Failed to write LLVM IR to {}: {}", out_ll, e))?;
    // Try to compile with clang
    let out_bin = out_ll.clone();
    let out_bin = out_bin.split(".").into_iter().next().unwrap();
    let output = std::process::Command::new("clang-17")
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
            return Err(format!("clang failed with status {}:\n{}", o.status, stderr).into());
        }
        Err(e) => return Err(format!("Failed to run clang: {}", e).into()),
    }

    Ok(())
}

/// Legacy binary entrypoint wrapper that exits the process on error.
#[cfg(feature = "llvm")]
pub fn run_pipeline(file: &Path, is_debug_mode: bool) {
    match compile(file, is_debug_mode) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn run_lexer(src: &str) -> Vec<Token> {
    Lexer::new(src).collect()
}

fn run_parser<'a>(tokens: Vec<Token>, filename: &Path, source: String) -> ParseResult<Ast> {
    let mut parser = Parser::new(tokens.into_iter(), filename, source);
    parser.parse()
}

pub(crate) fn show_tokens(tokens: &Vec<Token>) {
    println!("====START TOKENS=======");
    for token in tokens {
        println!("{:?}", token);
    }
    println!("====END TOKENS=========");
}

pub(crate) fn show_ast(ast: &Ast) {
    println!("====START AST==========");
    println!("{:#?}", ast);
    println!("====END AST============");
}
