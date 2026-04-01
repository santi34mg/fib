#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::ast::{
        Ast, ConstantDeclaration, DeclarationNode, Expression, StatementNode, TypeExpression,
    };
    use crate::lexing::Lexer;
    use crate::parsing::Parser;
    use crate::tokens::{Literal, Operator, Token};

    fn get_ast(test_string: &str) -> Ast {
        let src = test_string.to_string();
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
            if let DeclarationNode::Statement(s) = decl {
                stmts.push(s);
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
    fn test_full_const_declaration() {
        let test_string = "const int4 x = 5;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ConstantDeclaration(ConstantDeclaration {
            identifier,
            constant_type: Some(TypeExpression::Builtin(_)),
            expression: Expression::Literal(Literal::Integer(5)),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "x");
        } else {
            panic!("AST statement did not match expected ConstantDeclaration, got: {:#?}", stmts);
        }
    }

    #[test]
    fn test_const_declaration_without_type() {
        let test_string = "const x = 5;";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ConstantDeclaration(ConstantDeclaration {
            identifier,
            constant_type: None,
            expression: Expression::Literal(Literal::Integer(5)),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "x");
        } else {
            panic!("AST statement did not match expected ConstantDeclaration, got: {:#?}", stmts);
        }
    }

    #[test]
    fn test_const_declaration_without_semicolon() {
        let test_string = "const int4 x = 5";
        let ast = get_ast(test_string);
        let stmts = module_statements(&ast);
        assert_eq!(stmts.len(), 1);
        if let StatementNode::ConstantDeclaration(ConstantDeclaration {
            identifier,
            constant_type: Some(TypeExpression::Builtin(_)),
            expression: Expression::Literal(Literal::Integer(5)),
        }) = stmts[0]
        {
            assert_eq!(identifier.identifier, "x");
        } else {
            panic!("AST statement did not match expected ConstantDeclaration, got: {:#?}", stmts);
        }
    }
}
