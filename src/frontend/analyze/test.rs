#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::frontend::analyze::analyze;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use crate::frontend::tokens::{Token, builtin::BuiltinType};
    use crate::frontend::typed_ast::{
        Ty, TypedDecl, TypedExpr, TypedExprKind, TypedFunction, TypedProgram, TypedStatement,
    };

    fn get_typed(source: &str) -> TypedProgram {
        let src = source.to_string();
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.clone());
        let ast = parser.parse().expect("parse failed");
        analyze(ast, &HashMap::new()).expect("analysis failed")
    }

    fn get_typed_err(source: &str) -> String {
        let src = source.to_string();
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.clone());
        let ast = parser.parse().expect("parse failed");
        analyze(ast, &HashMap::new())
            .expect_err("expected analysis error")
            .msg
    }

    fn get_function<'a>(cu: &'a TypedProgram, name: &str) -> &'a TypedFunction {
        cu.declarations
            .iter()
            .find_map(|d| {
                if let TypedDecl::Function(f) = d
                    && f.name.value == name
                {
                    return Some(f);
                }
                None
            })
            .unwrap_or_else(|| panic!("function '{}' not found in Typed", name))
    }

    // ── Literals & type inference ─────────────────────────────────────────────

    #[test]
    fn test_integer_literal_defaults_to_int4() {
        let cu = get_typed("fn f() @int4 { return 42 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[0] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Int4));
            assert!(matches!(
                expr.expression,
                TypedExprKind::LiteralInt { value: 42 }
            ));
        } else {
            panic!("expected Return statement");
        }
    }

    #[test]
    fn test_float_literal_defaults_to_float8() {
        let cu = get_typed("fn f() @float8 { return 3.14 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[0] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Float8));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_bool_literal_type() {
        let cu = get_typed("fn f() @bool { return true }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[0] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
            assert!(matches!(expr.expression, TypedExprKind::LiteralBool(true)));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_string_literal_type() {
        let cu = get_typed(r#"fn f() @string { return "hi" }"#);
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[0] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::String));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_null_literal_type_is_void() {
        let cu = get_typed("fn f() { x: @int4 = 1\n return }");
        // Just ensure void return analyzes without error.
        let _ = cu;
    }

    // ── Type aliases & var bindings ───────────────────────────────────────────

    #[test]
    fn test_type_declaration_typed() {
        let cu = get_typed("type Num @int4");
        let binding = cu
            .declarations
            .iter()
            .find_map(|d| {
                if let TypedDecl::Type(t) = d {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("expected TypedType");
        assert_eq!(binding.name.value, "Num");
        assert_eq!(binding.ty, Ty::Builtin(BuiltinType::Int4));
    }

    #[test]
    fn test_type_declaration_float_typed() {
        let cu = get_typed("type Float @float8");
        let binding = cu
            .declarations
            .iter()
            .find_map(|d| {
                if let TypedDecl::Type(t) = d {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("expected TypedType");
        assert_eq!(binding.ty, Ty::Builtin(BuiltinType::Float8));
    }

    #[test]
    fn test_var_declaration_is_mutable() {
        let cu = get_typed("fn f() { var @int4 x = 0 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_var_declaration_without_init_is_uninitialized() {
        let cu = get_typed("fn f() { var @int4 x }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert_eq!(b.ty, Ty::Builtin(BuiltinType::Int4));
            assert!(b.init.is_none());
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_colon_var_declaration_with_type() {
        let cu = get_typed("fn f() { x: @int4 = 0 } ");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_colon_var_declaration_infers_type() {
        let cu = get_typed("fn f() { x := 0 } ");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.mutable);
            assert_eq!(b.ty, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Binding statement");
        }
    }

    #[test]
    fn test_multi_var_declaration_infers_tuple_types() {
        let cu = get_typed(
            "fn divmod(a: @int4, b: @int4) (@int4, @int4) { return a / b, a % b }\nfn f() { q, r := divmod(17, 5) }",
        );
        let f = get_function(&cu, "f");
        if let TypedStatement::MultiBinding { bindings, .. } = &f.body[0] {
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].name.value, "q");
            assert_eq!(bindings[0].ty, Ty::Builtin(BuiltinType::Int4));
            assert_eq!(bindings[1].name.value, "r");
            assert_eq!(bindings[1].ty, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected MultiBinding statement");
        }
    }

    #[test]
    fn test_var_bool_without_init_is_uninitialized() {
        let cu = get_typed("fn f() { var @bool b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.init.is_none());
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_var_pointer_without_init_is_uninitialized() {
        let cu = get_typed("fn f() { var *@int4 p }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.init.is_none());
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn test_colon_var_declaration_is_mutable() {
        let cu = get_typed("fn f() { x: @int4 = 5 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert!(b.mutable);
        } else {
            panic!("expected Binding");
        }
    }

    // ── Function signatures ───────────────────────────────────────────────────

    #[test]
    fn test_function_return_type() {
        let cu = get_typed("fn add(a: @int4, b: @int4) @int4 { return a }");
        let f = get_function(&cu, "add");
        assert_eq!(f.return_type, Ty::Builtin(BuiltinType::Int4));
    }

    #[test]
    fn test_function_params_count_and_types() {
        let cu = get_typed("fn add(a: @int4, b: @int4) @int4 { return a }");
        let f = get_function(&cu, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].0.value, "a");
        assert_eq!(f.params[0].1, Ty::Builtin(BuiltinType::Int4));
        assert_eq!(f.params[1].0.value, "b");
    }

    #[test]
    fn test_function_no_params() {
        let cu = get_typed("fn noop() { }");
        let f = get_function(&cu, "noop");
        assert_eq!(f.params.len(), 0);
    }

    #[test]
    fn test_void_return_type() {
        let cu = get_typed("fn noop() { }");
        let f = get_function(&cu, "noop");
        assert_eq!(f.return_type, Ty::Builtin(BuiltinType::Void));
    }

    #[test]
    fn test_extern_function_is_marked() {
        let cu = get_typed("extern fn puts(s: @string) @int4;");
        let f = get_function(&cu, "puts");
        assert!(f.is_extern);
        assert!(f.body.is_empty());
    }

    #[test]
    fn test_variadic_function_is_marked() {
        let cu = get_typed("extern fn printf(fmt: @string, ...) @int4;");
        let f = get_function(&cu, "printf");
        assert!(f.is_variadic);
    }

    // ── Binary expressions ────────────────────────────────────────────────────

    #[test]
    fn test_binary_add_type_is_lhs() {
        let cu = get_typed("fn f() @int4 { a: @int4 = 1\n b: @int4 = 2\n return a + b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_binary_sub_type_is_lhs() {
        let cu = get_typed("fn f() @int4 { a: @int4 = 10\n b: @int4 = 3\n return a - b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Int4));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_comparison_result_is_bool() {
        let cu = get_typed("fn f() @bool { a: @int4 = 1\n b: @int4 = 2\n return a < b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_equality_result_is_bool() {
        let cu = get_typed("fn f() @bool { a: @int4 = 1\n b: @int4 = 1\n return a == b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_comparison_coerces_right_integer_literal_to_left_type() {
        let cu = get_typed("fn f() @bool { a: @int4 = 1\n return a as @uint8 != 0 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[1] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
            if let TypedExprKind::Binary { right, .. } = &expr.expression {
                assert_eq!(right.inferred_type, Ty::Builtin(BuiltinType::UInt8));
            } else {
                panic!("expected Binary expression");
            }
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_logical_and_result_is_bool() {
        let cu = get_typed("fn f() @bool { a: @bool = true\n b: @bool = false\n return a && b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_logical_or_result_is_bool() {
        let cu = get_typed("fn f() @bool { a: @bool = true\n b: @bool = false\n return a || b }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[2] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        } else {
            panic!("expected Return");
        }
    }

    // ── Control flow ──────────────────────────────────────────────────────────

    #[test]
    fn test_if_stmt_in_hir() {
        let cu = get_typed("fn f() { if true { } }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], TypedStatement::If(_)));
    }

    #[test]
    fn test_if_else_in_hir() {
        let cu = get_typed("fn f() { if true { } else { } }");
        let f = get_function(&cu, "f");
        if let TypedStatement::If(typed_if) = &f.body[0] {
            assert!(typed_if.else_branch.is_some());
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_if_condition_type_is_bool() {
        let cu = get_typed("fn f() { if true { } }");
        let f = get_function(&cu, "f");
        if let TypedStatement::If(typed_if) = &f.body[0] {
            assert_eq!(
                typed_if.cond.inferred_type,
                Ty::Builtin(BuiltinType::Boolean)
            );
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_for_loop_in_hir() {
        let cu = get_typed("fn f() { for (;;) { break } }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], TypedStatement::For { .. }));
    }

    #[test]
    fn test_break_continue_in_hir() {
        let cu = get_typed("fn f() { for (;;) { break continue } }");
        let f = get_function(&cu, "f");
        if let TypedStatement::For { body, .. } = &f.body[0] {
            assert!(matches!(body[0], TypedStatement::Break));
            assert!(matches!(body[1], TypedStatement::Continue));
        } else {
            panic!("expected For");
        }
    }

    #[test]
    fn test_defer_in_hir() {
        let cu = get_typed("extern fn cleanup() @void;\nfn f() { defer cleanup() }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], TypedStatement::Defer(_)));
    }

    #[test]
    fn test_return_void_in_hir() {
        let cu = get_typed("fn f() { return }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[0], TypedStatement::Return(None)));
    }

    // ── SymbolTable & identifier resolution ─────────────────────────────────────────

    #[test]
    fn test_identifier_resolves_to_binding_type() {
        let cu = get_typed("fn f() @int4 { x: @int4 = 5\n return x }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[1] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Int4));
            assert!(matches!(expr.expression, TypedExprKind::Identifier(_)));
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn test_undefined_identifier_errors() {
        let err = get_typed_err("fn f() @int4 { return undefined_var }");
        assert!(
            err.contains("undefined_var"),
            "error should mention 'undefined_var', got: {}",
            err
        );
    }

    #[test]
    fn test_type_mismatch_struct_vs_int_errors() {
        let err =
            get_typed_err("type Point struct { x: @int4, y: @int4 }\nfn f() { p: Point = 5 }");
        assert!(!err.is_empty(), "expected type mismatch error");
    }

    // ── Type declarations ─────────────────────────────────────────────────────

    #[test]
    fn test_type_declaration_produces_no_typed_decl() {
        // Type aliases should be in scope but not emit TypedDecls
        let cu = get_typed("type Num @int4\nfn f() { }");
        let const_decls: Vec<_> = cu
            .declarations
            .iter()
            .filter(|d| matches!(d, TypedDecl::Const(_)))
            .collect();
        assert_eq!(const_decls.len(), 0);
    }

    // ── Cast expression ───────────────────────────────────────────────────────

    #[test]
    fn test_cast_changes_inferred_type() {
        let cu = get_typed("fn f() @int8 { x: @int4 = 5\n return x as @int8 }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Return(Some(ret)) = &f.body[1] {
            let expr = ret.first().expect("test expects a return value");
            assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Int8));
            assert!(matches!(expr.expression, TypedExprKind::Cast { .. }));
        } else {
            panic!("expected Return with cast");
        }
    }

    // ── Multiple functions ────────────────────────────────────────────────────

    #[test]
    fn test_multiple_functions_in_compilation_unit() {
        let cu = get_typed("fn foo() { }\nfn bar() { }\nfn baz() { }");
        let count = cu
            .declarations
            .iter()
            .filter(|d| matches!(d, TypedDecl::Function(_)))
            .count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_function_calling_another_function() {
        let cu = get_typed("fn helper() @int4 { return 1 }\nfn main() @int4 { return helper() }");
        let main_f = get_function(&cu, "main");
        if let TypedStatement::Return(Some(ret)) = &main_f.body[0] {
            let expr = ret.first().expect("test expects a return value");
            assert!(matches!(expr.expression, TypedExprKind::Call { .. }));
        } else {
            panic!("expected Return with Call");
        }
    }

    #[test]
    fn test_calling_undefined_function_errors() {
        let err = get_typed_err("fn f() { ghost() }");
        assert!(
            err.contains("ghost"),
            "error should mention 'ghost', got: {}",
            err
        );
    }

    // ── Pointer types ─────────────────────────────────────────────────────────

    #[test]
    fn test_address_of_produces_pointer_type() {
        let cu = get_typed("fn f() { x: @int4 = 5\n p: *@int4 = x.& }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[1] {
            assert!(matches!(b.ty, Ty::Pointer(_)));
        } else {
            panic!("expected Binding");
        }
    }

    // ── Assign statements ─────────────────────────────────────────────────────

    #[test]
    fn test_assign_stmt_in_hir() {
        let cu = get_typed("fn f() { var @int4 x = 0\n x = 1 }");
        let f = get_function(&cu, "f");
        assert!(matches!(f.body[1], TypedStatement::Assign { .. }));
    }

    // ── Builtin string functions ──────────────────────────────────────────────

    #[test]
    fn test_builtin_string_calls_infer_types() {
        let cases = [
            (r#"fn f() { x := @str_len("a") }"#, BuiltinType::UInt8),
            (r#"fn f() { x := @str_eq("a", "b") }"#, BuiltinType::Boolean),
            (r#"fn f() { x := @concat("a", "b") }"#, BuiltinType::String),
        ];
        for (src, expected) in cases {
            let cu = get_typed(src);
            let f = get_function(&cu, "f");
            if let TypedStatement::Binding(b) = &f.body[0] {
                let init = b.init.as_ref().expect("binding initializer");
                assert_eq!(init.inferred_type, Ty::Builtin(expected));
                assert!(matches!(init.expression, TypedExprKind::BuiltinCall { .. }));
            } else {
                panic!("expected Binding statement for source {:?}", src);
            }
        }
    }

    #[test]
    fn test_builtin_string_call_rejects_non_string_arg() {
        let err = get_typed_err("fn f() { x := @str_len(5) }");
        assert!(err.contains("@str_len"), "unexpected error: {}", err);
    }

    #[test]
    fn test_builtin_string_call_checks_arity() {
        let err = get_typed_err(r#"fn f() { x := @concat("a") }"#);
        assert!(err.contains("@concat"), "unexpected error: {}", err);
    }

    // ── 02 reliability: assignment soundness ──────────────────────────

    #[test]
    fn deref_assign_coerces_numeric_literal() {
        // `1` widens to the pointee type instead of being stored unchecked.
        get_typed("fn f(p: *@int4) @void { p.* = 1 }");
    }

    #[test]
    fn deref_assign_rejects_type_mismatch() {
        let err = get_typed_err(r#"fn f(p: *@int4) @void { p.* = "s" }"#);
        assert!(
            err.contains("cannot assign value of type"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn deref_assign_rejects_non_pointer() {
        let err = get_typed_err("fn f() { x := 1\nx.* = 2 }");
        assert!(err.contains("non-pointer"), "unexpected error: {}", err);
    }

    #[test]
    fn field_assign_coerces_numeric_literal() {
        get_typed(
            "type Point struct { x: @int4, y: @int4 }\nfn f() { p: Point = Point { x: 1, y: 2 }\np.x = 3 }",
        );
    }

    #[test]
    fn field_assign_rejects_type_mismatch() {
        let err = get_typed_err(
            "type Point struct { x: @int4, y: @int4 }\nfn f() { p: Point = Point { x: 1, y: 2 }\np.x = \"s\" }",
        );
        assert!(
            err.contains("cannot assign value of type"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn index_assign_accepts_int_index_and_coerces_value() {
        get_typed("fn f() { arr: @int4[2] = [1, 2]\narr.[0] = 3 }");
    }

    #[test]
    fn index_assign_rejects_float_index() {
        let err = get_typed_err("fn f() { arr: @int4[2] = [1, 2]\narr.[1.5] = 3 }");
        assert!(err.contains("integer index"), "unexpected error: {}", err);
    }

    #[test]
    fn index_assign_rejects_value_mismatch() {
        let err = get_typed_err("fn f() { arr: @int4[2] = [1, 2]\narr.[0] = \"s\" }");
        assert!(err.contains("array element"), "unexpected error: {}", err);
    }

    #[test]
    fn index_access_rejects_float_index() {
        let err = get_typed_err("fn f() @int4 { arr: @int4[2] = [1, 2]\nreturn arr.[1.5] }");
        assert!(err.contains("integer index"), "unexpected error: {}", err);
    }

    #[test]
    fn array_literal_accepts_numeric_mix() {
        // Mixed-width ints unify via the shared numeric coercion rule.
        let cu = get_typed("fn f() { x: @int8 = 300\na := [x, 1] }");
        let f = get_function(&cu, "f");
        assert!(
            matches!(
                &f.body[1],
                TypedStatement::Binding(b) if matches!(
                    &b.init.as_ref().expect("init").expression,
                    TypedExprKind::ArrayLiteral { .. }
                )
            ),
            "expected array binding"
        );
    }

    #[test]
    fn array_literal_accepts_null_tail() {
        get_typed("fn f() { a := [1, null] }");
    }

    #[test]
    fn array_literal_rejects_incompatible_mix() {
        let err = get_typed_err(r#"fn f() { a := [1, "s"] }"#);
        assert!(
            err.contains("incompatible type"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn array_binding_coerces_elements_to_declared_type() {
        // Declared `@int8` elements: the `Int4` literal must widen, not relabel.
        let cu = get_typed("fn f() { a: @int8[1] = [300] }");
        let f = get_function(&cu, "f");
        if let TypedStatement::Binding(b) = &f.body[0] {
            assert_eq!(
                b.ty,
                Ty::Array {
                    element_type: Box::new(Ty::Builtin(BuiltinType::Int8)),
                    size: 1
                }
            );
        } else {
            panic!("expected Binding");
        }
    }

    #[test]
    fn array_binding_rejects_bad_element_type() {
        let err = get_typed_err(r#"fn f() { a: @int4[1] = ["s"] }"#);
        assert!(
            err.contains("does not match declared element type"),
            "unexpected error: {}",
            err
        );
    }

    // ── 02 reliability: diagnostics carry lines ───────────────────────

    #[test]
    fn analysis_error_carries_statement_line() {
        let src = "fn f() @int4 {\nreturn 0\nx = 1\n}".to_string();
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.clone());
        let ast = parser.parse().expect("parse failed");
        let err = analyze(ast, &HashMap::new()).expect_err("expected analysis error");
        assert_eq!(err.line, Some(3), "error should point at line 3: {}", err);
    }

    #[test]
    fn with_line_keeps_inner_precise_line() {
        use crate::frontend::analyze::AnalysisError;
        let err = AnalysisError {
            msg: "inner".to_string(),
            line: Some(2),
        }
        .with_line(9);
        assert_eq!(err.line, Some(2));
        let err = AnalysisError {
            msg: "outer".to_string(),
            line: None,
        }
        .with_line(9);
        assert_eq!(err.line, Some(9));
    }

    #[test]
    fn bare_return_has_no_values_to_deref() {
        use crate::frontend::typed_ast::TypedReturn;
        let ret = TypedReturn { values: vec![] };
        assert!(ret.first().is_none());
        get_typed("fn f() @void { return }");
    }

    // ── 04 maintainability: BinOp / ShortCircuit mapping ────────────────────

    use crate::frontend::typed_ast::{BinOp, LogicalOp};

    fn return_expr(cu: &TypedProgram, name: &str) -> TypedExpr {
        let f = get_function(cu, name);
        if let TypedStatement::Return(Some(ret)) = &f.body[0] {
            ret.first().expect("test expects a return value").clone()
        } else {
            panic!("expected return statement, got {:#?}", f.body[0]);
        }
    }

    #[test]
    fn test_logical_produces_short_circuit() {
        let cu = get_typed("fn f(a: @bool, b: @bool) @bool { return a && b }");
        let expr = return_expr(&cu, "f");
        assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        assert!(matches!(
            expr.expression,
            TypedExprKind::ShortCircuit {
                operator: LogicalOp::And,
                ..
            }
        ));

        let cu = get_typed("fn f(a: @bool, b: @bool) @bool { return a || b }");
        let expr = return_expr(&cu, "f");
        assert!(matches!(
            expr.expression,
            TypedExprKind::ShortCircuit {
                operator: LogicalOp::Or,
                ..
            }
        ));
    }

    #[test]
    fn test_arithmetic_and_comparison_produce_binop() {
        let cu = get_typed("fn f(a: @int4, b: @int4) @int4 { return a + b * a }");
        let expr = return_expr(&cu, "f");
        if let TypedExprKind::Binary {
            operator,
            left,
            right,
        } = expr.expression
        {
            assert_eq!(operator, BinOp::Add);
            assert!(matches!(left.expression, TypedExprKind::Identifier(_)));
            assert!(matches!(
                right.expression,
                TypedExprKind::Binary {
                    operator: BinOp::Mul,
                    ..
                }
            ));
        } else {
            panic!("expected Binary, got {:#?}", expr.expression);
        }

        let cu = get_typed("fn f(a: @int4, b: @int4) @bool { return a == b }");
        let expr = return_expr(&cu, "f");
        assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        assert!(matches!(
            expr.expression,
            TypedExprKind::Binary {
                operator: BinOp::Eq,
                ..
            }
        ));
    }

    #[test]
    fn test_unary_desugar_uses_binop() {
        // -x desugars to 0 - x
        let cu = get_typed("fn f(x: @int4) @int4 { return -x }");
        let expr = return_expr(&cu, "f");
        assert!(matches!(
            expr.expression,
            TypedExprKind::Binary {
                operator: BinOp::Sub,
                ..
            }
        ));

        // ~x desugars to x ^ -1
        let cu = get_typed("fn f(x: @int4) @int4 { return ~x }");
        let expr = return_expr(&cu, "f");
        assert!(matches!(
            expr.expression,
            TypedExprKind::Binary {
                operator: BinOp::Xor,
                ..
            }
        ));

        // !x desugars through == and still type-checks to bool
        let cu = get_typed("fn f(x: @bool) @bool { return !x }");
        let expr = return_expr(&cu, "f");
        assert_eq!(expr.inferred_type, Ty::Builtin(BuiltinType::Boolean));
        assert!(matches!(
            expr.expression,
            TypedExprKind::Binary {
                operator: BinOp::Eq,
                ..
            }
        ));
    }
}
