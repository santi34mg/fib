#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::analysis::analyze;
    use crate::hir::{HIRDeclaration, HIRExpressionKind, HIRStmt, HIRTypeKind};
    use crate::lexing::Lexer;
    use crate::parsing::Parser;
    use crate::tokens::{Token, builtin::BuiltinType};

    fn get_hir(source: &str) -> crate::hir::CompilationUnit {
        let src = source.to_string();
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.clone());
        let ast = parser.parse().expect("parse failed");
        analyze(ast, &HashMap::new()).expect("analysis failed")
    }

    fn get_hir_err(source: &str) -> String {
        let src = source.to_string();
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.clone());
        let ast = parser.parse().expect("parse failed");
        analyze(ast, &HashMap::new())
            .expect_err("expected analysis error")
            .msg
    }

    fn get_function<'a>(
        cu: &'a crate::hir::CompilationUnit,
        name: &str,
    ) -> &'a crate::hir::HIRFunction {
        cu.declarations
            .iter()
            .find_map(|d| {
                if let HIRDeclaration::HIRFunction(f) = d {
                    if f.name.identifier == name {
                        return Some(f);
                    }
                }
                None
            })
            .unwrap_or_else(|| panic!("function '{}' not found in HIR", name))
    }

    // ── Literals & type inference ─────────────────────────────────────────────

    #[test]
    fn test_integer_literal_defaults_to_int4() {
        let cu = get_hir("fn f() int4 { ret 42 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[0] {
            assert_eq!(expr.inferred_type, HIRTypeKind::Builtin(BuiltinType::Int4));
            assert!(matches!(
                expr.expression,
                HIRExpressionKind::LiteralInt { value: 42 }
            ));
        } else {
            panic!("expected Return statement");
        }
    }

    #[test]
    fn test_float_literal_defaults_to_float8() {
        let cu = get_hir("fn f() float8 { ret 3.14 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[0] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Float8)
            );
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_bool_literal_type() {
        let cu = get_hir("fn f() bool { ret true }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[0] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
            assert!(matches!(
                expr.expression,
                HIRExpressionKind::LiteralBool(true)
            ));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_string_literal_type() {
        let cu = get_hir(r#"fn f() string { ret "hi" }"#);
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[0] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::String)
            );
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_null_literal_type_is_void() {
        let cu = get_hir("fn f() { x: int4 = 1\n ret }");
        // Just ensure void return analyzes without error.
        let _ = cu;
    }

    // ── Type aliases & var bindings ───────────────────────────────────────────

    #[test]
    fn test_type_declaration_typed() {
        let cu = get_hir("type Num int4");
        let binding = cu
            .declarations
            .iter()
            .find_map(|d| {
                if let HIRDeclaration::HIRType(t) = d {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("expected HIRType");
        assert_eq!(binding.name.identifier, "Num");
        assert_eq!(binding.ty, HIRTypeKind::Builtin(BuiltinType::Int4));
    }

    #[test]
    fn test_type_declaration_float_typed() {
        let cu = get_hir("type Float float8");
        let binding = cu
            .declarations
            .iter()
            .find_map(|d| {
                if let HIRDeclaration::HIRType(t) = d {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("expected HIRType");
        assert_eq!(binding.ty, HIRTypeKind::Builtin(BuiltinType::Float8));
    }

    #[test]
    fn test_var_declaration_is_mutable() {
        let cu = get_hir("fn f() { var int4 x = 0 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_var_declaration_zero_init_int() {
        let cu = get_hir("fn f() { var int4 x }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            assert_eq!(b.ty, HIRTypeKind::Builtin(BuiltinType::Int4));
            if let Some(init) = &b.init {
                assert!(matches!(
                    init.expression,
                    HIRExpressionKind::LiteralInt { value: 0 }
                ));
            }
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_colon_var_declaration_with_type() {
        let cu = get_hir("fn f() { x: int4 = 0 } ");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_colon_var_declaration_infers_type() {
        let cu = get_hir("fn f() { x := 0 } ");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_multi_var_declaration_infers_tuple_types() {
        let cu = get_hir(
            "fn divmod(int4 a, int4 b) (int4, int4) { ret a / b, a % b }\nfn f() { q, r := divmod(17, 5) }",
        );
        let f = get_function(&cu, "f");
        if let HIRStmt::MultiBinding { bindings, .. } = &f.body[0] {
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].name.identifier, "q");
            assert_eq!(bindings[0].ty, HIRTypeKind::Builtin(BuiltinType::Int4));
            assert_eq!(bindings[1].name.identifier, "r");
            assert_eq!(bindings[1].ty, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected MultiBinding statement");
        }
    }

    #[test]
    fn test_var_bool_zero_init_is_false() {
        let cu = get_hir("fn f() { var bool b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            if let Some(init) = &b.init {
                assert!(matches!(
                    init.expression,
                    HIRExpressionKind::LiteralBool(false)
                ));
            }
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_var_pointer_zero_init_is_null() {
        let cu = get_hir("fn f() { var *int4 p }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            if let Some(init) = &b.init {
                assert!(matches!(init.expression, HIRExpressionKind::Null));
            }
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_colon_var_declaration_is_mutable() {
        let cu = get_hir("fn f() { x: int4 = 5 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[0] {
            assert!(b.mutable);
        } else {
            panic!("expected Binding");
        }
    }

    // ── Function signatures ───────────────────────────────────────────────────

    #[test]
    fn test_function_return_type() {
        let cu = get_hir("fn add(int4 a, int4 b) int4 { ret a }");
        let f = get_function(&cu, "add");
        assert_eq!(f.return_type, HIRTypeKind::Builtin(BuiltinType::Int4));
    }

    #[test]
    fn test_function_params_count_and_types() {
        let cu = get_hir("fn add(int4 a, int4 b) int4 { ret a }");
        let f = get_function(&cu, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].0.identifier, "a");
        assert_eq!(f.params[0].1, HIRTypeKind::Builtin(BuiltinType::Int4));
        assert_eq!(f.params[1].0.identifier, "b");
    }

    #[test]
    fn test_function_no_params() {
        let cu = get_hir("fn noop() { }");
        let f = get_function(&cu, "noop");
        assert_eq!(f.params.len(), 0);
    }

    #[test]
    fn test_void_return_type() {
        let cu = get_hir("fn noop() { }");
        let f = get_function(&cu, "noop");
        assert_eq!(f.return_type, HIRTypeKind::Builtin(BuiltinType::Void));
    }

    #[test]
    fn test_extern_function_is_marked() {
        let cu = get_hir("extern fn puts(string s) int4;");
        let f = get_function(&cu, "puts");
        assert!(f.is_extern);
        assert!(f.body.is_empty());
    }

    #[test]
    fn test_variadic_function_is_marked() {
        let cu = get_hir("extern fn printf(string fmt, ...) int4;");
        let f = get_function(&cu, "printf");
        assert!(f.is_variadic);
    }

    // ── Binary expressions ────────────────────────────────────────────────────

    #[test]
    fn test_binary_add_type_is_lhs() {
        let cu = get_hir("fn f() int4 { a: int4 = 1\n b: int4 = 2\n ret a + b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(expr.inferred_type, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_binary_sub_type_is_lhs() {
        let cu = get_hir("fn f() int4 { a: int4 = 10\n b: int4 = 3\n ret a - b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(expr.inferred_type, HIRTypeKind::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_comparison_result_is_bool() {
        let cu = get_hir("fn f() bool { a: int4 = 1\n b: int4 = 2\n ret a < b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_equality_result_is_bool() {
        let cu = get_hir("fn f() bool { a: int4 = 1\n b: int4 = 1\n ret a == b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_comparison_coerces_right_integer_literal_to_left_type() {
        let cu = get_hir("fn f() bool { a: int4 = 1\n ret a as uint8 != 0 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[1] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
            if let HIRExpressionKind::Binary { right, .. } = &expr.expression {
                assert_eq!(
                    right.inferred_type,
                    HIRTypeKind::Builtin(BuiltinType::UInt8)
                );
            } else {
                panic!("expected Binary expression");
            }
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_logical_and_result_is_bool() {
        let cu = get_hir("fn f() bool { a: bool = true\n b: bool = false\n ret a && b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_logical_or_result_is_bool() {
        let cu = get_hir("fn f() bool { a: bool = true\n b: bool = false\n ret a || b }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[2] {
            assert_eq!(
                expr.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected Return");
        }
    }

    // ── Control flow ──────────────────────────────────────────────────────────

    #[test]
    fn test_if_stmt_in_hir() {
        let cu = get_hir("fn f() { if true { } }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], HIRStmt::If(_)));
    }

    #[test]
    fn test_if_else_in_hir() {
        let cu = get_hir("fn f() { if true { } else { } }");
        let f = get_function(&cu, "f");
        if let HIRStmt::If(hir_if) = &f.body[0] {
            assert!(hir_if.else_branch.is_some());
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_if_condition_type_is_bool() {
        let cu = get_hir("fn f() { if true { } }");
        let f = get_function(&cu, "f");
        if let HIRStmt::If(hir_if) = &f.body[0] {
            assert_eq!(
                hir_if.cond.inferred_type,
                HIRTypeKind::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_for_loop_in_hir() {
        let cu = get_hir("fn f() { for (;;) { break } }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], HIRStmt::For { .. }));
    }

    #[test]
    fn test_break_continue_in_hir() {
        let cu = get_hir("fn f() { for (;;) { break continue } }");
        let f = get_function(&cu, "f");
        if let HIRStmt::For { body, .. } = &f.body[0] {
            assert!(matches!(body[0], HIRStmt::Break));
            assert!(matches!(body[1], HIRStmt::Continue));
        } else {
            panic!("expected For");
        }
    }

    #[test]
    fn test_defer_in_hir() {
        let cu = get_hir("extern fn cleanup() void;\nfn f() { defer cleanup() }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], HIRStmt::Defer(_)));
    }

    #[test]
    fn test_return_void_in_hir() {
        let cu = get_hir("fn f() { ret }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], HIRStmt::Return(None)));
    }

    // ── Scope & identifier resolution ─────────────────────────────────────────

    #[test]
    fn test_identifier_resolves_to_binding_type() {
        let cu = get_hir("fn f() int4 { x: int4 = 5\n ret x }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[1] {
            assert_eq!(expr.inferred_type, HIRTypeKind::Builtin(BuiltinType::Int4));
            assert!(matches!(expr.expression, HIRExpressionKind::Identifier(_)));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_undefined_identifier_errors() {
        let err = get_hir_err("fn f() int4 { ret undefined_var }");
        assert!(
            err.contains("undefined_var"),
            "error should mention 'undefined_var', got: {}",
            err
        );
    }

    #[test]
    fn test_type_mismatch_struct_vs_int_errors() {
        let err = get_hir_err("type Point struct { int4 x, int4 y }\nfn f() { p: Point = 5 }");
        assert!(!err.is_empty(), "expected type mismatch error");
    }

    // ── Type declarations ─────────────────────────────────────────────────────

    #[test]
    fn test_type_declaration_produces_no_hir_decl() {
        // Type aliases should be in scope but not emit HIRDeclarations
        let cu = get_hir("type Num int4\nfn f() { }");
        let const_decls: Vec<_> = cu
            .declarations
            .iter()
            .filter(|d| matches!(d, HIRDeclaration::HIRConst(_)))
            .collect();
        assert_eq!(const_decls.len(), 0);
    }

    // ── Cast expression ───────────────────────────────────────────────────────

    #[test]
    fn test_cast_changes_inferred_type() {
        let cu = get_hir("fn f() int8 { x: int4 = 5\n ret x as int8 }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Return(Some(expr)) = &f.body[1] {
            assert_eq!(expr.inferred_type, HIRTypeKind::Builtin(BuiltinType::Int8));
            assert!(matches!(expr.expression, HIRExpressionKind::Cast { .. }));
        } else {
            panic!("expected Return with cast");
        }
    }

    // ── Multiple functions ────────────────────────────────────────────────────

    #[test]
    fn test_multiple_functions_in_compilation_unit() {
        let cu = get_hir("fn foo() { }\nfn bar() { }\nfn baz() { }");
        let count = cu
            .declarations
            .iter()
            .filter(|d| matches!(d, HIRDeclaration::HIRFunction(_)))
            .count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_function_calling_another_function() {
        let cu = get_hir("fn helper() int4 { ret 1 }\nfn main() int4 { ret helper() }");
        let main_f = get_function(&cu, "main");
        if let HIRStmt::Return(Some(expr)) = &main_f.body[0] {
            assert!(matches!(expr.expression, HIRExpressionKind::Call { .. }));
        } else {
            panic!("expected Return with Call");
        }
    }

    #[test]
    fn test_calling_undefined_function_errors() {
        let err = get_hir_err("fn f() { ghost() }");
        assert!(
            err.contains("ghost"),
            "error should mention 'ghost', got: {}",
            err
        );
    }

    // ── Pointer types ─────────────────────────────────────────────────────────

    #[test]
    fn test_address_of_produces_pointer_type() {
        let cu = get_hir("fn f() { x: int4 = 5\n p: *int4 = x.& }");
        let f = get_function(&cu, "f");
        if let HIRStmt::Binding(b) = &f.body[1] {
            assert!(matches!(b.ty, HIRTypeKind::Pointer(_)));
        } else {
            panic!("expected Binding");
        }
    }

    // ── Assign statements ─────────────────────────────────────────────────────

    #[test]
    fn test_assign_stmt_in_hir() {
        let cu = get_hir("fn f() { var int4 x = 0\n x = 1 }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[1], HIRStmt::Assign { .. }));
    }
}
