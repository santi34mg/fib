use std::env::{self};

use clap::Parser;

use crate::builder_object::BuilderObject;

#[derive(Parser, Debug)]
#[command(name = "fib", about = "Integrated compilation tool")]
pub enum Command {
    #[command(name = "build")]
    Build,

    #[command(name = "run")]
    Run,

    #[command(name = "test")]
    Test,
}

pub fn exec_command(command: Command, env_vars: env::VarsOs) -> anyhow::Result<()> {
    let _ = env_vars;
    let mut bo = BuilderObject::new();
    bo.add_build_file()?.compile_and_interpret_build_file()?;
    match command {
        Command::Build => {
            // here we ignore the output of build but we might be interested
            // in inspecting it in the future.
            // for now we leave it to build object's internals
            bo.build()?;
        }
        Command::Run => bo.run()?,
        // TODO: do some testingggg
        Command::Test => todo!(),
    };
    Ok(())
}
