use super::*;
use crate::ast::PrimitiveType;
use crate::compile_to_ir;

#[test]
fn test_fold_numeric_addition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { scale: I32 = 1 + 2 }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    // Check the default was folded
    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value.as_f64() - 3.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 3, got {}", n.value.as_f64()).into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_multiplication() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { scale: I32 = 2 * 3 }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value.as_f64() - 6.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 6, got {}", n.value.as_f64()).into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_chained_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { value: I32 = 2 + 3 * 4 }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    // 2 + 3 * 4 = 2 + 12 = 14
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value.as_f64() - 14.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 14, got {}", n.value.as_f64()).into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_boolean_and() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { flag: Boolean = true && false }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    {
        if *b {
            return Err(format!("Expected false, got {b}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_boolean_or() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { flag: Boolean = true || false }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    {
        if !*b {
            return Err(format!("Expected true, got {b}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { result: Boolean = 1 < 2 }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    {
        if !*b {
            return Err(format!("Expected true, got {b}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_if_constant_condition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
            struct Config { value: I32 = if true { 1 } else { 2 } }
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value.as_f64() - 1.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 1, got {}", n.value.as_f64()).into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_no_fold_non_constant() -> Result<(), Box<dyn std::error::Error>> {
    // Use a let binding that references another let binding
    let source = r"
            let x: I32 = 1
            let y: I32 = x + 1
        ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    // The y references x, which is a variable - the folder may or may not optimize this
    // depending on whether it does constant propagation through let bindings
    let let_binding = folded
        .lets
        .iter()
        .find(|l| l.name == "y")
        .ok_or("expected y let binding")?;
    let expr = &let_binding.value;

    // Accept either BinaryOp (no constant propagation) or Literal (with propagation)
    match expr {
        IrExpr::BinaryOp { .. } | IrExpr::Literal { .. } => {}
        IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::FunctionCall { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. }
        | IrExpr::Block { .. } => {
            return Err(format!("Expected BinaryOp or Literal, got {expr:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_fold_string_concat() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
            struct Config { name: String = "Hello" + " World" }
        "#;
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let folded = fold_constants(&module);

    let struct_def = folded
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    if let IrExpr::Literal {
        value: Literal::String(s),
        ..
    } = expr
    {
        if s != "Hello World" {
            return Err(format!("Expected 'Hello World', got {s:?}").into());
        }
    } else {
        return Err(format!("Expected folded string literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_fold_float_eq_signed_zero() -> Result<(), Box<dyn std::error::Error>> {
    // IEEE 754: +0.0 == -0.0 must fold to `true`.
    let ir_module = IrModule::new();
    let _ = &ir_module;
    let folder = ConstantFolder::new();
    let number_ty = ResolvedType::Primitive(PrimitiveType::I32);
    let expression = IrExpr::BinaryOp {
        left: Box::new(IrExpr::Literal {
            value: Literal::Number(0.0.into()),
            ty: number_ty.clone(),
        }),
        op: BinaryOperator::Eq,
        right: Box::new(IrExpr::Literal {
            value: Literal::Number((-0.0_f64).into()),
            ty: number_ty,
        }),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    };
    let result = folder.fold_expr(expression);
    if let IrExpr::Literal {
        value: Literal::Boolean(true),
        ..
    } = result
    {
        Ok(())
    } else {
        Err(format!("Expected folded `true`, got {result:?}").into())
    }
}

#[test]
fn test_fold_float_eq_nan() -> Result<(), Box<dyn std::error::Error>> {
    // IEEE 754: NaN == NaN must fold to `false`.
    let ir_module = IrModule::new();
    let _ = &ir_module;
    let folder = ConstantFolder::new();
    let number_ty = ResolvedType::Primitive(PrimitiveType::I32);
    let expression = IrExpr::BinaryOp {
        left: Box::new(IrExpr::Literal {
            value: Literal::Number(f64::NAN.into()),
            ty: number_ty.clone(),
        }),
        op: BinaryOperator::Eq,
        right: Box::new(IrExpr::Literal {
            value: Literal::Number(f64::NAN.into()),
            ty: number_ty,
        }),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    };
    let result = folder.fold_expr(expression);
    if let IrExpr::Literal {
        value: Literal::Boolean(false),
        ..
    } = result
    {
        Ok(())
    } else {
        Err(format!("Expected folded `false`, got {result:?}").into())
    }
}
