use std::error::Error;
use std::fs;
use std::path::Path;

use crate::ast::Ast;
use crate::lowering;
use crate::parser::Parser;
use crate::parser::parser::ParseResult;
use crate::analysis::analyze;
use crate::{lexer::Lexer, token::Token};

/// Library-friendly compile function. Returns Err with a message on failure.
pub fn compile(file: &Path, is_debug_mode: bool) -> Result<(), Box<dyn Error>> {
    if !file.is_file() {
        return Err(format!("Not a file. Found {:?}", file).into());
    }
    if file.extension().and_then(|s| s.to_str()) != Some("fib") {
        return Err(format!("Not a fib file. Found {:?}", file).into());
    }

    let file_contents = fs::read_to_string(file)?;
    let filename = file.to_string_lossy().to_string();

    let tokens = run_lexer(&file_contents);
    if is_debug_mode {
        show_tokens(&tokens);
    }
    let ast = match run_parser(tokens, file, file_contents) {
        Ok(ast) => ast,
        Err(pe) => {
            eprintln!("{}", pe);
            return Err(format!("Parser error.").into());
        }
    };
    if is_debug_mode {
        show_ast(&ast);
    }

    // I should probably canonicalize the names, not sure how to do it right now tho
    // let canonicalized_ast = semantic_analysis::canonicalization::canonicalize(&ast);

    let hir = analyze(ast).map_err(|e| format!("Analysis failed: {}", e))?;

    let c_src = lowering::lower(hir, &filename).map_err(|e| format!("Lowering failed: {}", e))?;

    // Write LLVM IR to a temporary file and attempt to compile with clang
    // TODO: dont hard code out path
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
pub fn run_pipeline(file: &Path, is_debug_mode: bool) {
    match compile(file, is_debug_mode) {
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
