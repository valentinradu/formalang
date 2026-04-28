#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "non_exhaustive IrExpr requires wildcard fall-through in tests"
)]

//! Regression coverage for closure-typed values inside destructuring
//! let bindings.
//!
//! Before the fix, `lower_array_destructuring_let` and
//! `lower_tuple_destructuring_let` did not propagate the let's type
//! annotation down to closure literals nested inside the value, so an
//! un-annotated closure parameter lowered to `ResolvedType::Error`.

use formalang::ast::{ParamConvention, PrimitiveType};
use formalang::compile_to_ir;
use formalang::ir::{IrExpr, ResolvedType};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Walk a destructured-binding's synthesised access expression
/// (`arr[0]`, `tup.a`, `struct.f`) back to the source closure literal.
fn find_closure(expr: &IrExpr) -> Option<&IrExpr> {
    match expr {
        IrExpr::Closure { .. } => Some(expr),
        IrExpr::FieldAccess { object, .. } => find_closure(object),
        IrExpr::DictAccess { dict, .. } => match dict.as_ref() {
            IrExpr::Array { elements, .. } => elements.first().and_then(find_closure),
            other => find_closure(other),
        },
        IrExpr::Array { elements, .. } => elements.iter().find_map(find_closure),
        IrExpr::DictLiteral { entries, .. } => entries.iter().find_map(|(_, v)| find_closure(v)),
        IrExpr::Tuple { fields, .. } => fields.iter().find_map(|(_, e)| find_closure(e)),
        IrExpr::StructInst { fields, .. } => fields.iter().find_map(|(_, _, e)| find_closure(e)),
        _ => None,
    }
}

fn assert_closure_param_is_i32(value: &IrExpr, label: &str) -> TestResult {
    let closure =
        find_closure(value).ok_or_else(|| format!("[{label}] no closure found in {value:?}"))?;
    let IrExpr::Closure { params, .. } = closure else {
        return Err(format!("[{label}] expected Closure, got {closure:?}").into());
    };
    let (_, _, _, ty) = params.first().ok_or("missing param")?;
    if !matches!(ty, ResolvedType::Primitive(PrimitiveType::I32)) {
        return Err(format!("[{label}] expected param ty I32, got {ty:?}").into());
    }
    Ok(())
}

#[test]
fn array_destructuring_threads_closure_annotation() -> TestResult {
    let module =
        compile_to_ir("pub let [f]: [I32 -> I32] = [|x| x]").map_err(|e| format!("{e:?}"))?;
    let f = module.lets.iter().find(|l| l.name == "f").ok_or("no f")?;
    assert_closure_param_is_i32(&f.value, "array")
}

#[test]
fn tuple_destructuring_threads_closure_annotation() -> TestResult {
    let module =
        compile_to_ir("pub let (f): (a: I32 -> I32) = (a: |x| x)").map_err(|e| format!("{e:?}"))?;
    let f = module.lets.iter().find(|l| l.name == "f").ok_or("no f")?;
    assert_closure_param_is_i32(&f.value, "tuple")
}

#[test]
fn struct_destructuring_threads_closure_annotation() -> TestResult {
    // Struct destructuring already worked via the field-default machinery;
    // pin the behaviour with a regression test alongside the new ones.
    let module = compile_to_ir(
        r"
        pub struct Wrap { f: I32 -> I32 }
        pub let {f}: Wrap = Wrap(f: |x| x)
        ",
    )
    .map_err(|e| format!("{e:?}"))?;
    let f = module.lets.iter().find(|l| l.name == "f").ok_or("no f")?;
    assert_closure_param_is_i32(&f.value, "struct")
}

#[test]
fn dict_literal_threads_closure_annotation_to_entry_value() -> TestResult {
    // `let d: [String : I32 -> I32] = ["k": |x| x]` — the closure
    // entry's parameter type must come from the annotation's
    // `value_ty`, not fall through to `ResolvedType::Error`.
    let module = compile_to_ir(r#"pub let d: [String: I32 -> I32] = ["k": |x| x]"#)
        .map_err(|e| format!("{e:?}"))?;
    let d = module.lets.iter().find(|l| l.name == "d").ok_or("no d")?;
    let IrExpr::DictLiteral { entries, .. } = &d.value else {
        return Err(format!("expected DictLiteral, got {:?}", d.value).into());
    };
    let (_, value_expr) = entries.first().ok_or("no entries")?;
    assert_closure_param_is_i32(value_expr, "dict")
}

#[test]
fn array_of_dict_threads_closure_annotation() -> TestResult {
    // Nested container: `[[String: I32 -> I32]]` — array element is a
    // dictionary whose value is a closure. The annotation must reach
    // the inner closure literal through *two* container layers, not
    // stop at the array boundary.
    let module = compile_to_ir(r#"pub let [d]: [[String: I32 -> I32]] = [["k": |x| x]]"#)
        .map_err(|e| format!("{e:?}"))?;
    let d = module.lets.iter().find(|l| l.name == "d").ok_or("no d")?;
    assert_closure_param_is_i32(&d.value, "array<dict<closure>>")
}

#[test]
fn dict_of_array_threads_closure_annotation() -> TestResult {
    let module = compile_to_ir(r#"pub let m: [String: [I32 -> I32]] = ["k": [|x| x]]"#)
        .map_err(|e| format!("{e:?}"))?;
    let m = module.lets.iter().find(|l| l.name == "m").ok_or("no m")?;
    assert_closure_param_is_i32(&m.value, "dict<array<closure>>")
}

#[test]
fn tuple_of_array_threads_closure_annotation() -> TestResult {
    let module = compile_to_ir("pub let t: (a: [I32 -> I32]) = (a: [|x| x])")
        .map_err(|e| format!("{e:?}"))?;
    let t = module.lets.iter().find(|l| l.name == "t").ok_or("no t")?;
    assert_closure_param_is_i32(&t.value, "tuple<array<closure>>")
}

#[test]
fn array_destructuring_preserves_param_convention() -> TestResult {
    // `mut x` annotation on the closure param survives even when the
    // declared type comes from the let annotation.
    let module = compile_to_ir("pub let [f]: [mut I32 -> I32] = [|mut x| x]")
        .map_err(|e| format!("{e:?}"))?;
    let f = module.lets.iter().find(|l| l.name == "f").ok_or("no f")?;
    let closure = find_closure(&f.value).ok_or_else(|| format!("no closure in {:?}", f.value))?;
    let IrExpr::Closure { params, .. } = closure else {
        return Err(format!("expected Closure, got {closure:?}").into());
    };
    let (conv, _, _, _) = params.first().ok_or("missing param")?;
    if *conv != ParamConvention::Mut {
        return Err(format!("expected Mut convention, got {conv:?}").into());
    }
    Ok(())
}
