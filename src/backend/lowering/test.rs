use inkwell::context::Context;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValue;

use super::context::{CodegenCtx, coerce_int_to_llvm_type};
use super::lower_ir;
use super::types::map_type_to_llvm;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{SymbolTable, Ty};

#[cfg(all(test, feature = "llvm"))]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use std::path::PathBuf;

    fn lower_ir_src(src: &str) -> String {
        let tokens: Vec<_> = Lexer::new(src).collect();
        let path = PathBuf::from("test.fib");
        let mut parser = Parser::new(tokens.into_iter(), &path, src.to_string());
        let ast = parser.parse().expect("parse failed");
        let typed =
            crate::frontend::analyze::analyze(ast, &Default::default()).expect("analysis failed");
        let ir = crate::ir::lower_typed_program(typed).expect("ir lower failed");
        lower_ir(ir, "test").expect("llvm lower_ir failed")
    }

    #[test]
    fn coerce_unsigned_widening_uses_zext() {
        let ctx = Context::create();
        let module = ctx.create_module("coerce_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        // fn f(i8) -> i64 { zext/sext param; ret }
        for (is_unsigned, name) in [(true, "fu"), (false, "fs")] {
            let i8_ty = ctx.i8_type();
            let i64_ty = ctx.i64_type();
            let fn_ty = i64_ty.fn_type(&[i8_ty.into()], false);
            let function = module.add_function(name, fn_ty, None);
            let entry = ctx.append_basic_block(function, "entry");
            builder.position_at_end(entry);
            let param = function.get_nth_param(0).unwrap().into_int_value();
            let coerced = coerce_int_to_llvm_type(
                &cctx,
                param.as_basic_value_enum(),
                i64_ty.as_basic_type_enum(),
                is_unsigned,
            )
            .expect("coerce");
            builder.build_return(Some(&coerced)).unwrap();
        }
        let ir = module.print_to_string().to_string();
        assert!(
            ir.contains("zext i8"),
            "expected zext for unsigned widening, got:\n{}",
            ir
        );
        assert!(
            ir.contains("sext i8"),
            "expected sext for signed widening, got:\n{}",
            ir
        );
    }

    #[test]
    fn map_type_covers_unsigned_kinds() {
        let ctx = Context::create();
        let scope = SymbolTable::new();
        let cases = [
            (BuiltinType::UInt1, 8u32),
            (BuiltinType::UInt2, 16),
            (BuiltinType::UInt4, 32),
            (BuiltinType::UInt8, 64),
            (BuiltinType::UInt16, 128),
            (BuiltinType::Int1, 8),
            (BuiltinType::Int8, 64),
        ];
        for (bt, bits) in cases {
            let llvm_ty =
                map_type_to_llvm(&Ty::Builtin(bt.clone()), &ctx, scope.clone()).expect("map type");
            match llvm_ty {
                BasicTypeEnum::IntType(it) => {
                    assert_eq!(it.get_bit_width(), bits, "wrong width for {:?}", bt)
                }
                other => panic!("expected int type for {:?}, got {:?}", bt, other),
            }
        }
        // Void is never a value type.
        assert!(map_type_to_llvm(&Ty::Builtin(BuiltinType::Void), &ctx, scope).is_err());
    }

    #[test]
    fn ir_consumer_lowers_straight_line() {
        let ir = lower_ir_src("fn main() @int { x := 1\ny := 2\nreturn x + y }");
        assert!(ir.contains("define"), "expected define, got:\n{}", ir);
        assert!(ir.contains("add"), "expected add, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }

    #[test]
    fn ir_consumer_uses_unsigned_div() {
        let ir = lower_ir_src("fn main() @uint8 { x: @uint8 = 200\ny: @uint8 = 3\nreturn x / y }");
        assert!(
            ir.contains("udiv"),
            "expected udiv for @uint8 division, got:\n{}",
            ir
        );
    }

    #[test]
    fn ir_consumer_uses_unsigned_cmp() {
        let ir = lower_ir_src("fn main() @bool { x: @uint8 = 200\ny: @uint8 = 3\nreturn x > y }");
        // Unsigned greater-than lowers to `icmp ugt`.
        assert!(
            ir.contains("ugt"),
            "expected ugt for @uint8 comparison, got:\n{}",
            ir
        );
    }

    #[test]
    fn ir_consumer_lowers_if_and_loop() {
        let ir = lower_ir_src(
            "fn main() @int { x := 0\nif x == 0 { x = 1 } else { x = 2 }\nfor (i: @int = 0; i < 10; i = i + 1) { x = x + i }\nreturn x }",
        );
        assert!(ir.contains("br i1"), "expected cond br, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }
}
