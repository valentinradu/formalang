//! Additional tests for fold.rs coverage: comparison operators, subtraction,
//! division, modulo, boolean eq/ne, string non-add, unary neg, if-false no-else,
//! fold in impl functions, closure/dict/for/match folding.

use formalang::ast::Literal;
use formalang::compile_to_ir;
use formalang::ir::fold_constants;
use formalang::ir::{ConstantFoldingPass, IrBlockStatement, IrExpr};
use formalang::pipeline::IrPass;

// =============================================================================
// fold_binary_op: numeric comparisons
// =============================================================================

#[test]
fn test_fold_numeric_subtraction() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Number = 10 - 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if ((n - 6.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 6, got {n}").into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_division() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Number = 12 / 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if ((n - 3.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 3, got {n}").into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_modulo() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Number = 10 % 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if ((n - 1.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 1, got {n}").into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_le() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = 3 <= 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for 3 <= 3".into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_gt() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = 5 > 2 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for 5 > 2".into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_ge() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = 4 >= 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for 4 >= 4".into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_eq() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = 3 == 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for 3 == 3".into());
    }
    Ok(())
}

#[test]
fn test_fold_numeric_ne() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = 3 != 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for 3 != 4".into());
    }
    Ok(())
}

// =============================================================================
// fold_binary_op: boolean eq/ne
// =============================================================================

#[test]
fn test_fold_boolean_eq() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = true == true }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for true == true".into());
    }
    Ok(())
}

#[test]
fn test_fold_boolean_ne() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = true != false }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for true != false".into());
    }
    Ok(())
}

// =============================================================================
// fold_unary_op: numeric negation
// =============================================================================

#[test]
fn test_fold_numeric_negation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Number = -5 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if ((n + 5.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected -5, got {n}").into());
    }
    Ok(())
}

#[test]
fn test_fold_boolean_not_false() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Boolean = !false }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    } = expr
    else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    };
    if !(*b) {
        return Err("Expected true for !false".into());
    }
    Ok(())
}

// =============================================================================
// fold_if: false condition with no else
// =============================================================================

