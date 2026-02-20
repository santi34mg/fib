use std::path::Path;
use std::{fs, process};

use crate::lowering;
use crate::parser::{Ast, Parser};
use crate::semantic_analysis;
use crate::{lexer::Lexer, token::Token};

pub fn run_pipeline(file: &Path, is_debug_mode: bool) {
    // Run pipeline
    let src = fs::read_to_string(&file).unwrap_or_else(|e| {
        eprintln!("Failed to read '{:?}': {}", file, e);
        process::exit(1);
    });
    let tokens = run_lexer(&src);
    // Optionally display tokens during development
    if is_debug_mode {
        show_tokens(&tokens);
    }
    let ast_opt = run_parser(tokens, file.to_string_lossy().to_string(), src.clone());
    let ast = match ast_opt {
        Some(a) => a,
        None => process::exit(1),
    };
    if is_debug_mode {
        show_ast(&ast);
    }

    // Run semantic analysis (stub)
    match semantic_analysis::analyze(&ast) {
        Ok(hirs) => {
            if is_debug_mode {
                println!("====START HIR (stub)====");
                for f in &hirs {
                    println!("HIR Function: {} -> {}", f.name, f.ret_type);
                }
                println!("====END HIR (stub)======");
            }

            // Lowering (stub): produce C source string
            match lowering::lower(&hirs) {
                Ok(c_src) => {
                    if is_debug_mode {
                        println!("====LOWERED C (stub)====\n{}", c_src);
                    }
                    // Write LLVM IR to a temporary file and attempt to compile with clang
                    let out_ll = "fib_out.ll";
                    if let Err(e) = std::fs::write(out_ll, &c_src) {
                        eprintln!("Failed to write LLVM IR to {}: {}", out_ll, e);
                        process::exit(1);
                    }
                    // Try to compile with clang
                    let out_bin = "fib_out";
                    let status = std::process::Command::new("clang")
                        .arg(out_ll)
                        .arg("-o")
                        .arg(out_bin)
                        .status();
                    match status {
                        Ok(s) if s.success() => {
                            println!("Built binary: {}", out_bin);
                        }
                        Ok(s) => {
                            eprintln!("clang failed with status: {}", s);
                            process::exit(1);
                        }
                        Err(e) => {
                            eprintln!("Failed to run clang: {}", e);
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Lowering failed: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Semantic analysis failed: {}", e);
            process::exit(1);
        }
    }
}

fn run_lexer(src: &String) -> Vec<Token> {
    let lexer = Lexer::new(&src);
    let tokens = lexer.collect();
    tokens
}

fn run_parser<'a>(tokens: Vec<Token>, filename: String, source: String) -> Option<Ast> {
    // TODO: improve error handling
    let mut parser = Parser::new(tokens.into_iter(), filename, source);
    parser.parse_program().ok()
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
