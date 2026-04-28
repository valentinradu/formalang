//! Round-trip and overflow-rejection coverage for the
//! `NumberValue::Integer(i128)` representation introduced in PR
//! `feat(ast): NumberValue discriminated union for exact integer literals`.
//!
//! Companion to `docs/developer/numeric_literal_precision.md`.

use formalang::ast::{Literal, NumberValue, PrimitiveType};
use formalang::error::CompilerError;
use formalang::ir::IrExpr;
use formalang::{compile_to_ir, IrModule};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn first_function_body_literal(module: &IrModule, fn_name: &str) -> Option<Literal> {
    let func = module.functions.iter().find(|f| f.name == fn_name)?;
    let body = func.body.as_ref()?;
    walk_for_literal(body)
}

fn walk_for_literal(expr: &IrExpr) -> Option<Literal> {
    match expr {
        IrExpr::Literal { value, .. } => Some(value.clone()),
        IrExpr::Block { result, .. } => walk_for_literal(result),
        IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::BinaryOp { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::FunctionCall { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. } => None,
    }
}

#[test]
fn i64_max_roundtrips_exactly() -> TestResult {
    // Above 2^53 — pre-refactor this rounded to 9_223_372_036_854_775_808
    // when the lexer parsed via f64.
    let source = "pub fn answer() -> I64 { 9223372036854775807I64 }";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let lit = first_function_body_literal(&module, "answer").ok_or("no literal in answer body")?;
    let Literal::Number(n) = lit else {
        return Err(format!("expected Literal::Number, got {lit:?}").into());
    };
    // `NumberValue` is `non_exhaustive`; a wildcard arm is mandatory.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "non_exhaustive enum requires a wildcard fall-through"
    )]
    match n.value {
        NumberValue::Integer(v) => {
            if v != 9_223_372_036_854_775_807_i128 {
                return Err(format!("lost precision: got {v}").into());
            }
        }
        other => return Err(format!("expected Integer variant, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn i32_overflow_rejected_at_semantic_analysis() -> TestResult {
    // 2^31 = 2_147_483_648 — one past i32::MAX. The I32 suffix forces the
    // target primitive, so semantic analysis must reject the literal.
    let source = "pub fn x() -> I32 { 2147483648I32 }";
    let result = compile_to_ir(source);
    match result {
        Err(errors)
            if errors.iter().any(|e| {
                matches!(
                    e,
                    CompilerError::NumericOverflow {
                        target: PrimitiveType::I32,
                        ..
                    }
                )
            }) =>
        {
            Ok(())
        }
        other => Err(format!("expected NumericOverflow, got {other:?}").into()),
    }
}

#[test]
fn unsuffixed_integer_default_i32_overflow_rejected() -> TestResult {
    // Unsuffixed integer literals default to I32; an unsuffixed
    // `9_999_999_999` must therefore fail the I32 range check rather than
    // silently widening to F64 or wrapping.
    let source = "pub fn x() -> I32 { 9999999999 }";
    let result = compile_to_ir(source);
    match result {
        Err(errors)
            if errors.iter().any(|e| {
                matches!(
                    e,
                    CompilerError::NumericOverflow {
                        target: PrimitiveType::I32,
                        ..
                    }
                )
            }) =>
        {
            Ok(())
        }
        other => Err(format!("expected NumericOverflow, got {other:?}").into()),
    }
}

#[test]
fn i64_overflow_rejected() -> TestResult {
    // 2^63 — one past i64::MAX. Even with the I64 suffix, the literal must
    // be rejected (i128 storage allowed it through the lexer).
    let source = "pub fn x() -> I64 { 9223372036854775808I64 }";
    let result = compile_to_ir(source);
    match result {
        Err(errors)
            if errors.iter().any(|e| {
                matches!(
                    e,
                    CompilerError::NumericOverflow {
                        target: PrimitiveType::I64,
                        ..
                    }
                )
            }) =>
        {
            Ok(())
        }
        other => Err(format!("expected NumericOverflow, got {other:?}").into()),
    }
}
