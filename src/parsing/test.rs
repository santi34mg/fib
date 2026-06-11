#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::ast::{Ast, DeclarationNode, Expression, StatementNode, TypeExpression};
    use crate::lexing::Lexer;
    use crate::parsing::Parser;
    use crate::tokens::{Literal, Operator, Token};

    fn get_ast(test_string: &str) -> Ast {
        let trimmed = test_string.trim_start();
        let src = if trimmed.starts_with("fn ")
            || trimmed.starts_with("extern ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("type ")
        {
            test_string.to_string()
        } else {
            format!("fn __test() {{ {} }}", test_string)
        };
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test_instance"), src.clone());
        match parser.parse() {
            Ok(ast) => ast,
            Err(e) => panic!("Could not parse {}, error:\n{}", src, e),
        }
    }

    fn module_statements(ast: &Ast) -> Vec<&StatementNode> {
        let mut stmts = Vec::new();
        for decl in &ast.declarations {
            if let DeclarationNode::FunctionDeclaration(f) = decl {
                if let Some(body) = &f.body {
                    for s in &body.statements {
                        stmts.push(&s.kind);
                    }
                }
            }
        }
        stmts
    }

    #[test]
    fn test_expression_literal() {
        let test_string = "1";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Literal(Literal::Integer(1)))
        ));
    }

    #[test]
    fn test_expression_addition() {
        let test_string = "1 + 2";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(l_expr, Expression::Literal(Literal::Integer(1))));
            assert!(matches!(r_expr, Expression::Literal(Literal::Integer(2))));
            assert!(matches!(op, Operator::Plus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_substraction() {
        let test_string = "1 - 2";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(l_expr, Expression::Literal(Literal::Integer(1))));
            assert!(matches!(r_expr, Expression::Literal(Literal::Integer(2))));
            assert!(matches!(op, Operator::Minus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_multiplication() {
        let test_string = "1 * 2";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(l_expr, Expression::Literal(Literal::Integer(1))));
            assert!(matches!(r_expr, Expression::Literal(Literal::Integer(2))));
            assert!(matches!(op, Operator::Star));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_division() {
        let test_string = "1 / 2";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(l_expr, Expression::Literal(Literal::Integer(1))));
            assert!(matches!(r_expr, Expression::Literal(Literal::Integer(2))));
            assert!(matches!(op, Operator::Slash));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_expression_order_operations() {
        let test_string = "2 * 3 + 2";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = &stmts[0]
        {
            let l_expr = *left.clone();
            let op = operator;
            let r_expr = *right.clone();
            assert!(matches!(l_expr, Expression::Binary { .. }));
            assert!(matches!(r_expr, Expression::Literal(Literal::Integer(2))));
            assert!(matches!(op, Operator::Plus));
        } else {
            panic!("AST statement did not match expected Expression");
        }
    }

    #[test]
    fn test_var_declaration_with_type() {
        use crate::ast::VariableDeclaration;
        let test_string = "var int4 count = 0;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            constant_type: Some(TypeExpression::Builtin(_)),
            expression: Some(Expression::Literal(Literal::Integer(0))),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_var_declaration_no_initializer() {
        use crate::ast::VariableDeclaration;
        let test_string = "var int4 x;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            expression: None,
            ..
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "x");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_colon_var_declaration_with_type() {
        use crate::ast::VariableDeclaration;
        let test_string = "fn f() { count: int4 = 0; }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            constant_type: Some(TypeExpression::Builtin(_)),
            expression: Some(Expression::Literal(Literal::Integer(0))),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_colon_var_declaration_inferred_type() {
        use crate::ast::VariableDeclaration;
        let test_string = "fn f() { count := 0; }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            constant_type: None,
            expression: Some(Expression::Literal(Literal::Integer(0))),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "count");
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_multi_var_declaration_inferred_type() {
        let test_string = "fn f() { q, r := divmod(17, 5); }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::MultiVariableDeclaration {
            identifiers,
            values,
        } = stmts[0]
        {
            assert_eq!(identifiers.len(), 2);
            assert_eq!(identifiers[0].identifier, "q");
            assert_eq!(identifiers[1].identifier, "r");
            assert_eq!(values.len(), 1);
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_return_statement() {
        let test_string = "fn foo() int4 { ret 42 }";
        let ast = get_ast(test_string);
        let func = ast.declarations.iter().find_map(|d| {
            if let crate::ast::DeclarationNode::FunctionDeclaration(f) = d {
                Some(f)
            } else {
                None
            }
        });
        let func = func.expect("expected function declaration");
        let body = func.body.as_ref().expect("expected function body");
        assert!(matches!(
            &body.statements[0].kind,
            StatementNode::Return(Some(exprs))
                if exprs.len() == 1
                    && matches!(exprs[0], Expression::Literal(Literal::Integer(42)))
        ));
    }

    #[test]
    fn test_return_void() {
        let test_string = "fn foo() { ret }";
        let ast = get_ast(test_string);
        let func = ast.declarations.iter().find_map(|d| {
            if let crate::ast::DeclarationNode::FunctionDeclaration(f) = d {
                Some(f)
            } else {
                None
            }
        });
        let func = func.expect("expected function declaration");
        let body = func.body.as_ref().expect("expected function body");
        assert!(matches!(
            body.statements[0].kind,
            StatementNode::Return(None)
        ));
    }

    #[test]
    fn test_if_statement() {
        let test_string = "if true { x := 1 }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::If {
            condition,
            then_branch,
            else_branch,
        } = stmts[0]
        {
            assert!(matches!(
                condition,
                Expression::Literal(Literal::Boolean(true))
            ));
            assert_eq!(then_branch.len(), 1);
            assert!(else_branch.is_none());
        } else {
            panic!("expected If statement, got {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_if_else_statement() {
        let test_string = "if false { x := 1 } else { x := 2 }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::If { else_branch, .. } = stmts[0] {
            assert!(else_branch.is_some());
        } else {
            panic!("expected If statement");
        }
    }

    #[test]
    fn test_for_loop() {
        let test_string = "for (i: int4 = 0; i < 10; i += 1) { }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], StatementNode::For { .. }));
    }

    #[test]
    fn test_break_continue() {
        let test_string = "for (;;) { break continue }";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementNode::For { body, .. } = stmts[0] {
            assert!(matches!(body[0].kind, StatementNode::Break));
            assert!(matches!(body[1].kind, StatementNode::Continue));
        } else {
            panic!("expected For statement");
        }
    }

    #[test]
    fn test_function_declaration_no_params() {
        let test_string = "fn greet() { }";
        let ast = get_ast(test_string);
        let func = ast.declarations.iter().find_map(|d| {
            if let crate::ast::DeclarationNode::FunctionDeclaration(f) = d {
                Some(f)
            } else {
                None
            }
        });
        let func = func.expect("expected function");
        assert_eq!(func.signature.name.identifier, "greet");
        assert_eq!(func.signature.parameters.len(), 0);
        assert!(func.signature.return_type.is_none());
    }

    #[test]
    fn test_function_declaration_with_params() {
        let test_string = "fn add(int4 a, int4 b) int4 { ret a }";
        let ast = get_ast(test_string);
        let func = ast.declarations.iter().find_map(|d| {
            if let crate::ast::DeclarationNode::FunctionDeclaration(f) = d {
                Some(f)
            } else {
                None
            }
        });
        let func = func.expect("expected function");
        assert_eq!(func.signature.name.identifier, "add");
        assert_eq!(func.signature.parameters.len(), 2);
        assert_eq!(func.signature.parameters[0].parameter_name.identifier, "a");
        assert_eq!(func.signature.parameters[1].parameter_name.identifier, "b");
        assert!(func.signature.return_type.is_some());
    }

    #[test]
    fn test_extern_function_declaration() {
        let test_string = "extern fn printf(string fmt) int4;";
        let ast = get_ast(test_string);
        let func = ast.declarations.iter().find_map(|d| {
            if let crate::ast::DeclarationNode::FunctionDeclaration(f) = d {
                Some(f)
            } else {
                None
            }
        });
        let func = func.expect("expected function");
        assert!(func.is_extern);
        assert!(func.body.is_none());
    }

    #[test]
    fn test_function_call_expression() {
        let test_string = "foo(1, 2, 3)";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementNode::ExpressionStatement(Expression::Call { callee, args }) = stmts[0] {
            assert!(matches!(**callee, Expression::Identifier(_)));
            assert_eq!(args.len(), 3);
        } else {
            panic!("expected Call expression, got {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_unary_negation() {
        let test_string = "-5";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Unary {
                operator: Operator::Minus,
                ..
            })
        ));
    }

    #[test]
    fn test_unary_logical_not() {
        let test_string = "!true";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Unary {
                operator: Operator::LogicalNot,
                ..
            })
        ));
    }

    #[test]
    fn test_comparison_expression() {
        let test_string = "a == b";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Binary {
                operator: Operator::DoubleEquals,
                ..
            })
        ));
    }

    #[test]
    fn test_logical_and_expression() {
        let test_string = "a && b";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Binary {
                operator: Operator::LogicalAnd,
                ..
            })
        ));
    }

    #[test]
    fn test_grouping_expression() {
        let test_string = "(1 + 2) * 3";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        // The outer operation must be multiplication with LHS being a grouping
        if let StatementNode::ExpressionStatement(Expression::Binary { left, operator, .. }) =
            stmts[0]
        {
            assert!(matches!(operator, Operator::Star));
            assert!(matches!(**left, Expression::Grouping(_)));
        } else {
            panic!("unexpected AST: {:#?}", stmts[0]);
        }
    }

    #[test]
    fn test_field_access() {
        let test_string = "obj.field";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::FieldAccess { .. })
        ));
    }

    #[test]
    fn test_cast_expression() {
        let test_string = "x as int8";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Cast { .. })
        ));
    }

    #[test]
    fn test_address_of() {
        let test_string = "x.&";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::AddressOf(_))
        ));
    }

    #[test]
    fn test_dereference() {
        let test_string = "x.*";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Dereference(_))
        ));
    }

    #[test]
    fn test_index_access() {
        let test_string = "arr.[0]";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::IndexAccess { .. })
        ));
    }

    #[test]
    fn test_string_literal_expression() {
        let test_string = r#""hello""#;
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Literal(Literal::String(_)))
        ));
    }

    #[test]
    fn test_bool_literal_true_false() {
        let ast_t = get_ast("true");
        let ast_f = get_ast("false");
        let stmts_t = module_statements(&ast_t);
        let stmts_f = module_statements(&ast_f);
        assert!(matches!(
            stmts_t[0],
            StatementNode::ExpressionStatement(Expression::Literal(Literal::Boolean(true)))
        ));
        assert!(matches!(
            stmts_f[0],
            StatementNode::ExpressionStatement(Expression::Literal(Literal::Boolean(false)))
        ));
    }

    #[test]
    fn test_null_literal_expression() {
        let ast = get_ast("null");
        let stmts = module_statements(&ast);
        assert!(matches!(
            stmts[0],
            StatementNode::ExpressionStatement(Expression::Literal(Literal::Null))
        ));
    }

    #[test]
    fn test_defer_statement() {
        let test_string = "defer foo()";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(stmts[0], StatementNode::Defer(_)));
    }

    #[test]
    fn test_multiple_statements() {
        let test_string = "x := 1\ny := 2\nz := 3";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn test_precedence_mul_before_add() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let ast = get_ast("1 + 2 * 3");
        let stmts = module_statements(&ast);
        if let StatementNode::ExpressionStatement(Expression::Binary {
            left,
            operator,
            right,
        }) = stmts[0]
        {
            assert!(matches!(operator, Operator::Plus));
            assert!(matches!(**left, Expression::Literal(Literal::Integer(1))));
            assert!(matches!(**right, Expression::Binary { .. }));
        } else {
            panic!("unexpected AST");
        }
    }

    #[test]
    fn test_assignment_statement() {
        let test_string = "x = 42";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert!(matches!(stmts[0], StatementNode::Assignment { .. }));
    }

    #[test]
    fn test_array_literal() {
        let test_string = "[1, 2, 3]";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        if let StatementNode::ExpressionStatement(Expression::ArrayLiteral { elements }) = stmts[0]
        {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected ArrayLiteral, got {:#?}", stmts[0]);
        }
    }
}
