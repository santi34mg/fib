#[cfg(test)]
mod tests {
    use crate::ast::ast::{
        DeclarationNode, Expression, Field, PointerVariant, TypeIdentifier, VariableDeclaration,
    };
    use crate::ast::{Ast, StatementNode};
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::token::{Literal, Operator, Token};

    fn get_ast(test_string: &str) -> Ast {
        // ensure input is wrapped in a module for parser.parse_module
        let src = if test_string.trim_start().starts_with("module") {
            test_string.to_string()
        } else {
            format!("module test; {}", test_string)
        };
        let lexer = Lexer::new(&src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), "test_instance", src.clone());
        match parser.parse_module() {
            Ok(ast) => ast,
            Err(e) => panic!("Could not parse {}, error:\n{}", src, e),
        }
    }

    fn module_statements(ast: &Ast) -> Vec<&StatementNode> {
        let mut stmts = Vec::new();
        if let Some(module) = ast.program.modules.get(0) {
            for decl in &module.declarations {
                if let DeclarationNode::Statement(s) = decl {
                    stmts.push(s);
                }
            }
        }
        stmts
    }

    #[test]
    fn test_expression_literal() {
        let test_string = "1";
        let ast = get_ast(test_string);
        assert_eq!(ast.program.modules.len(), 1);
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
            assert!(matches!(op, Operator::Multiply));
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
            assert!(matches!(op, Operator::Divide));
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
    fn test_full_variable_declaration() {
        let test_string = "let x int = 5;";
        let ast = get_ast(test_string);
        // println!("{:#?}", ast.statements); // Removed: Ast has no 'statements' field
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: Some(TypeIdentifier::Integer),
            expression: Some(Expression::Literal(Literal::Integer(5))),
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    #[test]
    fn test_variable_declaration_without_type() {
        let test_string = "let x = 5;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: None,
            expression: Some(Expression::Literal(Literal::Integer(5))),
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    #[test]
    fn test_variable_declaration_without_expresion() {
        let test_string = "let x int;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: Some(TypeIdentifier::Integer),
            expression: None,
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    #[test]
    fn test_variable_declaration_without_semicolon() {
        let test_string = "let x int = 5";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: Some(TypeIdentifier::Integer),
            expression: Some(Expression::Literal(Literal::Integer(5))),
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    #[test]
    fn test_variable_declaration_only_identifier() {
        let test_string = "let x";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: None,
            expression: None,
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    fn test_type(type_string: String, expected_type: TypeIdentifier) {
        let test_string = format!("let x {};", type_string);
        let ast = get_ast(&test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::VariableDeclaration(VariableDeclaration {
            identifier,
            variable_type: Some(var_type),
            expression: None,
        }) = stmts[0]
        {
            assert_eq!(identifier, "x");
            assert_eq!(*var_type, expected_type);
        } else {
            panic!("AST statement did not match expected VariableDeclaration");
        }
    }

    #[test]
    fn test_type_integer() {
        test_type("int".to_string(), TypeIdentifier::Integer);
    }

    #[test]
    fn test_type_float() {
        test_type("float".to_string(), TypeIdentifier::Float);
    }

    #[test]
    fn test_type_bool() {
        test_type("bool".to_string(), TypeIdentifier::Boolean);
    }

    #[test]
    fn test_type_char() {
        test_type("char".to_string(), TypeIdentifier::Character);
    }

    #[test]
    fn test_type_unit() {
        test_type("unit".to_string(), TypeIdentifier::Unit);
    }

    #[test]
    fn test_type_string() {
        test_type("string".to_string(), TypeIdentifier::String);
    }

    #[test]
    fn test_type_dynamic() {
        test_type("dynamic".to_string(), TypeIdentifier::Dynamic);
    }

    #[test]
    fn test_type_blob() {
        test_type("blob".to_string(), TypeIdentifier::Blob);
    }

    #[test]
    fn test_type_never() {
        test_type("never".to_string(), TypeIdentifier::Never);
    }

    #[test]
    fn test_type_raw_pointer() {
        test_type(
            "&int".to_string(),
            TypeIdentifier::Pointer {
                pointer_variant: PointerVariant::Raw,
                pointed_type: Box::new(TypeIdentifier::Integer),
            },
        );
    }

    #[test]
    fn test_type_unique_pointer() {
        test_type(
            "unique &int".to_string(),
            TypeIdentifier::Pointer {
                pointer_variant: PointerVariant::Unique,
                pointed_type: Box::new(TypeIdentifier::Integer),
            },
        );
    }

    #[test]
    fn test_type_shared_pointer() {
        test_type(
            "shared &int".to_string(),
            TypeIdentifier::Pointer {
                pointer_variant: PointerVariant::Shared,
                pointed_type: Box::new(TypeIdentifier::Integer),
            },
        );
    }

    #[test]
    fn test_type_weak_pointer() {
        test_type(
            "weak &int".to_string(),
            TypeIdentifier::Pointer {
                pointer_variant: PointerVariant::Weak,
                pointed_type: Box::new(TypeIdentifier::Integer),
            },
        );
    }

    #[test]
    fn test_type_struct() {
        test_type(
            "struct { x int, y float }".to_string(),
            TypeIdentifier::Struct {
                fields: vec![
                    Field {
                        label: "x".to_string(),
                        type_id: TypeIdentifier::Integer,
                    },
                    Field {
                        label: "y".to_string(),
                        type_id: TypeIdentifier::Float,
                    },
                ],
            },
        );
    }

    #[test]
    fn test_type_variant() {
        test_type(
            "variant { x int, y float }".to_string(),
            TypeIdentifier::Variant {
                fields: vec![
                    Field {
                        label: "x".to_string(),
                        type_id: TypeIdentifier::Integer,
                    },
                    Field {
                        label: "y".to_string(),
                        type_id: TypeIdentifier::Float,
                    },
                ],
            },
        );
    }

    #[test]
    fn test_type_function_simple() {
        test_type(
            "function(int) -> bool".to_string(),
            TypeIdentifier::Function {
                argument_types: vec![TypeIdentifier::Integer],
                return_type: Box::new(TypeIdentifier::Boolean),
            },
        );
    }
}
