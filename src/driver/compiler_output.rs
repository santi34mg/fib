use std::path::PathBuf;

use crate::{
    frontend::{ast::Ast, tokens::Token},
    ir::Ir,
};

/// What `run_pipeline` produced. The CLI prints the binary path; library users
/// inspect the paths directly instead of parsing stdout.
#[derive(Debug)]
pub enum CompilationOutput {
    Tokens(Vec<Token>),
    Ast(Ast),
    Checked,
    Ir(Ir),
    Objects(Vec<PathBuf>),
    Binary(PathBuf),
}
