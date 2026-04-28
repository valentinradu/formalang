#![allow(clippy::expect_used, clippy::indexing_slicing)]

use super::{expr_has_closure, find_residual_closures};
use crate::ast::{Literal, PrimitiveType, Visibility};
use crate::ir::{IrExpr, IrFunction, IrLet, IrModule, ResolvedType};

fn unit_closure_expr() -> IrExpr {
    IrExpr::Closure {
        params: Vec::new(),
        captures: Vec::new(),
        body: Box::new(IrExpr::Literal {
            value: Literal::Boolean(true),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        }),
        ty: ResolvedType::Closure {
            param_tys: Vec::new(),
            return_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Boolean)),
        },
    }
}

#[test]
fn expr_has_closure_detects_top_level() {
    assert!(expr_has_closure(&unit_closure_expr()));
}

#[test]
fn expr_has_closure_finds_nested_closure_in_block() {
    let block = IrExpr::Block {
        statements: Vec::new(),
        result: Box::new(unit_closure_expr()),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    };
    assert!(expr_has_closure(&block));
}

#[test]
fn expr_has_closure_returns_false_for_closure_ref() {
    // `IrExpr::ClosureRef` is the *converted* form — it must NOT
    // be flagged as a residual.
    let closure_ref = IrExpr::ClosureRef {
        funcref: vec!["__closure0".to_string()],
        env_struct: Box::new(IrExpr::Literal {
            value: Literal::Boolean(true),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        }),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    };
    assert!(!expr_has_closure(&closure_ref));
}

#[test]
fn find_residual_closures_locates_let_value() {
    let mut module = IrModule::new();
    module.lets.push(IrLet {
        name: "bad".to_string(),
        visibility: Visibility::Private,
        mutable: false,
        ty: ResolvedType::Closure {
            param_tys: Vec::new(),
            return_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Boolean)),
        },
        value: unit_closure_expr(),
        doc: None,
    });
    let hits = find_residual_closures(&module);
    let first = hits.first().expect("one residual hit expected");
    assert_eq!(hits.len(), 1);
    assert!(first.contains("bad"), "{first}");
}

#[test]
fn find_residual_closures_locates_function_body() {
    let mut module = IrModule::new();
    module.functions.push(IrFunction {
        name: "f".to_string(),
        generic_params: Vec::new(),
        params: Vec::new(),
        return_type: Some(ResolvedType::Primitive(PrimitiveType::Boolean)),
        body: Some(unit_closure_expr()),
        extern_abi: None,
        attributes: Vec::new(),
        doc: None,
    });
    let hits = find_residual_closures(&module);
    let first = hits.first().expect("one residual hit expected");
    assert_eq!(hits.len(), 1);
    assert!(first.contains("function `f`"), "{first}");
}

#[test]
fn find_residual_closures_returns_empty_on_clean_module() {
    let module = IrModule::new();
    assert!(find_residual_closures(&module).is_empty());
}