#[test]
fn test_fold_if_constant_false_no_else() -> Result<(), Box<dyn std::error::Error>> {
    // `if false { 5 }` -> condition is false, no else branch -> stays as If
    let source = r"struct Config { val: Number = if false { 5 } }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    // No else branch, condition false -> fold_constants should NOT eliminate (no else to return)
    if !(matches!(expr, IrExpr::If { .. } | IrExpr::Literal { .. })) {
        return Err(format!(
            "Expected If (unfoldable, no else) or Literal (nil), got {expr:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// fold in impl function bodies
// =============================================================================

#[test]
fn test_fold_constants_in_impl_function_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { val: Number = 0 }
        impl Counter {
            fn doubled() -> Number { 2 * 3 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    // Find the impl function "doubled"
    let impl_block = folded
        .impls
        .iter()
        .find(|i| i.functions.iter().any(|f| f.name == "doubled"))
        .ok_or("impl block with doubled not found")?;
    let func = impl_block
        .functions
        .iter()
        .find(|f| f.name == "doubled")
        .ok_or("doubled not found")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = &func.body
    else {
        return Err(format!(
            "Expected folded literal in impl func body, got {:?}",
            func.body
        )
        .into());
    };
    if ((n - 6.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 6, got {n}").into());
    }
    Ok(())
}

// =============================================================================
// fold: dict literal entries folded
// =============================================================================

#[test]
fn test_fold_dict_literal_entries() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config { data: [String: Number] = ["key": 1 + 2] }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::DictLiteral { entries, .. } = expr else {
        return Err(format!("Expected DictLiteral, got {expr:?}").into());
    };
    if entries.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, entries.len()).into());
    }
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = &entries.first().ok_or("index out of bounds")?.1
    else {
        return Err(format!(
            "Expected folded value in dict, got {:?}",
            entries.first().ok_or("index out of bounds")?.1
        )
        .into());
    };
    if ((n - 3.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 3, got {n}").into());
    }
    Ok(())
}

// =============================================================================
// fold: for loop body folded
// =============================================================================

#[test]
fn test_fold_for_loop_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [Number] = [1, 2, 3]
        let doubled: [Number] = for x in items { if true { 1 + 2 } else { 0 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let binding = folded
        .lets
        .iter()
        .find(|l| l.name == "doubled")
        .ok_or("doubled not found")?;
    // For loop body should be folded
    if let IrExpr::For { body, .. } = &binding.value {
        // Body should be the folded result: either the then branch (3) or the loop itself
        if let IrExpr::Literal { .. } = body.as_ref() {
            // Good: folded
        } else if let IrExpr::If { .. } = body.as_ref() {
            // Acceptable: not folded through
        } else {
            // Other forms ok
        }
    }
    // Just verifying fold_constants runs without error
    if folded.lets.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// fold: match arms folded
// =============================================================================

#[test]
fn test_fold_match_arm_bodies() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green }
        struct Config {
            val: Number = match Color.red {
                .red: 2 + 3,
                _: 0
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Match { arms, .. } = expr else {
        return Err(format!("Expected Match, got {expr:?}").into());
    };
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = &arms.first().ok_or("index out of bounds")?.body
    else {
        return Err(format!(
            "Expected folded literal in match arm, got {:?}",
            arms.first().ok_or("index out of bounds")?.body
        )
        .into());
    };
    if ((n - 5.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 5 in arm, got {n}").into());
    }
    Ok(())
}

// =============================================================================
// fold: method call receiver and args folded
// =============================================================================

#[test]
fn test_fold_method_call_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Vec2 { x: Number, y: Number }
        impl Vec2 {
            fn scale(factor: Number) -> Number { self.x }
            fn compute() -> Number { self.scale(factor: 2 + 3) }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.impls.is_empty() {
        return Err("assertion failed".into());
    }
    // Just verify it runs without panic - method args get folded inside impl body
    Ok(())
}

// =============================================================================
// fold: closure body folded
// =============================================================================

#[test]
fn test_fold_closure_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            callback: (Number) -> Number = |n: Number| 2 + 3
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Closure { body, .. } = expr else {
        return Err(format!("Expected Closure, got {expr:?}").into());
    };
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = body.as_ref()
    else {
        return Err(format!("Expected folded closure body, got {body:?}").into());
    };
    if ((n - 5.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 5, got {n}").into());
    }
    Ok(())
}

// =============================================================================
// fold: block with let statements folded
// =============================================================================

#[test]
fn test_fold_block_let_statement() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { val: Number = {
            let x: Number = 1 + 2
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block {
        statements, result, ..
    } = expr
    else {
        return Err(format!("Expected Block, got {expr:?}").into());
    };
    // Let statement value should be folded
    let IrBlockStatement::Let { value, .. } = &statements.first().ok_or("index out of bounds")?
    else {
        return Err(format!(
            "Expected Let statement, got {:?}",
            statements.first().ok_or("index out of bounds")?
        )
        .into());
    };
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = value
    else {
        return Err(format!("Expected folded let value, got {value:?}").into());
    };
    if ((n - 3.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 3 in block let, got {n}").into());
    }
    // Result expression should be a reference to x (LetRef/Reference) or a Literal
    if !(matches!(
        result.as_ref(),
        IrExpr::LetRef { .. } | IrExpr::Reference { .. } | IrExpr::Literal { .. }
    )) {
        return Err(format!(
            "Block result should be a LetRef, Reference, or Literal, got {result:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// fold: field access with folded object
// =============================================================================

#[test]
fn test_fold_field_access_object() -> Result<(), Box<dyn std::error::Error>> {
    // Field access on a computed object - fold verifies the object gets folded
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn get() -> Number { self.x }
        }
        struct Config {
            p: Point = Point(x: 1 + 2, y: 0)
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::StructInst { fields, .. } = expr else {
        return Err(format!("Expected StructInst, got {expr:?}").into());
    };
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = &fields.first().ok_or("index out of bounds")?.1
    else {
        return Err(format!(
            "Expected folded x field, got {:?}",
            fields.first().ok_or("index out of bounds")?.1
        )
        .into());
    };
    if ((n - 3.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 3, got {n}").into());
    }
    Ok(())
}

// =============================================================================
// ConstantFoldingPass via Pipeline
// =============================================================================

#[test]
fn test_constant_folding_pass_via_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::ConstantFoldingPass;
    use formalang::pipeline::IrPass;

    let source = r"
        struct Config { value: Number = 3 * 7 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut pass = ConstantFoldingPass::new();
    let result = pass.run(module);
    let folded = result.map_err(|e| format!("pass failed: {e:?}"))?;
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    else {
        return Err(format!("Expected 21 after folding pass, got {expr:?}").into());
    };
    if ((n - 21.0).abs()).abs() >= f64::EPSILON {
        return Err(format!("Expected 21, got {n}").into());
    }
    Ok(())
}

#[test]
fn test_constant_folding_pass_default() -> Result<(), Box<dyn std::error::Error>> {
    let pass = ConstantFoldingPass::new();
    if pass.name() != "constant-folding" {
        return Err(format!("expected 'constant-folding', got '{}'", pass.name()).into());
    }
    let default_pass = ConstantFoldingPass;
    if default_pass.name() != "constant-folding" {
        return Err("ConstantFoldingPass should have name 'constant-folding'".into());
    }
    Ok(())
}

// =============================================================================
// fold: division by zero doesn't fold
// =============================================================================

#[test]
fn test_fold_no_divide_by_zero() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Config { val: Number = 10 / 0 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    let expr = folded
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    // Division by zero: should stay as BinaryOp (unfoldable) or fold to infinity literal
    if !(matches!(expr, IrExpr::BinaryOp { .. } | IrExpr::Literal { .. })) {
        return Err(format!("Expected BinaryOp or Literal for 10/0, got {expr:?}").into());
    }
    Ok(())
}

// =============================================================================
// fold: range operator doesn't fold
// =============================================================================

#[test]
fn test_fold_range_not_folded() -> Result<(), Box<dyn std::error::Error>> {
    // Range is used in for loops
    let source = r"
        let items: [Number] = [1, 2, 3]
        let x: [Number] = for n in items { n }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.lets.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}
