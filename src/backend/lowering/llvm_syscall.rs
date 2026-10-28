//! The Linux x86-64 kernel ABI, independent of Fib and C calling conventions.
use super::BackendError;
use inkwell::{
    builder::Builder,
    context::Context,
    values::{BasicValueEnum, IntValue},
};

pub(super) fn emit<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    number: BasicValueEnum<'ctx>,
    args: &[BasicValueEnum<'ctx>],
) -> Result<IntValue<'ctx>, BackendError> {
    let error = |e: inkwell::builder::BuilderError| BackendError::Invalid {
        message: e.to_string(),
    };
    let word = context.i64_type();
    let mut words = vec![number.into_int_value().into()];
    for arg in args {
        let value = match arg {
            BasicValueEnum::PointerValue(pointer) => builder
                .build_ptr_to_int(*pointer, word, "syscall.address")
                .map_err(error)?,
            BasicValueEnum::IntValue(value) => *value,
            _ => {
                return Err(BackendError::Invalid {
                    message: "invalid syscall register operand".into(),
                });
            }
        };
        words.push(value.into());
    }
    let signature = word.fn_type(&vec![word.into(); words.len()], false);
    let mut constraints = "={rax},0".to_owned();
    for register in ["rdi", "rsi", "rdx", "r10", "r8", "r9"]
        .iter()
        .take(args.len())
    {
        constraints.push_str(&format!(",{{{register}}}"));
    }
    constraints.push_str(",~{rcx},~{r11},~{dirflag},~{fpsr},~{flags},~{memory}");
    let asm = context.create_inline_asm(
        signature,
        "syscall".into(),
        constraints,
        true,
        false,
        None,
        false,
    );
    let call = builder
        .build_indirect_call(signature, asm, &words, "syscall.result")
        .map_err(error)?;
    Ok(call
        .try_as_basic_value()
        .basic()
        .expect("syscall returns int64")
        .into_int_value())
}
