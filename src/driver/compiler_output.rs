use std::path::PathBuf;

use crate::{
    frontend::{ast::Ast, tokens::Token},
    ir::Ir,
};

/// What `compile()` produced. The CLI prints a short summary; library users
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
