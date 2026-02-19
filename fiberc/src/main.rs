mod cli;
mod driver;
mod lexer;
mod parser;
mod token;

fn main() {
    let args = cli::parse_args();

    cli::exec_command(args);
}
