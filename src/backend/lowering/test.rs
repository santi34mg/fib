use inkwell::context::Context;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValue;

use super::context::{CodegenCtx, FunctionLowering, coerce_int_to_llvm_type};
use super::error::LowerError;
use super::lower_ir;
use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{SymbolTable, Ty, TypedEnumVariant};

#[cfg(all(test, feature = "llvm"))]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::super::expressions::{call_result, unpack_tuple_value};
    use crate::frontend::typed_ast::{TypedExpr, TypedExprKind};

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
    fn slice_maps_to_ptr_len_struct() {
        use crate::backend::lowering::types::typed_type_size_align;
        let ctx = Context::create();
        let scope = SymbolTable::new();
        let ty = Ty::Slice(Box::new(Ty::Builtin(BuiltinType::Int4)));
        let llvm_ty = map_type_to_llvm(&ty, &ctx, scope.clone()).expect("slice maps");
        match llvm_ty {
            BasicTypeEnum::StructType(st) => {
                assert_eq!(st.count_fields(), 2, "slice is {{ ptr, len }}");
            }
            other => panic!("expected slice struct, got {:?}", other),
        }
        let (size, align) = typed_type_size_align(&ty, &scope).expect("slice layout");
        assert_eq!((size, align), (16, 8));
        // `@usize` is the 64-bit length type.
        let usize_ty =
            map_type_to_llvm(&Ty::Builtin(BuiltinType::Usize), &ctx, scope).expect("usize maps");
        match usize_ty {
            BasicTypeEnum::IntType(it) => assert_eq!(it.get_bit_width(), 64),
            other => panic!("expected i64 for @usize, got {:?}", other),
        }
    }

    #[test]
    fn ir_consumer_lowers_straight_line() {
        let ir = lower_ir_src("fn main() @int { x := 1;\ny := 2;\nreturn x + y; }");
        assert!(ir.contains("define"), "expected define, got:\n{}", ir);
        assert!(ir.contains("add"), "expected add, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }

    #[test]
    fn ir_consumer_uses_unsigned_div() {
        let ir =
            lower_ir_src("fn main() @uint8 { x: @uint8 = 200;\ny: @uint8 = 3;\nreturn x / y; }");
        assert!(
            ir.contains("udiv"),
            "expected udiv for @uint8 division, got:\n{}",
            ir
        );
    }

    #[test]
    fn ir_consumer_uses_unsigned_cmp() {
        let ir =
            lower_ir_src("fn main() @bool { x: @uint8 = 200;\ny: @uint8 = 3;\nreturn x > y; }");
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
            "fn main() @int { x := 0;\nif x == 0 { x = 1; } else { x = 2; };\nfor (i: @int = 0; i < 10; i = i + 1) { x = x + i; };\nreturn x; }",
        );
        assert!(ir.contains("br i1"), "expected cond br, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }

    #[test]
    fn unknown_layout_errors_instead_of_silent_zero() {
        // An enum payload over an unresolvable type must fail loudly —
        // a silent `(0, 1)` size would emit out-of-bounds stores.
        let ctx = Context::create();
        let scope = SymbolTable::new();
        let ty = Ty::Enum {
            variants: vec![TypedEnumVariant {
                name: "V".to_string(),
                discriminant: 0,
                payload: Some(vec![(
                    "f".to_string(),
                    Ty::Identifier(Identifier {
                        value: "Missing".to_string(),
                    }),
                )]),
            }],
        };
        let err = map_type_to_llvm(&ty, &ctx, scope).expect_err("expected UnknownLayout");
        assert!(
            err.to_string().contains("UnknownLayout"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn lvalue_of_undeclared_identifier_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("lvalue_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let fn_ty = ctx.i32_type().fn_type(&[], false);
        let function = module.add_function("probe", fn_ty, None);
        let mut fl =
            FunctionLowering::new(&cctx, function, HashMap::new(), SymbolTable::new(), false);
        let expr = TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Int4),
            expression: TypedExprKind::Identifier(Identifier {
                value: "undeclared".to_string(),
            }),
        };
        let err = fl
            .compute_lvalue_ptr(&expr)
            .expect_err("expected no-alloca error");
        assert!(
            err.to_string().contains("no alloca"),
            "unexpected error: {}",
            err
        );
    }

    // ── error paths: coercion needs a positioned builder ──

    #[test]
    fn coerce_width_change_without_insert_block_errors() {
        // A fresh builder has no insert block, so any width-changing
        // coercion (zext/sext/trunc) fails with `UnsetPosition` instead of
        // emitting into the void.
        let ctx = Context::create();
        let module = ctx.create_module("coerce_err_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let i8_val = ctx.i8_type().const_int(1, false).as_basic_value_enum();
        let i64_ty = ctx.i64_type().as_basic_type_enum();
        let err = coerce_int_to_llvm_type(&cctx, i8_val, i64_ty, true)
            .expect_err("expected unset-position error on widening");
        assert!(
            err.to_string().contains("position"),
            "unexpected error: {}",
            err
        );
        let i64_val = ctx.i64_type().const_int(1, false).as_basic_value_enum();
        let i8_ty = ctx.i8_type().as_basic_type_enum();
        let err = coerce_int_to_llvm_type(&cctx, i64_val, i8_ty, false)
            .expect_err("expected unset-position error on truncation");
        assert!(
            err.to_string().contains("position"),
            "unexpected error: {}",
            err
        );
        // Same-width coercion is a no-op: it never touches the builder, so it
        // succeeds even without an insert block.
        let same = coerce_int_to_llvm_type(&cctx, i8_val, i8_ty, true).expect("no-op coerce");
        assert_eq!(same, i8_val);
    }

    // ── error paths: more `compute_lvalue_ptr` shapes ──

    #[test]
    fn lvalue_of_non_lvalue_expression_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("lvalue_nonlvalue_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let fn_ty = ctx.i32_type().fn_type(&[], false);
        let function = module.add_function("probe", fn_ty, None);
        let mut fl =
            FunctionLowering::new(&cctx, function, HashMap::new(), SymbolTable::new(), false);
        let expr = TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Int4),
            expression: TypedExprKind::LiteralInt { value: 1 },
        };
        let err = fl
            .compute_lvalue_ptr(&expr)
            .expect_err("expected not-an-lvalue error");
        assert!(
            err.to_string().contains("not an lvalue"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn lvalue_field_access_on_non_struct_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("lvalue_field_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let i32_ty = ctx.i32_type();
        let fn_ty = i32_ty.fn_type(&[], false);
        let function = module.add_function("f", fn_ty, None);
        let entry = ctx.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let alloca = builder.build_alloca(i32_ty, "s_addr").expect("alloca");
        let mut vars = HashMap::new();
        vars.insert(
            Identifier {
                value: "s".to_string(),
            },
            alloca,
        );
        let scope = SymbolTable::new();
        let object = TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Int4),
            expression: TypedExprKind::Identifier(Identifier {
                value: "s".to_string(),
            }),
        };
        let expr = TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Int4),
            expression: TypedExprKind::FieldAccess {
                object: Box::new(object),
                field: "x".to_string(),
                field_index: 0,
            },
        };
        let mut fl = FunctionLowering::new(&cctx, function, vars, scope, false);
        let err = fl
            .compute_lvalue_ptr(&expr)
            .expect_err("expected non-struct error");
        assert!(
            err.to_string().contains("non-struct"),
            "unexpected error: {}",
            err
        );
    }

    // ── error paths: tuple helpers around coercion ──

    #[test]
    fn build_tuple_value_arity_mismatch_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("tuple_arity_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let fn_ty = ctx.i32_type().fn_type(&[], false);
        let function = module.add_function("probe", fn_ty, None);
        let mut fl =
            FunctionLowering::new(&cctx, function, HashMap::new(), SymbolTable::new(), false);
        let i32_ty = ctx.i32_type();
        let tuple_ty = ctx.struct_type(&[i32_ty.into(), i32_ty.into()], false);
        let expr = TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Int4),
            expression: TypedExprKind::LiteralInt { value: 1 },
        };
        // Two fields but only one value: the arity check fires before any
        // coercion or builder use.
        let err = fl
            .build_tuple_value(&[expr], tuple_ty)
            .expect_err("expected arity error");
        assert!(
            err.to_string().contains("return arity mismatch"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn unpack_tuple_value_of_non_tuple_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("unpack_arity_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let int_val = ctx.i32_type().const_int(0, false).as_basic_value_enum();
        let err = unpack_tuple_value(&cctx, int_val, 1).expect_err("expected tuple error");
        assert!(
            err.to_string().contains("multiple-return tuple"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn call_result_of_void_call_errors() {
        let ctx = Context::create();
        let module = ctx.create_module("call_result_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let void_ty = ctx.void_type();
        let fn_ty = void_ty.fn_type(&[], false);
        let function = module.add_function("void_fn", fn_ty, None);
        let entry = ctx.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let call_site = builder.build_call(function, &[], "voidcall").expect("call");
        let err = call_result(call_site).expect_err("expected void-call error");
        assert!(
            err.to_string().contains("expected a return value"),
            "unexpected error: {}",
            err
        );
        builder.build_return(None).expect("ret");
        let _ = cctx;
    }

    // ── audit 02 §1: typed LowerError ────────────────────────────────────

    #[test]
    fn insert_block_without_position_reports_missing_block() {
        // A fresh builder has no insert block: `insert_block` must return the
        // typed `MissingBlock` (not a stringly `Box<dyn Error>`).
        let ctx = Context::create();
        let module = ctx.create_module("missing_block_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        let err =
            super::super::context::insert_block(&cctx, "probe-site").expect_err("expected error");
        match &err {
            LowerError::MissingBlock { what, .. } => assert_eq!(what, "probe-site"),
            other => panic!("expected MissingBlock, got {:?}", other),
        }
        assert!(
            err.to_string().contains("no insert block"),
            "unexpected rendering: {}",
            err
        );
    }

    #[test]
    fn missing_block_rendering_carries_fn_and_line() {
        let err = LowerError::missing_block("probe", Some("my_fn".to_string()), Some(3));
        let msg = err.to_string();
        assert!(msg.contains("probe"), "missing site: {}", msg);
        assert!(msg.contains("my_fn"), "missing fn name: {}", msg);
        assert!(msg.contains('3'), "missing line: {}", msg);
        assert!(msg.contains("no insert block"), "missing kind: {}", msg);
        // Bare form (free `insert_block` with no function context) keeps the
        // historical rendering.
        let bare = LowerError::missing_block("probe", None, None);
        assert_eq!(bare.to_string(), "lowering 'probe': no insert block");
    }

    #[test]
    fn unknown_layout_is_typed_and_keeps_prefix() {
        // `types.rs` ad-hoc `UnknownLayout: ...` strings are now the typed
        // `UnknownLayout` variant; the `UnknownLayout: ` prefix is kept so
        // existing diagnostics/tests matching on it still work.
        let ctx = Context::create();
        let scope = SymbolTable::new();
        let ty = Ty::Identifier(Identifier {
            value: "Missing".to_string(),
        });
        let err = map_type_to_llvm(&ty, &ctx, scope).expect_err("expected UnknownLayout");
        match &err {
            LowerError::UnknownLayout { name } => assert!(
                name.contains("Missing"),
                "variant should name the type, got: {}",
                name
            ),
            other => panic!("expected UnknownLayout, got {:?}", other),
        }
        assert!(
            err.to_string().contains("UnknownLayout"),
            "prefix lost: {}",
            err
        );
    }

    #[test]
    fn builder_errors_convert_to_lower_error() {
        // `From<BuilderError>` keeps `?` working after the
        // `Box<dyn Error>` -> `LowerError` conversion.
        let err = LowerError::from("plain string still becomes Unsupported".to_string());
        assert!(matches!(err, LowerError::Unsupported(_)));
        let err = LowerError::from("static str likewise");
        assert!(matches!(err, LowerError::Unsupported(_)));
    }

    // ── debug bounds checks ─────────────────────────────────────────────

    /// Analyze + lower directly (the IR middle-end rejects index/slice, so
    /// this exercises the `TypedProgram -> LLVM` path that owns the checks).
    fn lower_direct_src(src: &str, bounds_checks: bool) -> String {
        let tokens: Vec<_> = Lexer::new(src).collect();
        let path = PathBuf::from("test.fib");
        let mut parser = Parser::new(tokens.into_iter(), &path, src.to_string());
        let ast = parser.parse().expect("parse failed");
        let typed =
            crate::frontend::analyze::analyze(ast, &Default::default()).expect("analysis failed");
        super::super::llvm_lower::lower(typed, "test", bounds_checks).expect("lower failed")
    }

    #[test]
    fn index_access_emits_oob_trap_in_debug() {
        let ir = lower_direct_src(
            "fn main() @int4 { arr: @int4[4] = [1, 2, 3, 4];\ni: @int4 = 2;\nreturn arr.[i]; }",
            true,
        );
        assert!(ir.contains("oob_trap"), "missing trap block:\n{}", ir);
        assert!(ir.contains("oob_cont"), "missing cont block:\n{}", ir);
        assert!(ir.contains("dprintf"), "missing stderr report:\n{}", ir);
        assert!(ir.contains("abort"), "missing abort:\n{}", ir);
        assert!(
            ir.contains("index out of bounds"),
            "missing message:\n{}",
            ir
        );
    }

    #[test]
    fn index_access_omits_oob_trap_in_release() {
        let ir = lower_direct_src(
            "fn main() @int4 { arr: @int4[4] = [1, 2, 3, 4];\ni: @int4 = 2;\nreturn arr.[i]; }",
            false,
        );
        assert!(
            !ir.contains("oob_trap"),
            "trap leaked into release:\n{}",
            ir
        );
        assert!(
            !ir.contains("dprintf"),
            "report leaked into release:\n{}",
            ir
        );
    }

    #[test]
    fn slice_range_emits_oob_trap_in_debug() {
        let ir = lower_direct_src(
            "fn f(a: @int4, b: @int4) @void { arr: @int4[4] = [1, 2, 3, 4];\ns: @int4[] = arr.[a..b]; }",
            true,
        );
        assert!(
            ir.contains("slice out of bounds"),
            "missing slice message:\n{}",
            ir
        );
        assert!(ir.contains("oob_trap"), "missing trap block:\n{}", ir);
    }

    #[test]
    fn slice_range_omits_oob_trap_in_release() {
        let ir = lower_direct_src(
            "fn f(a: @int4, b: @int4) @void { arr: @int4[4] = [1, 2, 3, 4];\ns: @int4[] = arr.[a..b]; }",
            false,
        );
        assert!(
            !ir.contains("oob_trap"),
            "trap leaked into release:\n{}",
            ir
        );
    }
}
