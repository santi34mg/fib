use clap::Parser;
use std::env;

fn main() -> anyhow::Result<()> {
    let command = fibc::cli::Command::parse();
    let env_vars = env::vars_os();
    fibc::cli::exec_command(command, env_vars)
}
