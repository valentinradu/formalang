use super::*;
use crate::location::Span;

#[test]
fn test_binary_operator_precedence_all() -> Result<(), Box<dyn std::error::Error>> {
    if BinaryOperator::Or.precedence() != 1 {
        return Err(format!("expected 1, got {:?}", BinaryOperator::Or.precedence()).into());
    }
    if BinaryOperator::And.precedence() != 2 {
        return Err(format!("expected 2, got {:?}", BinaryOperator::And.precedence()).into());
    }
    if BinaryOperator::Eq.precedence() != 3 {
        return Err(format!("expected 3, got {:?}", BinaryOperator::Eq.precedence()).into());
    }
    if BinaryOperator::Ne.precedence() != 3 {
        return Err(format!("expected 3, got {:?}", BinaryOperator::Ne.precedence()).into());
    }
    if BinaryOperator::Lt.precedence() != 4 {
        return Err(format!("expected 4, got {:?}", BinaryOperator::Lt.precedence()).into());
    }
    if BinaryOperator::Gt.precedence() != 4 {
        return Err(format!("expected 4, got {:?}", BinaryOperator::Gt.precedence()).into());
    }
    if BinaryOperator::Le.precedence() != 4 {
        return Err(format!("expected 4, got {:?}", BinaryOperator::Le.precedence()).into());
    }
    if BinaryOperator::Ge.precedence() != 4 {
        return Err(format!("expected 4, got {:?}", BinaryOperator::Ge.precedence()).into());
    }
    if BinaryOperator::Add.precedence() != 5 {
        return Err(format!("expected 5, got {:?}", BinaryOperator::Add.precedence()).into());
    }
    if BinaryOperator::Sub.precedence() != 5 {
        return Err(format!("expected 5, got {:?}", BinaryOperator::Sub.precedence()).into());
    }
    if BinaryOperator::Mul.precedence() != 6 {
        return Err(format!("expected 6, got {:?}", BinaryOperator::Mul.precedence()).into());
    }
    if BinaryOperator::Div.precedence() != 6 {
        return Err(format!("expected 6, got {:?}", BinaryOperator::Div.precedence()).into());
    }
    if BinaryOperator::Mod.precedence() != 6 {
        return Err(format!("expected 6, got {:?}", BinaryOperator::Mod.precedence()).into());
    }
    Ok(())
}

#[test]
fn test_binary_operator_precedence_order() -> Result<(), Box<dyn std::error::Error>> {
    if BinaryOperator::Mul.precedence() <= BinaryOperator::Add.precedence() {
        return Err("mul > add".into());
    }
    if BinaryOperator::Add.precedence() <= BinaryOperator::Lt.precedence() {
        return Err("add > lt".into());
    }
    if BinaryOperator::Lt.precedence() <= BinaryOperator::Eq.precedence() {
        return Err("lt > eq".into());
    }
    if BinaryOperator::Eq.precedence() <= BinaryOperator::And.precedence() {
        return Err("eq > and".into());
    }
    if BinaryOperator::And.precedence() <= BinaryOperator::Or.precedence() {
        return Err("and > or".into());
    }
    Ok(())
}

#[test]
fn test_binary_operator_is_left_associative() -> Result<(), Box<dyn std::error::Error>> {
    if !BinaryOperator::Add.is_left_associative() {
        return Err("Add".into());
    }
    if !BinaryOperator::Mul.is_left_associative() {
        return Err("Mul".into());
    }
    if !BinaryOperator::Or.is_left_associative() {
        return Err("Or".into());
    }
    Ok(())
}

#[test]
fn test_expr_span_literal() -> Result<(), Box<dyn std::error::Error>> {
    let expr = Expr::Literal {
        value: Literal::Nil,
        span: Span::default(),
    };
    if expr.span() != Span::default() {
        return Err("Literal should return default span".into());
    }
    Ok(())
}

#[test]
fn test_expr_span_invocation() -> Result<(), Box<dyn std::error::Error>> {
    let test_span = Span::from_range(10, 20);
    let expr = Expr::Invocation {
        path: vec![Ident::new("Test", Span::default())],
        type_args: vec![],
        args: vec![],
        span: test_span,
    };
    if expr.span() != test_span {
        return Err(format!("expected {test_span:?}, got {:?}", expr.span()).into());
    }
    Ok(())
}

#[test]
fn test_file_new_sets_format_version() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::new(vec![], Span::default());
    if file.format_version != FORMAT_VERSION {
        return Err(format!("expected {FORMAT_VERSION}, got {}", file.format_version).into());
    }
    Ok(())
}
