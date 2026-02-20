mod cli;
mod driver;
pub mod hir;
mod lexer;
pub mod lowering;
mod parser;
pub mod semantic_analysis;
mod token;

fn main() {
    let args = cli::parse_args();

    cli::exec_command(args);
}
