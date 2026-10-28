use std::{path::PathBuf, str::FromStr};

use anyhow::bail;

use crate::{
    builtins::BuiltinFunction,
    ir::{ConstantKind, FunctionBody, InstructionKind, Ir, Operand, Terminator},
    plan::PlanBuilder,
};

pub fn interpret_ir(ir: Ir, bo: &mut PlanBuilder) -> anyhow::Result<()> {
    crate::ir::verify_ir(&ir)?;
    // TODO: add general CFG/value interpretation; this initial build-file subset is
    // deliberately limited to direct builtin calls with constant byte strings.
    let build_function = ir
        .functions
        .iter()
        .find(|&f| f.identifier == "build")
        .ok_or_else(|| anyhow::anyhow!("couldn't find build function"))?;
    match build_function.body {
        FunctionBody::Declaration => bail!("expected build function to have a body"),
        FunctionBody::Definition {
            ref entry,
            ref blocks,
            stack_slots: _,
        } => {
            let entry_block = ir
                .get_block(*entry)
                .expect("couldn't get build entry block");
            if blocks.len() != 1 || !matches!(entry_block.terminator, Terminator::Return { .. }) {
                bail!("unsupported control flow in build-file interpreter");
            }
            for instruction in entry_block.instructions.iter() {
                match instruction.kind {
                    InstructionKind::Call {
                        ref callee,
                        ref args,
                    } => {
                        let builtin =
                            ir.builtin_functions.get(callee).copied().ok_or_else(|| {
                                anyhow::anyhow!("call to unsupported declaration {callee:?}")
                            })?;
                        let [Operand::Constant(constant_id)] = args.as_slice() else {
                            bail!("build builtin requires one constant argument")
                        };
                        let constant = ir
                            .get_constant(*constant_id)
                            .ok_or_else(|| anyhow::anyhow!("missing builtin argument constant"))?;
                        let ConstantKind::Bytes(bytes) = &constant.kind else {
                            bail!("build builtin argument must be bytes")
                        };
                        let string = std::str::from_utf8(bytes)?;
                        match builtin {
                            BuiltinFunction::UncheckedDeref
                            | BuiltinFunction::SliceFromParts
                            | BuiltinFunction::SliceData
                            | BuiltinFunction::Syscall0
                            | BuiltinFunction::Syscall1
                            | BuiltinFunction::Syscall2
                            | BuiltinFunction::Syscall3
                            | BuiltinFunction::Syscall4
                            | BuiltinFunction::Syscall5
                            | BuiltinFunction::Syscall6
                            | BuiltinFunction::PointerFromWord => {
                                bail!("runtime intrinsic cannot be interpreted")
                            }
                            BuiltinFunction::AddSource => {
                                bo.modules.push(PathBuf::from_str(string)?);
                            }
                            BuiltinFunction::AddCSource => {
                                bo.native.sources.push(PathBuf::from_str(string)?)
                            }
                            BuiltinFunction::AddLinkLib => {
                                bo.native.libraries.push(string.to_owned())
                            }
                            BuiltinFunction::AddLibraryPath => {
                                bo.native.library_paths.push(PathBuf::from_str(string)?)
                            }
                            BuiltinFunction::SetEntry => bo.entry_name = Some(string.to_owned()),
                            BuiltinFunction::SetOutput => {
                                bo.output = Some(PathBuf::from_str(string)?);
                            }
                        }
                    }
                    InstructionKind::Syscall { .. } => {
                        bail!("syscalls cannot be interpreted in build files")
                    }
                    _ => bail!("unsupported instruction in build-file interpreter"),
                }
            }
        }
    }
    Ok(())
}
