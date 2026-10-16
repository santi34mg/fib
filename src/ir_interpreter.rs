use std::path::PathBuf;

use crate::{builder_object::BuilderObject, ir::Ir};

pub fn interpret_ir(ir: Ir, bo: &mut BuilderObject) -> anyhow::Result<Vec<PathBuf>> {
    let _ = bo;
    let _ = ir;
    todo!()
}
