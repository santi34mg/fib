#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::frontend::ast::binary_expression::BinaryExpression;
    use crate::frontend::ast::function_declaration::FunctionDeclaration;
    use crate::frontend::ast::pattern::Pattern;
    use crate::frontend::ast::qualified_name::QualifiedIdentifier;
    use crate::frontend::ast::statement::StatementKind;
    use crate::frontend::ast::unary_expression::UnaryExpression;
    use crate::frontend::ast::variable_declaration::VariableDeclaration;
    use crate::frontend::ast::{
        Ast, expression::Expression, expression::ExpressionKind, type_expression::TypeExpression,
        type_expression::TypeExpressionKind,
    };
    use crate::frontend::parser::Parser;
    use crate::frontend::tokenizer::Tokenizer;
    use crate::frontend::tokens::builtin::BuiltinType;
    use crate::frontend::tokens::{Literal, Operator, Token};

    #[test]
    fn samples_parse_with_current_frontend() {
        let samples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
        let mut paths: Vec<_> = std::fs::read_dir(&samples_dir)
            .expect("samples directory must be readable")
            .map(|entry| entry.expect("sample entry must be readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "fib"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "sample discovery must not be empty");

        let mut failures = Vec::new();
        for path in paths {
            let source = std::fs::read_to_string(&path).expect("sample must be readable UTF-8");
            let mut parser = Parser::with_source(Tokenizer::new(&source), &path, &source);
            match parser.parse() {
                Ok(ast) => assert!(
                    !ast.statements.is_empty(),
                    "{} must contain parsed statements",
                    path.display()
                ),
                Err(error) => failures.push(format!("{}:\n{error}", path.display())),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    fn get_ast(test_string: &str) -> Ast {
        let trimmed = test_string.trim_start();
        let src = if trimmed.starts_with("fn ")
            || trimmed.starts_with("extern ")
            || trimmed.starts_with("type ")
        {
            test_string.to_string()
        } else {
            format!("fn __test() {{ {} }}", test_string)
        };
        let lexer = Tokenizer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::with_source(tokens.into_iter(), Path::new("test_instance"), &src);
        match parser.parse() {
            Ok(ast) => ast,
            Err(e) => panic!("Could not parse {}, error:\n{}", src, e),
        }
    }

    fn module_statements(ast: &Ast) -> Vec<&StatementKind> {
        if let [statement] = ast.statements.as_slice()
            && let StatementKind::FunctionDeclaration(function) = &statement.kind
            && let Some(body) = &function.body
        {
            return body
                .statements
                .iter()
                .map(|statement| &statement.kind)
                .collect();
        }
        ast.statements
            .iter()
            .map(|statement| &statement.kind)
            .collect()
    }

    fn find_func(ast: &Ast) -> Option<&FunctionDeclaration> {
        ast.statements.iter().find_map(|s| {
            if let StatementKind::FunctionDeclaration(f) = &s.kind {
                Some(f)
            } else {
                None
            }
        })
    }

    #[test]
    fn test_expression_literal() {
        let test_string = "1;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Literal(Literal::Integer(1)),
                ..
            })
        ));
    }

    #[test]
    fn test_expression_addition() {
        let test_string = "1 + 2;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(
                l_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            assert!(matches!(
                r_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(op, Operator::Plus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_substraction() {
        let test_string = "1 - 2;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(
                l_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            assert!(matches!(
                r_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(op, Operator::Minus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_multiplication() {
        let test_string = "1 * 2;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(
                l_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            assert!(matches!(
                r_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(op, Operator::Star));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_division() {
        let test_string = "1 / 2;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(
                l_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            assert!(matches!(
                r_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(op, Operator::Slash));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_order_operations() {
        let test_string = "2 * 3 + 2;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = &stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(
                l_expr,
                Expression {
                    kind: ExpressionKind::BinaryExpresison { .. },
                    ..
                }
            ));
            assert!(matches!(
                r_expr,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(op, Operator::Plus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_typed_declaration() {
        let test_string = "count : int32 = 0;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::VariableDeclaration(VariableDeclaration {
            identifier,
            ty:
                Some(TypeExpression {
                    kind: TypeExpressionKind::Builtin(_),
                    ..
                }),
            expression:
                Some(Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(0)),
                    ..
                }),
            ..
        }) = stmts[0]
        {
            assert_eq!(identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_builtin_types_do_not_use_at_prefix() {
        for source in [
            "count: @int32 = 0;",
            "fn f(x: @int32) void {}",
            "fn f() @void {}",
            "x as @int32;",
        ] {
            let error = get_ast_err(source);
            assert!(error.contains("types do not use '@'"), "{source}: {error}");
        }
    }

    #[test]
    fn test_var_is_not_a_declaration_keyword() {
        assert!(get_ast_err("var int32 x = 0;").contains("expected ';'"));
    }

    #[test]
    fn test_typed_declaration_without_initializer() {
        let ast = get_ast("count: int32;");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::VariableDeclaration(VariableDeclaration {
                ty: Some(TypeExpression {
                    kind: TypeExpressionKind::Builtin(_),
                    ..
                }),
                expression: None,
                is_const: false,
                ..
            })
        ));
    }

    #[test]
    fn test_const_inferred_declaration() {
        let ast = get_ast("const count := 0;");
        let stmts = module_statements(&ast);
        assert!(
            matches!(stmts[0], StatementKind::VariableDeclaration(VariableDeclaration {
            identifier: name,
            ty: None,
            expression: Some(_),
            is_const: true,
            ..
        }) if name == "count")
        );
        let ast = get_ast("const count: int32 = 0;");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::VariableDeclaration(VariableDeclaration {
                ty: Some(TypeExpression {
                    kind: TypeExpressionKind::Builtin(_),
                    ..
                }),
                expression: Some(_),
                is_const: true,
                ..
            })
        ));
        let ast = get_ast("const count: int32;");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::VariableDeclaration(VariableDeclaration {
                ty: Some(_),
                expression: None,
                is_const: true,
                ..
            })
        ));
    }

    #[test]
    fn test_colon_var_declaration_with_type() {
        let test_string = "fn f() { count: int32 = 0; }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::VariableDeclaration(VariableDeclaration {
            identifier,
            ty:
                Some(TypeExpression {
                    kind: TypeExpressionKind::Builtin(_),
                    ..
                }),
            expression:
                Some(Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(0)),
                    ..
                }),
            ..
        }) = stmts[0]
        {
            assert_eq!(identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_colon_var_declaration_inferred_type() {
        let test_string = "fn f() { count := 0; }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::VariableDeclaration(VariableDeclaration {
            identifier,
            ty: None,
            expression:
                Some(Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(0)),
                    ..
                }),
            ..
        }) = stmts[0]
        {
            assert_eq!(identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_multi_var_declaration_inferred_type() {
        for (source, constness) in [
            ("q, r := divmod(17, 5);", [false, false]),
            ("const q, r := divmod(17, 5);", [true, false]),
            ("q, const r := divmod(17, 5);", [false, true]),
            ("const q, const r := divmod(17, 5);", [true, true]),
        ] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            let StatementKind::MultiVariableDeclaration { bindings, values } = stmts[0] else {
                panic!("unexpected AST for {source}: {:#?}", stmts[0]);
            };
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].identifier, "q");
            assert_eq!(bindings[1].identifier, "r");
            assert_eq!(
                bindings.iter().map(|b| b.is_const).collect::<Vec<_>>(),
                constness
            );
            assert!(
                bindings
                    .iter()
                    .all(|b| b.ty.is_none() && b.expression.is_none())
            );
            assert_eq!(values.len(), 1);
        }
        assert!(get_ast_err("const q, := 1;").contains("expected variable name"));
        let error = get_ast_err("const q, r: int32;");
        assert!(error.contains("each binding needs its own type"));
        assert!(error.contains("use ':=' with an initializer"));
    }

    #[test]
    fn test_typed_multi_variable_declarations() {
        for (source, constness) in [
            ("a: int8, b: string = 1, \"text\";", [false, false]),
            ("const a: int8, b: string = 1, \"text\";", [true, false]),
            ("a: int8, const b: string = 1, \"text\";", [false, true]),
            (
                "const a: int8, const b: string = 1, \"text\";",
                [true, true],
            ),
        ] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            let StatementKind::MultiVariableDeclaration { bindings, values } = stmts[0] else {
                panic!("expected typed multi-variable declaration: {source}");
            };
            assert_eq!(bindings.len(), 2);
            assert_eq!(
                bindings
                    .iter()
                    .map(|b| b.identifier.as_str())
                    .collect::<Vec<_>>(),
                ["a", "b"]
            );
            assert_eq!(
                bindings.iter().map(|b| b.is_const).collect::<Vec<_>>(),
                constness
            );
            assert!(bindings.iter().all(|b| matches!(
                b.ty,
                Some(TypeExpression {
                    kind: TypeExpressionKind::Builtin(_),
                    ..
                })
            ) && b.expression.is_none()));
            assert_eq!(values.len(), 2);
        }
        for (source, constness) in [
            ("a: int8, b: string;", [false, false]),
            ("const a: int, b: string;", [true, false]),
            ("const a: int8, b: string;", [true, false]),
            ("a: int8, const b: string;", [false, true]),
            ("const a: int8, const b: string;", [true, true]),
        ] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            let StatementKind::MultiVariableDeclaration { bindings, values } = stmts[0] else {
                panic!("expected typed multi-variable declaration: {source}");
            };
            assert!(values.is_empty());
            assert_eq!(
                bindings.iter().map(|b| b.is_const).collect::<Vec<_>>(),
                constness
            );
            assert!(
                bindings
                    .iter()
                    .all(|b| b.ty.is_some() && b.expression.is_none())
            );
        }
        assert!(get_ast_err("const a: int8, b = 1, 2;").contains("expected ':'"));
    }

    #[test]
    fn test_mixed_multi_variable_bindings() {
        let ast = get_ast("const a: int8, b, const c: string = 1, 2, \"text\";");
        let stmts = module_statements(&ast);
        let StatementKind::MultiVariableDeclaration { bindings, values } = stmts[0] else {
            panic!("expected multi-variable declaration: {:#?}", stmts[0]);
        };
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.identifier.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(
            bindings.iter().map(|b| b.is_const).collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(
            bindings.iter().map(|b| b.ty.is_some()).collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(values.len(), 3);

        let ast = get_ast("a: int8, const b := 1, 2;");
        let stmts = module_statements(&ast);
        let StatementKind::MultiVariableDeclaration { bindings, values } = stmts[0] else {
            panic!("expected multi-variable declaration");
        };
        assert_eq!(
            bindings.iter().map(|b| b.is_const).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(
            bindings.iter().map(|b| b.ty.is_some()).collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_return_statement() {
        let test_string = "fn foo() int32 { return 42; }";
        let ast = get_ast(test_string);
        let func = find_func(&ast);
        let func = func.expect("expected function declaration");
        let body = func.body.as_ref().expect("expected function body");
        assert!(matches!(
            &body.statements[0].kind,
            StatementKind::Return(Some(exprs))
                if exprs.len() == 1
                    && matches!(exprs[0], Expression { kind: ExpressionKind::Literal(Literal::Integer(42)), .. })
        ));
    }

    #[test]
    fn test_return_void() {
        let test_string = "fn foo() { return; }";
        let ast = get_ast(test_string);
        let func = find_func(&ast);
        let func = func.expect("expected function declaration");
        let body = func.body.as_ref().expect("expected function body");
        assert!(matches!(
            body.statements[0].kind,
            StatementKind::Return(None)
        ));
    }

    #[test]
    fn test_if_statement() {
        let test_string = "if true { x := 1; };";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } = stmts[0]
        {
            assert!(matches!(
                condition,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Boolean(true)),
                    ..
                }
            ));
            assert_eq!(then_branch.len(), 1);
            assert!(else_branch.is_none());
        } else {
            panic!("expected If statement, got {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_if_struct_construct_condition() {
        for (source, expected_name) in [
            (
                "if MyStruct { my_field: 10 } { x := 1; };",
                vec!["MyStruct"],
            ),
            (
                "if pkg::MyStruct { my_field: 10 } { x := 1; };",
                vec!["pkg", "MyStruct"],
            ),
        ] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            let StatementKind::If {
                condition:
                    Expression {
                        kind:
                            ExpressionKind::StructConstruct {
                                struct_identifier: struct_name,
                                fields,
                            },
                        ..
                    },
                then_branch,
                else_branch: None,
            } = stmts[0]
            else {
                panic!(
                    "expected a struct construction in the if condition: {:#?}",
                    stmts[0]
                );
            };
            assert_eq!(
                struct_name
                    .segments
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>(),
                expected_name
            );
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].field_identifier, "my_field");
            assert!(matches!(
                fields[0].expression.kind,
                ExpressionKind::Literal(Literal::Integer(10))
            ));
            assert!(matches!(
                then_branch.as_slice(),
                [crate::frontend::ast::statement::Statement {
                    kind: StatementKind::VariableDeclaration(_),
                    ..
                }]
            ));
        }
    }

    #[test]
    fn test_if_name_condition_is_not_struct_construct() {
        for source in ["if done { x := 1; };", "if done { x: int32 = 1; };"] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            assert!(matches!(
                stmts[0],
                StatementKind::If {
                    condition: Expression {
                        kind: ExpressionKind::QualifiedIdentifier(_),
                        ..
                    },
                    ..
                }
            ));
        }
    }

    #[test]
    fn test_if_else_statement() {
        let test_string = "if false { x := 1; } else { x := 2; };";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementKind::If { else_branch, .. } = stmts[0] {
            assert!(else_branch.is_some());
        } else {
            panic!("expected If statement");
        }
    }

    #[test]
    fn test_while_loop() {
        let test_string = "i := 0; while (i < 10) { i += 1; };";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            stmts[1],
            StatementKind::While {
                condition: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn test_while_without_condition() {
        let ast = get_ast("while () { break; };");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::While {
                condition: None,
                ..
            }
        ));
        assert!(get_ast_err("while (;) { break; };").contains("expected"));
    }

    #[test]
    fn test_break_continue() {
        let test_string = "while (true) { break; continue; };";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementKind::While { body, .. } = stmts[0] {
            assert!(matches!(body[0].kind, StatementKind::Break));
            assert!(matches!(body[1].kind, StatementKind::Continue));
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn test_function_declaration_no_params() {
        let ast = get_ast("fn greet() { }");
        let function = find_func(&ast).expect("expected function declaration");
        assert!(function.signature.parameters.is_empty());
        assert!(function.body.is_some());
    }

    #[test]
    fn test_function_declaration_with_params() {
        let test_string = "fn add(a: int32, b: int32) int32 { return a; }";
        let ast = get_ast(test_string);
        let func = find_func(&ast);
        let func = func.expect("expected function");
        assert_eq!(func.signature.function_identifier, "add");
        assert_eq!(func.signature.parameters.len(), 2);
        assert_eq!(func.signature.parameters[0].parameter_identifier, "a");
        assert_eq!(func.signature.parameters[1].parameter_identifier, "b");
    }

    #[test]
    fn test_extern_function_declaration() {
        let test_string = "extern fn printf(fmt: string) int32;";
        let ast = get_ast(test_string);
        let func = find_func(&ast);
        let func = func.expect("expected function");
        assert!(func.is_extern);
        assert!(func.body.is_none());
    }

    #[test]
    fn test_function_call_expression() {
        let test_string = "foo(1, 2, 3);";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind: ExpressionKind::Call { callee, args },
            ..
        }) = stmts[0]
        {
            assert!(matches!(
                **callee,
                Expression {
                    kind: ExpressionKind::QualifiedIdentifier(_),
                    ..
                }
            ));
            assert_eq!(args.len(), 3);
        } else {
            panic!("expected Call expression, got {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_unary_negation() {
        let test_string = "-5;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::UnaryExpression(UnaryExpression {
                    operator: Operator::Minus,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_unary_logical_not() {
        let test_string = "!true;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::UnaryExpression(UnaryExpression {
                    operator: Operator::LogicalNot,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_comparison_expression() {
        let test_string = "a == b;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::BinaryExpresison(BinaryExpression {
                    operator: Operator::DoubleEquals,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_logical_and_expression() {
        let test_string = "a && b;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::BinaryExpresison(BinaryExpression {
                    operator: Operator::LogicalAnd,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_grouping_expression() {
        let test_string = "(1 + 2) * 3;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        // The outer operation must be multiplication with LHS being a binary expression
        if let StatementKind::ExpressionStatement(Expression {
            kind: ExpressionKind::BinaryExpresison(BinaryExpression { left, operator, .. }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::Star));
            assert!(matches!(
                **left,
                Expression {
                    kind: ExpressionKind::BinaryExpresison(_),
                    ..
                }
            ));
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_field_access() {
        let test_string = "obj.field;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::ExprAcess { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_cast_expression() {
        let test_string = "x as int64;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Cast { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_address_of() {
        let test_string = "x&;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::UnaryExpression(UnaryExpression {
                    operator: Operator::AddressOf,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_dereference() {
        let test_string = "x*;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::UnaryExpression(UnaryExpression {
                    operator: Operator::Dereference,
                    ..
                }),
                ..
            })
        ));
    }

    #[test]
    fn test_index_access() {
        let test_string = "arr[0];";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::IndexAccess { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_slice_range() {
        let test_string = "arr[0..2];";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::SliceLiteral { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_slice_range_forms() {
        // (source, has_start, has_end, inclusive)
        let cases = [
            ("arr[0..2];", true, true, false),
            ("arr[0.=2];", true, true, true),
            ("arr[0..];", true, false, false),
            ("arr[..2];", false, true, false),
            ("arr[.=2];", false, true, true),
            ("arr[..];", false, false, false),
        ];
        for (src, want_start, want_end, want_incl) in cases {
            let ast = get_ast(src);
            let stmts = module_statements(&ast);
            match &stmts[0] {
                StatementKind::ExpressionStatement(Expression {
                    kind:
                        ExpressionKind::SliceLiteral {
                            start,
                            end,
                            inclusive,
                            ..
                        },
                    ..
                }) => {
                    assert_eq!(start.is_some(), want_start, "start for {}", src);
                    assert_eq!(end.is_some(), want_end, "end for {}", src);
                    assert_eq!(*inclusive, want_incl, "inclusive for {}", src);
                }
                other => panic!("expected Slice for {}, found {:?}", src, other),
            }
        }
    }

    #[test]
    fn test_slice_inclusive_needs_end() {
        for src in [
            "arr[0.=];",
            "arr[.=];",
            "arr[];",
            "arr[0..=2];",
            "arr[..=2];",
            "arr[0..=];",
            "arr[..=];",
        ] {
            let err = Tokenizer::new(src).collect::<Vec<_>>();
            let mut parser =
                Parser::with_source(err.into_iter(), std::path::Path::new("test"), src);
            assert!(parser.parse().is_err(), "expected parse error for {}", src);
        }
    }

    #[test]
    fn test_postfix_access_keeps_binary_operators() {
        for (source, operator) in [("x * y;", Operator::Star), ("x & y;", Operator::Ampersand)] {
            let ast = get_ast(source);
            let stmts = module_statements(&ast);
            assert!(
                matches!(stmts[0], StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::BinaryExpresison(BinaryExpression { operator: actual, .. }), ..
            }) if actual == &operator),
                "{source}: {:#?}",
                stmts[0]
            );
        }
    }

    #[test]
    fn test_old_postfix_access_forms_are_not_accepted() {
        for source in ["x.*;", "x.&;", "arr.[0];", "arr.[0..2];"] {
            assert!(!get_ast_err(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn test_len_property_after_dot() {
        let test_string = "arr.len;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        match &stmts[0] {
            StatementKind::ExpressionStatement(Expression {
                kind:
                    ExpressionKind::ExprAcess {
                        accessed_identifier: field,
                        ..
                    },
                ..
            }) => assert_eq!(field, "len"),
            other => panic!("expected FieldAccess, found {:?}", other),
        }
    }

    #[test]
    fn test_bare_len_property_errors() {
        assert!(get_ast_err("x := @len;").contains("use '.len' instead of '@len'"));
        assert!(get_ast_err("arr.@len;").contains("use '.len' instead of '@len'"));
    }

    #[test]
    fn test_slice_type_expression() {
        let ast = get_ast("fn f(x: int32[]) void {}");
        let stmt = ast.statements.first().expect("one statement");
        match stmt.kind {
            StatementKind::FunctionDeclaration(ref f) => {
                let param_ty = &f.signature.parameters[0].parameter_type.kind;
                assert!(
                    matches!(param_ty, TypeExpressionKind::Slice { .. }),
                    "expected Slice type, found {:?}",
                    param_ty
                );
            }
            ref other => panic!("expected function, found {:?}", other),
        }
    }

    #[test]
    fn test_array_type_still_parses_with_size() {
        let ast = get_ast("fn f(x: int32[3]) void {}");
        let stmt = ast.statements.first().expect("one statement");
        match stmt.kind {
            StatementKind::FunctionDeclaration(ref f) => {
                let param_ty = &f.signature.parameters[0].parameter_type.kind;
                assert!(
                    matches!(param_ty, TypeExpressionKind::Array { size: 3, .. }),
                    "expected Array[3], found {:?}",
                    param_ty
                );
            }
            ref other => panic!("expected function, found {:?}", other),
        }
    }

    #[test]
    fn test_string_literal_expression() {
        let test_string = r#""hello";"#;
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Literal(Literal::String(_)),
                ..
            })
        ));
    }

    #[test]
    fn test_bool_literal_true_false() {
        let ast_t = get_ast("true;");
        let ast_f = get_ast("false;");
        let stmts_t = module_statements(&ast_t);
        let stmts_f = module_statements(&ast_f);
        assert!(matches!(
            stmts_t[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Literal(Literal::Boolean(true)),
                ..
            })
        ));
        assert!(matches!(
            stmts_f[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Literal(Literal::Boolean(false)),
                ..
            })
        ));
    }

    #[test]
    fn test_null_literal_expression() {
        let ast = get_ast("null;");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::Literal(Literal::Null),
                ..
            })
        ));
    }

    #[test]
    fn test_defer_statement() {
        let test_string = "defer foo();";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(stmts[0], StatementKind::Defer(_)));
    }

    #[test]
    fn test_multiple_statements() {
        let test_string = "x := 1;\ny := 2;\nz := 3;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn test_precedence_mul_before_add() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let ast = get_ast("1 + 2 * 3;");
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::Plus));
            assert!(matches!(
                **left,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            assert!(matches!(
                **right,
                Expression {
                    kind: ExpressionKind::BinaryExpresison { .. },
                    ..
                }
            ));
        } else {
            panic!("unexpected AST");
        }
    }

    #[test]
    fn test_assignment_statement() {
        let test_string = "x = 42;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(stmts[0], StatementKind::Assignment { .. }));
    }

    #[test]
    fn test_array_literal() {
        let test_string = "[1, 2, 3];";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind: ExpressionKind::ArrayLiteral { elements },
            ..
        }) = stmts[0]
        {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected ArrayLiteral, got {:#?}", stmts[0]);
        }
    }

    // ── 02 reliability: error paths (no panics, messages preserved) ──

    fn get_ast_err(test_string: &str) -> String {
        let trimmed = test_string.trim_start();
        let src = if trimmed.starts_with("fn ")
            || trimmed.starts_with("extern ")
            || trimmed.starts_with("type ")
        {
            test_string.to_string()
        } else {
            format!("fn __test() {{ {} }}", test_string)
        };
        let lexer = Tokenizer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::with_source(tokens.into_iter(), Path::new("test_instance"), &src);
        match parser.parse() {
            Ok(_) => panic!("expected parse error for {:?}", src),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn test_unterminated_string_surfaces_lexer_message() {
        let err = get_ast_err(r#"x := "abc"#);
        assert!(err.contains("unterminated"), "lexer message lost: {}", err);
    }

    #[test]
    fn test_unknown_builtin_surfaces_lexer_message() {
        let err = get_ast_err("x := @nope");
        assert!(
            err.contains("unknown builtin"),
            "lexer message lost: {}",
            err
        );
    }

    #[test]
    fn test_unknown_character_surfaces() {
        let err = get_ast_err("x := 5 $ 3");
        assert!(
            err.contains("unknown character"),
            "lexer message lost: {}",
            err
        );
    }

    #[test]
    fn test_truncated_qualified_alias_errors_without_panic() {
        let err = get_ast_err("const File := std::");
        assert!(err.contains("after '::'"), "unexpected error: {}", err);
    }

    #[test]
    fn test_truncated_switch_pattern_errors_without_panic() {
        let err = get_ast_err("type Color enum { Red }\nfn f(c: Color) void { switch (c) { when");
        assert!(
            err.contains("variant name") || err.contains("else"),
            "unexpected error: {}",
            err
        );
    }

    // ── 04 maintainability: generic binary parser keeps precedence/assoc ──

    #[test]
    fn test_binary_levels_are_left_associative() {
        // 8 - 4 - 2 parses as (8 - 4) - 2
        let ast = get_ast("8 - 4 - 2;");
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::Minus));
            assert!(matches!(
                **right,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(2)),
                    ..
                }
            ));
            assert!(matches!(
                **left,
                Expression {
                    kind: ExpressionKind::BinaryExpresison { .. },
                    ..
                }
            ));
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_binary_shift_binds_looser_than_additive() {
        // 1 << 2 + 3 parses as 1 << (2 + 3)
        let ast = get_ast("1 << 2 + 3;");
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::LeftShift));
            assert!(matches!(
                **left,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(1)),
                    ..
                }
            ));
            if let Expression {
                kind:
                    ExpressionKind::BinaryExpresison(BinaryExpression {
                        operator: inner_op, ..
                    }),
                ..
            } = &**right
            {
                assert!(matches!(inner_op, Operator::Plus));
            } else {
                panic!("unexpected RHS: {:#?}", right);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_binary_logical_or_loosest() {
        // a && b || c parses as (a && b) || c
        let ast = get_ast("a && b || c;");
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::LogicalOr));
            assert!(matches!(
                **right,
                Expression {
                    kind: ExpressionKind::QualifiedIdentifier(_),
                    ..
                }
            ));
            if let Expression {
                kind:
                    ExpressionKind::BinaryExpresison(BinaryExpression {
                        operator: inner_op, ..
                    }),
                ..
            } = &**left
            {
                assert!(matches!(inner_op, Operator::LogicalAnd));
            } else {
                panic!("unexpected LHS: {:#?}", left);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_binary_equality_spans_comparison() {
        // 1 + 2 == 3 parses as (1 + 2) == 3
        let ast = get_ast("1 + 2 == 3;");
        let stmts = module_statements(&ast);
        if let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::BinaryExpresison(BinaryExpression {
                    left,
                    operator,
                    right,
                }),
            ..
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::DoubleEquals));
            assert!(matches!(
                **right,
                Expression {
                    kind: ExpressionKind::Literal(Literal::Integer(3)),
                    ..
                }
            ));
            assert!(matches!(
                **left,
                Expression {
                    kind: ExpressionKind::BinaryExpresison { .. },
                    ..
                }
            ));
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    // ── 05 testing: shape coverage for previously untested constructs ──

    #[test]
    fn test_qualified_access_shape() {
        let ast = get_ast("x := a::b;");
        let stmts = module_statements(&ast);
        if let StatementKind::VariableDeclaration(decl) = stmts[0] {
            let init = decl.expression.as_ref().expect("initializer");
            if let Expression {
                kind: ExpressionKind::QualifiedIdentifier(QualifiedIdentifier { segments, .. }),
                ..
            } = init
            {
                assert_eq!(segments[0], "a");
                assert_eq!(segments[1], "b");
            } else {
                panic!("unexpected init: {:#?}", init);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_module_alias_is_a_top_level_const_statement() {
        let source = "const File := std::io::File;\nfn use_file() {}";
        let mut parser = Parser::with_source(Tokenizer::new(source), Path::new("test"), source);
        let ast = parser.parse().expect("top-level statements should parse");
        assert_eq!(ast.statements.len(), 2);
        let StatementKind::VariableDeclaration(alias) = &ast.statements[0].kind else {
            panic!("expected a variable declaration");
        };
        assert_eq!(alias.identifier, "File");
        assert!(alias.is_const);
        assert!(alias.ty.is_none());
        let Some(Expression {
            kind: ExpressionKind::QualifiedIdentifier(QualifiedIdentifier { segments, .. }),
            ..
        }) = &alias.expression
        else {
            panic!("expected a qualified access initializer");
        };
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.as_str())
                .collect::<Vec<_>>(),
            ["std", "io", "File"]
        );
        assert!(matches!(
            ast.statements[1].kind,
            StatementKind::FunctionDeclaration(_)
        ));
        assert!(get_ast_err("import std::io::File;").contains("expected ';'"));
    }

    #[test]
    fn test_builtin_call_shape() {
        let ast = get_ast("x := @str_len(\"hi\");");
        let stmts = module_statements(&ast);
        if let StatementKind::VariableDeclaration(decl) = stmts[0] {
            let init = decl.expression.as_ref().expect("initializer");
            if let Expression {
                kind: ExpressionKind::CallToBuiltin { builtin, args },
                ..
            } = init
            {
                assert!(matches!(
                    builtin,
                    crate::frontend::tokens::builtin::BuiltinFunction::StrLen
                ));
                assert_eq!(args.len(), 1);
            } else {
                panic!("unexpected init: {:#?}", init);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_builtin_call_missing_paren_errors() {
        let err = get_ast_err("x := @str_len;");
        assert!(err.contains("expected '('"), "unexpected error: {}", err);
    }

    #[test]
    fn test_struct_construction_is_an_expression_statement() {
        let ast = get_ast("MyStruct { my_field: 10 };");
        let stmts = module_statements(&ast);
        let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::StructConstruct {
                    struct_identifier: struct_name,
                    fields,
                },
            ..
        }) = stmts[0]
        else {
            panic!("expected a struct construction expression: {:#?}", stmts[0]);
        };
        assert_eq!(struct_name.segments[0], "MyStruct");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_identifier, "my_field");
        assert!(matches!(
            fields[0].expression.kind,
            ExpressionKind::Literal(Literal::Integer(10))
        ));
    }

    #[test]
    fn test_struct_construct_shape() {
        let ast = get_ast("x := Point { x: 1, y: 2 };");
        let stmts = module_statements(&ast);
        if let StatementKind::VariableDeclaration(decl) = stmts[0] {
            let init = decl.expression.as_ref().expect("initializer");
            if let Expression {
                kind:
                    ExpressionKind::StructConstruct {
                        struct_identifier: struct_name,
                        fields,
                    },
                ..
            } = init
            {
                assert_eq!(struct_name.segments.len(), 1);
                assert_eq!(struct_name.segments[0], "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field_identifier, "x");
                assert_eq!(fields[1].field_identifier, "y");
            } else {
                panic!("unexpected init: {:#?}", init);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_enum_variant_construct_shape() {
        let ast = get_ast("x := Color.Red(1);");
        let stmts = module_statements(&ast);
        if let StatementKind::VariableDeclaration(decl) = stmts[0] {
            let init = decl.expression.as_ref().expect("initializer");
            if let Expression {
                kind: ExpressionKind::Call { callee, args },
                ..
            } = init
            {
                if let ExpressionKind::ExprAcess {
                    expr,
                    accessed_identifier,
                    ..
                } = &callee.kind
                {
                    let ExpressionKind::QualifiedIdentifier(name) = &expr.kind else {
                        panic!("expected {:#?} to be a QualifiedIdentifier", expr.kind);
                    };
                    assert_eq!(name.segments, ["Color"]);
                    assert_eq!(accessed_identifier, "Red");
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0].kind, ExpressionKind::Literal(Literal::Integer(1)));
                } else {
                    panic!("unexpected callee: {:#?}", callee);
                }
                // FIXME: check the payload, also test enum.variant[void]
            } else {
                panic!("unexpected init: {:#?}", init);
            }
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_switch_full_shape() {
        let ast = get_ast("fn f(c: int32) void { switch (c) { when .R { } when else { } }; }");
        let stmts = module_statements(&ast);
        if let StatementKind::Switch { subject, arms } = stmts[0] {
            assert!(matches!(
                subject,
                Expression {
                    kind: ExpressionKind::QualifiedIdentifier(_),
                    ..
                }
            ));
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                arms[0].pattern,
                crate::frontend::ast::pattern::Pattern::EnumVariant { .. }
            ));
            assert!(matches!(
                arms[1].pattern,
                crate::frontend::ast::pattern::Pattern::Wildcard
            ));
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn builtin_type_spellings_have_expected_ast_variants() {
        for (spelling, expected) in [
            ("uint8", BuiltinType::UInt8),
            ("uint16", BuiltinType::UInt16),
            ("uint32", BuiltinType::UInt32),
            ("uint64", BuiltinType::UInt64),
            ("uint128", BuiltinType::UInt128),
            ("int8", BuiltinType::Int8),
            ("int16", BuiltinType::Int16),
            ("int32", BuiltinType::Int32),
            ("int64", BuiltinType::Int64),
            ("int128", BuiltinType::Int128),
            ("float16", BuiltinType::Float16),
            ("float32", BuiltinType::Float32),
            ("float64", BuiltinType::Float64),
            ("float128", BuiltinType::Float128),
            ("void", BuiltinType::Void),
            ("char", BuiltinType::Char),
            ("bool", BuiltinType::Boolean),
            ("string", BuiltinType::String),
            ("never", BuiltinType::Never),
            ("usize", BuiltinType::Usize),
        ] {
            let source = format!("value: {spelling};");
            let ast = get_ast(&source);
            let stmts = module_statements(&ast);
            let StatementKind::VariableDeclaration(decl) = stmts[0] else {
                panic!("{source}: expected variable declaration: {:?}", stmts[0]);
            };
            assert!(
                matches!(decl.ty.as_ref(), Some(TypeExpression { kind: TypeExpressionKind::Builtin(actual), .. }) if *actual == expected),
                "{source}: unexpected type: {:?}",
                decl.ty
            );
            assert_eq!(decl.ty.as_ref().expect("type").to_string(), spelling);
        }
        for (alias, expected) in [("int", BuiltinType::Int32), ("uint", BuiltinType::UInt32)] {
            let source = format!("value: {alias};");
            let ast = get_ast(&source);
            let stmts = module_statements(&ast);
            let StatementKind::VariableDeclaration(decl) = stmts[0] else {
                panic!("{source}: expected variable declaration");
            };
            assert!(
                matches!(decl.ty.as_ref(), Some(TypeExpression { kind: TypeExpressionKind::Builtin(actual), .. }) if *actual == expected)
            );
            assert_eq!(decl.ty.as_ref().expect("type").to_string(), expected.name());
        }
    }

    #[test]
    fn type_declarations_preserve_struct_and_enum_payload_shapes() {
        let ast = get_ast(
            "type Record struct { id: int32, next: *Record }\ntype Result enum { Ok { value: (int32, string) }, Empty, Error { code: uint8 } }",
        );
        assert_eq!(ast.statements.len(), 2);
        let StatementKind::TypeDeclaration(record) = &ast.statements[0].kind else {
            panic!("expected struct declaration: {:#?}", ast.statements[0]);
        };
        assert_eq!(record.identifier, "Record");
        let TypeExpressionKind::Struct { fields } = &record.expression.kind else {
            panic!("expected struct type: {:#?}", record.expression);
        };
        assert_eq!(
            fields
                .iter()
                .map(|f| f.field_identifier.as_str())
                .collect::<Vec<_>>(),
            ["id", "next"]
        );
        assert!(matches!(
            &fields[0].expression.kind,
            ExpressionKind::TypeValue(TypeExpression {
                kind: TypeExpressionKind::Builtin(BuiltinType::Int32),
                ..
            })
        ));
        assert!(
            matches!(&fields[1].expression.kind, ExpressionKind::TypeValue(TypeExpression { kind: TypeExpressionKind::Pointer { pointed_type }, .. }) if matches!(pointed_type.kind, TypeExpressionKind::QualifiedIdentifier(_)))
        );

        let StatementKind::TypeDeclaration(result) = &ast.statements[1].kind else {
            panic!("expected enum declaration: {:#?}", ast.statements[1]);
        };
        assert_eq!(result.identifier, "Result");
        let TypeExpressionKind::Enum { variants } = &result.expression.kind else {
            panic!("expected enum type: {:#?}", result.expression);
        };
        assert_eq!(
            variants
                .iter()
                .map(|v| v.identifier.as_str())
                .collect::<Vec<_>>(),
            ["Ok", "Empty", "Error"]
        );
        let ok = variants[0].payload.as_ref().expect("Ok payload");
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].field_identifier, "value");
        assert!(
            matches!(&ok[0].expression.kind, ExpressionKind::TypeValue(TypeExpression { kind: TypeExpressionKind::Tuple { elements }, .. }) if matches!(elements.as_slice(), [TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::Int32), .. }, TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::String), .. }]))
        );
        assert!(variants[1].payload.is_none());
        let error = variants[2].payload.as_ref().expect("Error payload");
        assert!(
            matches!(error.as_slice(), [field] if field.field_identifier == "code" && matches!(field.expression.kind, ExpressionKind::TypeValue(TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::UInt8), .. })))
        );
    }

    #[test]
    fn composite_type_expressions_preserve_nesting() {
        let ast = get_ast("type Callback fn(*int32, (bool, string)) -> *uint64");
        let StatementKind::TypeDeclaration(decl) = &ast.statements[0].kind else {
            panic!("expected type declaration: {:#?}", ast.statements[0]);
        };
        assert_eq!(decl.identifier, "Callback");
        let TypeExpressionKind::Function {
            argument_types,
            return_type,
        } = &decl.expression.kind
        else {
            panic!("expected function type: {:#?}", decl.expression);
        };
        assert!(
            matches!(argument_types.as_slice(), [TypeExpression { kind: TypeExpressionKind::Pointer { pointed_type }, .. }, TypeExpression { kind: TypeExpressionKind::Tuple { elements }, .. }] if matches!(pointed_type.kind, TypeExpressionKind::Builtin(BuiltinType::Int32)) && matches!(elements.as_slice(), [TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::Boolean), .. }, TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::String), .. }]))
        );
        assert!(
            matches!(&return_type.kind, TypeExpressionKind::Pointer { pointed_type } if matches!(pointed_type.kind, TypeExpressionKind::Builtin(BuiltinType::UInt64)))
        );
    }

    #[test]
    fn function_signatures_preserve_variadic_return_and_bodyless_forms() {
        for (source, variadic, external, has_body, return_type, parameter_count) in [
            (
                "extern fn log(fmt: string, ...) int32;",
                true,
                true,
                false,
                BuiltinType::Int32,
                1,
            ),
            (
                "fn declared(x: *char) bool;",
                false,
                false,
                false,
                BuiltinType::Boolean,
                1,
            ),
            ("fn implicit() {}", false, false, true, BuiltinType::Void, 0),
        ] {
            let ast = get_ast(source);
            let func = find_func(&ast).expect("function declaration");
            assert_eq!(func.is_variadic, variadic, "{source}");
            assert_eq!(func.is_extern, external, "{source}");
            assert_eq!(func.body.is_some(), has_body, "{source}");
            assert_eq!(func.signature.parameters.len(), parameter_count, "{source}");
            assert!(
                matches!(&func.signature.return_type.kind, TypeExpressionKind::Builtin(actual) if *actual == return_type),
                "{source}: {:?}",
                func.signature.return_type
            );
            if source.contains("declared") {
                assert_eq!(func.signature.function_identifier, "declared");
                assert_eq!(func.signature.parameters[0].parameter_identifier, "x");
                assert!(
                    matches!(&func.signature.parameters[0].parameter_type.kind, TypeExpressionKind::Pointer { pointed_type } if matches!(pointed_type.kind, TypeExpressionKind::Builtin(BuiltinType::Char)))
                );
            }
        }
    }

    #[test]
    fn if_struct_condition_nested_field_else_if_and_continued_expression() {
        let ast = get_ast(
            "if Box { item: Inner { value: 1 } }.item.value == 1 { x := 1; } else if ready { x := 2; } else { x := 3; };",
        );
        let stmts = module_statements(&ast);
        let StatementKind::If {
            condition,
            then_branch,
            else_branch: Some(otherwise),
        } = stmts[0]
        else {
            panic!("expected outer if: {:#?}", stmts[0]);
        };
        let ExpressionKind::BinaryExpresison(BinaryExpression {
            left,
            operator: Operator::DoubleEquals,
            right,
        }) = &condition.kind
        else {
            panic!("expected continued comparison: {condition:#?}");
        };
        assert!(matches!(
            right.kind,
            ExpressionKind::Literal(Literal::Integer(1))
        ));
        let ExpressionKind::ExprAcess {
            expr,
            accessed_identifier,
            ..
        } = &left.kind
        else {
            panic!("expected .value access: {left:#?}");
        };
        assert_eq!(accessed_identifier, "value");
        let ExpressionKind::ExprAcess {
            expr,
            accessed_identifier,
            ..
        } = &expr.kind
        else {
            panic!("expected .item access: {expr:#?}");
        };
        assert_eq!(accessed_identifier, "item");
        let ExpressionKind::StructConstruct {
            struct_identifier: struct_name,
            fields,
        } = &expr.kind
        else {
            panic!("expected struct condition: {expr:#?}");
        };
        assert_eq!(struct_name.segments[0], "Box");
        assert_eq!(fields[0].field_identifier, "item");
        assert!(
            matches!(&fields[0].expression.kind, ExpressionKind::StructConstruct { struct_identifier, fields } if struct_identifier.segments[0] == "Inner" && matches!(fields.as_slice(), [field] if field.field_identifier == "value" && matches!(field.expression.kind, ExpressionKind::Literal(Literal::Integer(1)))))
        );
        assert_eq!(then_branch.len(), 1);
        let [inner] = otherwise.as_slice() else {
            panic!("expected one else-if: {otherwise:#?}");
        };
        let StatementKind::If {
            condition:
                Expression {
                    kind: ExpressionKind::QualifiedIdentifier(name),
                    ..
                },
            then_branch,
            else_branch: Some(final_branch),
        } = &inner.kind
        else {
            panic!("expected else-if: {inner:#?}");
        };
        assert_eq!(name.segments, ["ready"]);
        assert_eq!(then_branch.len(), 1);
        assert_eq!(final_branch.len(), 1);
    }

    #[test]
    fn compound_assignment_desugars_to_assignment_with_matching_operator() {
        for (symbol, operator) in [
            ("+=", Operator::Plus),
            ("-=", Operator::Minus),
            ("*=", Operator::Star),
            ("/=", Operator::Slash),
            ("%=", Operator::Percent),
        ] {
            let source = format!("x {symbol} 2;");
            let ast = get_ast(&source);
            let stmts = module_statements(&ast);
            let StatementKind::Assignment { left, right } = stmts[0] else {
                panic!("{source}: {:#?}", stmts[0]);
            };
            assert!(
                matches!(&left.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["x"]),
                "{source}"
            );
            assert!(
                matches!(&right.kind, ExpressionKind::BinaryExpresison(BinaryExpression { left, operator: actual, right }) if *actual == operator && matches!(&left.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["x"]) && matches!(right.kind, ExpressionKind::Literal(Literal::Integer(2)))),
                "{source}: {right:#?}"
            );
        }
    }

    #[test]
    fn index_assignment_preserves_target_and_value() {
        let ast = get_ast("items[i] = 5;");
        let stmts = module_statements(&ast);
        let StatementKind::Assignment { left, right } = stmts[0] else {
            panic!("expected assignment: {:#?}", stmts[0]);
        };
        let ExpressionKind::IndexAccess { object, index } = &left.kind else {
            panic!("expected indexed target: {left:#?}");
        };
        assert!(
            matches!(&object.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["items"])
        );
        assert!(
            matches!(&index.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["i"])
        );
        assert!(matches!(
            right.kind,
            ExpressionKind::Literal(Literal::Integer(5))
        ));
    }

    #[test]
    fn unsupported_deref_and_multi_assignment_syntax_errors() {
        for source in ["*ptr = 5;", "a, b = 1, 2;"] {
            let error = get_ast_err(source);
            assert!(!error.is_empty(), "{source}: expected a syntax error");
        }
    }

    #[test]
    fn switch_payload_binding_preserves_variant_and_body() {
        let ast = get_ast(
            "switch (result) { when .Ok(payload) { value := payload.value; } when .Empty { } when else { } };",
        );
        let stmts = module_statements(&ast);
        let StatementKind::Switch { subject, arms } = stmts[0] else {
            panic!("expected switch: {:#?}", stmts[0]);
        };
        assert!(
            matches!(&subject.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["result"])
        );
        assert_eq!(arms.len(), 3);
        assert!(
            matches!(&arms[0].pattern, Pattern::EnumVariant { variant, binding: Some(binding), .. } if variant == "Ok" && binding.identifier == "payload")
        );
        assert!(
            matches!(&arms[0].body.as_slice(), [statement] if matches!(&statement.kind, StatementKind::VariableDeclaration(decl) if decl.identifier == "value" && matches!(&decl.expression, Some(Expression { kind: ExpressionKind::ExprAcess { expr, accessed_identifier, .. }, .. }) if matches!(&expr.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["payload"]) && accessed_identifier == "value")))
        );
        assert!(
            matches!(&arms[1].pattern, Pattern::EnumVariant { variant, binding: None, .. } if variant == "Empty")
        );
        assert!(matches!(&arms[2].pattern, Pattern::Wildcard));
    }

    #[test]
    fn malformed_composite_types_signatures_and_patterns_error() {
        for (source, expected) in [
            ("type Broken struct { x: }", "expected a type identifier"),
            (
                "type Broken enum { Ok { value: } }",
                "expected a type identifier",
            ),
            ("type Broken fn(int32) ->", "expected a return type"),
            ("type Broken *", "expected type"),
            ("type Broken ()", "expected at least one type"),
            ("fn f(x: ) {}", "expected parameter type"),
            ("fn f(x: int32, ... y) void {}", "expected ')'"),
            ("switch (x) { when .Ok(1) { } };", "expected binding name"),
        ] {
            let error = get_ast_err(source);
            assert!(error.contains(expected), "{source}: {error}");
        }
    }

    #[test]
    fn parse_error_includes_source_line() {
        let src = "break";
        let mut parser = Parser::with_source(Tokenizer::new(src), Path::new("test_instance"), src);
        let error = parser
            .parse_statement()
            .expect_err("missing semicolon must fail");
        let error = error
            .downcast_ref::<crate::frontend::parser::ParseError>()
            .expect("typed parse error");
        assert_eq!(error.line, 1);
        assert_eq!(error.source_line, "break");
        assert!(error.to_string().contains("break"));
    }

    #[test]
    fn identifier_expressions_share_qualified_shape_and_preserve_spans() {
        for (source, expected) in [
            ("value", vec!["value"]),
            ("pkg::value", vec!["pkg", "value"]),
            ("pkg::nested::value", vec!["pkg", "nested", "value"]),
        ] {
            let source = format!("\n  {source};");
            let mut parser =
                Parser::with_source(Tokenizer::new(&source), Path::new("test"), &source);
            let statement = parser.parse_statement().expect("identifier expression");
            let StatementKind::ExpressionStatement(Expression {
                kind: ExpressionKind::QualifiedIdentifier(identifier),
                span,
                ..
            }) = statement.kind
            else {
                panic!("expected qualified identifier: {source}");
            };
            assert_eq!(identifier.segments, expected);
            assert_eq!(span, crate::diagnostics::Span::new(2, 3));
        }
    }

    #[test]
    fn qualified_callee_and_field_access_preserve_identifiers() {
        let ast = get_ast("pkg::factory().field;");
        let statements = module_statements(&ast);
        let StatementKind::ExpressionStatement(Expression {
            kind:
                ExpressionKind::ExprAcess {
                    expr,
                    accessed_identifier,
                    ..
                },
            ..
        }) = statements[0]
        else {
            panic!("expected field access");
        };
        assert_eq!(accessed_identifier, "field");
        let ExpressionKind::Call { callee, args } = &expr.kind else {
            panic!("expected call");
        };
        assert!(args.is_empty());
        assert!(
            matches!(&callee.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments == ["pkg", "factory"])
        );
    }

    #[test]
    fn metatype_token_works_in_declarations_annotations_and_expressions() {
        let ast = get_ast("type Alias pkg::Value\nfn identity(T: type) type { return type; }");
        assert_eq!(ast.statements.len(), 2);
        let StatementKind::TypeDeclaration(alias) = &ast.statements[0].kind else {
            panic!("expected type declaration");
        };
        assert_eq!(alias.identifier, "Alias");
        assert!(
            matches!(&alias.expression.kind, TypeExpressionKind::QualifiedIdentifier(name) if name.segments == ["pkg", "Value"])
        );
        let function = find_func(&ast).expect("function declaration");
        assert_eq!(function.signature.parameters[0].parameter_identifier, "T");
        assert!(matches!(
            function.signature.parameters[0].parameter_type.kind,
            TypeExpressionKind::Builtin(BuiltinType::MetaType)
        ));
        assert!(matches!(
            function.signature.return_type.kind,
            TypeExpressionKind::Builtin(BuiltinType::MetaType)
        ));
        let body = function.body.as_ref().expect("function body");
        assert!(
            matches!(&body.statements[0].kind, StatementKind::Return(Some(values)) if matches!(values.as_slice(), [Expression { kind: ExpressionKind::TypeValue(TypeExpression { kind: TypeExpressionKind::Builtin(BuiltinType::MetaType), .. }), .. }]))
        );
    }

    #[test]
    fn identifier_positions_reject_builtin_types_and_keywords() {
        for (source, expected) in [
            ("const int32 := 1;", "expected variable name"),
            ("fn type() {}", "expected function name"),
            ("fn f(int32: int32) {}", "expected parameter name"),
            ("type int32 int32", "expected identifier"),
            ("type Record struct { type: int32 }", "expected field name"),
            ("type Choice enum { return }", "expected variant name"),
            ("Point { type: 1 };", "expected field name"),
            ("switch (x) { when .return {} };", "expected variant name"),
            ("switch (x) { when .Ok(type) {} };", "expected binding name"),
            ("pkg::type;", "expected name after '::'"),
            ("value: pkg::type;", "must be followed by symbol"),
        ] {
            let error = get_ast_err(source);
            assert!(error.contains(expected), "{source}: {error}");
        }
    }

    #[test]
    fn compound_assignment_still_requires_a_single_identifier() {
        for target in ["pkg::value", "obj.field", "items[0]", "make()"] {
            let source = format!("{target} += 1;");
            let error = get_ast_err(&source);
            assert!(
                error.contains("compound assignment requires a variable name"),
                "{source}: {error}"
            );
        }
        assert!(get_ast_err("pkg::value := 1;").contains("expected ';'"));
    }

    #[test]
    fn definition_operator_tokens_require_a_single_identifier() {
        use crate::frontend::tokens::{Punctuation, TokenKind};

        for qualified in [false, true] {
            let mut kinds = vec![TokenKind::Identifier("value".to_owned())];
            if qualified {
                kinds.extend([
                    TokenKind::Punctuation(Punctuation::DoubleColon),
                    TokenKind::Identifier("member".to_owned()),
                ]);
            }
            kinds.extend([
                TokenKind::Operator(Operator::Def),
                TokenKind::Literal(Literal::Integer(1)),
                TokenKind::Punctuation(Punctuation::Semicolon),
            ]);
            let tokens = kinds.into_iter().map(|kind| Token::new(kind, 1, 1));
            let mut parser = Parser::with_source(tokens, Path::new("test"), "");
            let result = parser.parse_statement();
            if qualified {
                assert!(
                    result
                        .expect_err("qualified definition must fail")
                        .to_string()
                        .contains("expected a name")
                );
            } else {
                assert!(
                    matches!(result.expect("single-name definition").kind, StatementKind::VariableDeclaration(decl) if decl.identifier == "value" && decl.ty.is_none())
                );
            }
        }
    }

    #[test]
    fn parsed_expression_name_type_and_member_annotations_are_pending() {
        let ast = get_ast("value: pkg::Record = pkg::Record { field: pkg::source.value };");
        let statements = module_statements(&ast);
        let StatementKind::VariableDeclaration(binding) = statements[0] else {
            panic!("expected binding");
        };
        assert_eq!(binding.annotation.binding, None);
        let ty = binding.ty.as_ref().expect("declared type");
        assert_eq!(ty.annotation.resolved, None);
        let TypeExpressionKind::QualifiedIdentifier(name) = &ty.kind else {
            panic!("expected named type");
        };
        assert_eq!(name.segments, ["pkg", "Record"]);
        assert_eq!(name.annotation.resolved, None);

        let initializer = binding.expression.as_ref().expect("initializer");
        assert_eq!(initializer.annotation.resolved, None);
        let ExpressionKind::StructConstruct {
            struct_identifier,
            fields,
        } = &initializer.kind
        else {
            panic!("expected struct construction");
        };
        assert_eq!(struct_identifier.annotation.resolved, None);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_identifier, "field");
        assert_eq!(fields[0].annotation.resolved, None);
        let value = &fields[0].expression;
        assert_eq!(value.annotation.resolved, None);
        let ExpressionKind::ExprAcess {
            expr,
            accessed_identifier,
            member,
        } = &value.kind
        else {
            panic!("expected member access");
        };
        assert_eq!(accessed_identifier, "value");
        assert_eq!(member.resolved, None);
        assert_eq!(expr.annotation.resolved, None);
        let ExpressionKind::QualifiedIdentifier(name) = &expr.kind else {
            panic!("expected receiver name");
        };
        assert_eq!(name.segments, ["pkg", "source"]);
        assert_eq!(name.annotation.resolved, None);
    }

    #[test]
    fn parsed_declaration_and_parameter_annotations_are_pending() {
        let ast = get_ast(
            "type Alias pkg::Value\nfn identity(T: type) type { return type; }\nextern fn foreign(value: Alias);\nconst top := 1;",
        );
        let StatementKind::TypeDeclaration(alias) = &ast.statements[0].kind else {
            panic!("expected type declaration");
        };
        assert_eq!(alias.annotation.binding, None);
        assert_eq!(alias.expression.annotation.resolved, None);
        let TypeExpressionKind::QualifiedIdentifier(name) = &alias.expression.kind else {
            panic!("expected named type");
        };
        assert_eq!(name.annotation.resolved, None);
        for statement in &ast.statements[1..3] {
            let StatementKind::FunctionDeclaration(function) = &statement.kind else {
                panic!("expected function declaration");
            };
            assert_eq!(function.signature.annotation.binding, None);
            assert_eq!(function.signature.return_type.annotation.resolved, None);
            let parameter = &function.signature.parameters[0];
            assert_eq!(parameter.annotation.binding, None);
            assert_eq!(parameter.parameter_type.annotation.resolved, None);
        }
        let function = find_func(&ast).expect("identity function");
        let StatementKind::Return(Some(values)) =
            &function.body.as_ref().unwrap().statements[0].kind
        else {
            panic!("expected return value");
        };
        assert_eq!(values[0].annotation.resolved, None);
        let ExpressionKind::TypeValue(ty) = &values[0].kind else {
            panic!("expected metatype expression");
        };
        assert_eq!(ty.kind, TypeExpressionKind::Builtin(BuiltinType::MetaType));
        assert_eq!(ty.annotation.resolved, None);
        let StatementKind::VariableDeclaration(top) = &ast.statements[3].kind else {
            panic!("expected file-level constant");
        };
        assert!(top.is_const);
        assert_eq!(top.annotation.binding, None);
        assert_eq!(top.expression.as_ref().unwrap().annotation.resolved, None);
    }

    #[test]
    fn parsed_multi_bindings_each_own_a_pending_annotation() {
        for source in ["a: int32, const b := 1, 2;", "a: int32, b: int32;"] {
            let ast = get_ast(source);
            let statements = module_statements(&ast);
            let StatementKind::MultiVariableDeclaration { bindings, values } = statements[0] else {
                panic!("expected multi-binding");
            };
            assert_eq!(bindings.len(), 2);
            for binding in bindings {
                assert_eq!(binding.annotation.binding, None);
                assert!(binding.expression.is_none());
                if let Some(ty) = &binding.ty {
                    assert_eq!(ty.annotation.resolved, None);
                }
            }
            for value in values {
                assert_eq!(value.annotation.resolved, None);
            }
        }
    }

    #[test]
    fn parsed_pattern_payload_and_variant_annotations_are_pending() {
        let ast = get_ast("switch (result) { when .Ok(payload) {} when .Empty {} when else {} };");
        let statements = module_statements(&ast);
        let StatementKind::Switch { arms, .. } = statements[0] else {
            panic!("expected switch");
        };
        let Pattern::EnumVariant {
            member,
            binding: Some(binding),
            ..
        } = &arms[0].pattern
        else {
            panic!("expected payload binding");
        };
        assert_eq!(member.resolved, None);
        assert_eq!(binding.identifier, "payload");
        assert_eq!(binding.annotation.binding, None);
        let Pattern::EnumVariant {
            member,
            binding: None,
            ..
        } = &arms[1].pattern
        else {
            panic!("unit variant must not have a payload binding slot");
        };
        assert_eq!(member.resolved, None);
        assert!(matches!(arms[2].pattern, Pattern::Wildcard));
    }

    #[test]
    fn parsed_type_definition_fields_have_pending_member_not_binding_slots() {
        let ast = get_ast("type Record struct { value: int32 }");
        let StatementKind::TypeDeclaration(declaration) = &ast.statements[0].kind else {
            panic!("expected type declaration");
        };
        let TypeExpressionKind::Struct { fields } = &declaration.expression.kind else {
            panic!("expected struct type");
        };
        assert_eq!(fields[0].annotation.resolved, None);
        assert_eq!(fields[0].expression.annotation.resolved, None);
        let ExpressionKind::TypeValue(ty) = &fields[0].expression.kind else {
            panic!("expected field type");
        };
        assert_eq!(ty.annotation.resolved, None);
    }

    #[test]
    fn qualified_name_missing_segment_is_an_error() {
        let src = "some::";
        let mut parser = Parser::with_source(Tokenizer::new(src), Path::new("test"), src);
        let error = parser
            .parse_qualified_name()
            .expect_err("missing segment must fail");
        assert!(error.to_string().contains("must be followed by symbol"));
    }
}
