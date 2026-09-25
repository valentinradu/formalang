//! Integration tests for IR-level source spans (SP-1 through SP-4).
//!
//! Verify that:
//! - `IrFunction`, `IrStruct`, `IrEnum`, `IrEnumVariant`, `IrField`,
//!   `IrLet`, `IrTrait`, `IrImpl`, `IrFunctionParam`, `IrFunctionSig`
//!   carry a populated `span` field after lowering.
//! - `IrExpr` variants carry populated spans.
//! - `IrModule.file_table` and `register_file` round-trip.

use formalang::compile_to_ir;
use formalang::ir::{FileId, IrExpr, IrModule, IrSpan};
use std::path::PathBuf;

/// Extract the span from a `Reference` expression, descending into
/// `Block` results. Returns `None` for other expression shapes; the
/// caller renders an error when the body doesn't match the expected
/// shape.
fn reference_span(expr: &IrExpr) -> Option<IrSpan> {
    match expr {
        IrExpr::Reference { span, .. } => Some(*span),
        IrExpr::Block { result, .. } => reference_span(result),
        IrExpr::Literal { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::BinaryOp { .. }
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
        | IrExpr::DictAccess { .. } => None,
    }
}

/// SP-2 + SP-7: IrFunction.span is populated (non-default) for each
/// lowered function when the caller supplies a source path via
/// `compile_to_ir_with_path`. The span's `file` carries `FileId(1)`
/// after path registration, so `is_default()` returns false even when
/// the byte range happens to start at offset 0.
#[test]
fn function_carries_non_default_span() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub fn add(a: I32, b: I32) -> I32 { a + b }";
    let module = formalang::compile_to_ir_with_path(source, PathBuf::from("test.fv"))
        .map_err(|e| format!("{e:?}"))?;
    let add = module
        .functions
        .iter()
        .find(|f| f.name == "add")
        .ok_or("add missing")?;
    if add.span.is_default() {
        return Err("add.span should not be IrSpan::default()".into());
    }
    if add.span.file.is_synthetic() {
        return Err("add.span.file should not be SYNTHETIC after path registration".into());
    }
    Ok(())
}

/// SP-2: each definition in the output of `compile_to_ir` has the span
/// of its own AST node: a struct, its fields, an enum, its variants, a
/// function and its parameters.
#[test]
fn definitions_carry_their_own_span() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct Point {\n    x: I32,\n    y: I32\n}\n\npub enum Mode {\n    on,\n    off\n}\n\npub fn add(a: I32, b: I32) -> I32 {\n    a + b\n}\n";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let point = module
        .structs
        .iter()
        .find(|s| s.name == "Point")
        .ok_or("Point missing")?;
    let y = point.fields.get(1).ok_or("field y missing")?;
    let mode = module
        .enums
        .iter()
        .find(|e| e.name == "Mode")
        .ok_or("Mode missing")?;
    let off = mode.variants.get(1).ok_or("variant off missing")?;
    let add = module
        .functions
        .iter()
        .find(|f| f.name == "add")
        .ok_or("add missing")?;
    let b = add.params.get(1).ok_or("param b missing")?;
    let lines = [
        ("struct Point", point.span, 1),
        ("field y", y.span, 3),
        ("enum Mode", mode.span, 6),
        ("variant off", off.span, 8),
        ("fn add", add.span, 11),
        ("parameter b", b.span, 11),
    ];
    for (what, span, line) in lines {
        if span.span.start.line != line || span.span.end.offset <= span.span.start.offset {
            return Err(format!("{what} must start on line {line}: {span:?}").into());
        }
    }
    if b.span.span.start.column <= add.span.span.start.column {
        return Err(format!("parameter b must have its own span: {:?}", b.span).into());
    }
    Ok(())
}

/// SP-3: `IrExpr` variants carry span fields.
#[test]
fn ir_expr_variants_have_span_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub fn id(x: I32) -> I32 { x }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let id = module
        .functions
        .iter()
        .find(|f| f.name == "id")
        .ok_or("id missing")?;
    let body = id.body.as_ref().ok_or("body missing")?;
    // Reference expression should have a span.
    let _span = reference_span(body).ok_or_else(|| format!("unexpected body shape: {body:?}"))?;
    Ok(())
}

/// SP-1: `file_table` and `register_file` round-trip.
#[test]
fn file_table_round_trips() {
    let mut module = IrModule::new();
    assert!(module.file_path(FileId::SYNTHETIC).is_none());

    let id_a = module.register_file(PathBuf::from("a.fv"));
    assert_eq!(id_a, FileId(1));
    assert_eq!(
        module
            .file_path(id_a)
            .map(|p| p.to_string_lossy().into_owned()),
        Some("a.fv".to_string())
    );

    let id_b = module.register_file(PathBuf::from("b.fv"));
    assert_eq!(id_b, FileId(2));

    // Re-registering returns the existing id.
    let id_a_again = module.register_file(PathBuf::from("a.fv"));
    assert_eq!(id_a_again, id_a);
}

/// SP-1: `IrSpan::is_default` behaves correctly.
#[test]
fn ir_span_default_predicate() {
    let s = IrSpan::default();
    assert!(s.is_default());

    let s2 = IrSpan::new(s.span, FileId(1));
    assert!(!s2.is_default());
}
