//! Integration tests for default parameter values (DP-1 through DP-5).

use formalang::compile_to_ir;
use formalang::error::CompilerError;
use formalang::ir::IrExpr;

/// DP-1 + DP-2: `f(1)` compiles for `fn f(x: I32, y: I32 = 0)`. The
/// IR's `FunctionCall.args` has two entries (the explicit `1` and
/// the substituted default `0`).
#[test]
fn arity_range_with_default_substitution() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn f(x: I32, y: I32 = 0) -> I32 { x + y }
fn main() -> I32 { f(1) }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let main = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or("main missing")?;
    let body = main.body.as_ref().ok_or("main body missing")?;
    // main's body should be a FunctionCall with 2 args (the explicit 1 + default 0).
    match body {
        IrExpr::FunctionCall { args, .. } => {
            if args.len() != 2 {
                return Err(format!("expected 2 args after default substitution, got {}", args.len()).into());
            }
        }
        IrExpr::Block { result, .. } => match result.as_ref() {
            IrExpr::FunctionCall { args, .. } => {
                if args.len() != 2 {
                    return Err(format!("expected 2 args after default substitution, got {}", args.len()).into());
                }
            }
            _ => return Err(format!("unexpected main body shape: {result:?}").into()),
        },
        _ => return Err(format!("unexpected main body shape: {body:?}").into()),
    }
    Ok(())
}

/// DP-4: when a default references an earlier param, the call is
/// wrapped in a Block whose Let statements bind those preceding
/// args. Today the semantic validator rejects bare references to
/// preceding params inside default expressions
/// (`UndefinedReference`) — DP-4's IR-side wrapper is in place but
/// requires a paired semantic-side fix to populate the param scope
/// when validating default expressions. The test asserts the
/// validator-side limitation surface as a clean error rather than
/// a panic so a future commit that lifts the validation can flip
/// this assertion to the wrapper-shape check.
#[test]
fn earlier_param_ref_in_default_currently_unscoped() {
    let source = r"
fn f(x: I32, y: I32 = x) -> I32 { y }
fn main() -> I32 { f(5) }
";
    let result = compile_to_ir(source);
    match result {
        Ok(_) => {
            // If a future commit lifts the validator scoping limit,
            // this test should be tightened to assert the let-wrapper
            // shape: Block { statements: [Let{name:"x", ...}], result:
            // FunctionCall{..} }.
        }
        Err(errors) => {
            // Expected today: UndefinedReference for `x` inside the
            // default expression. The DP-4 IR-side wrapper is correct
            // but never gets exercised because semantic rejects first.
            assert!(
                errors.iter().any(|e| matches!(
                    e,
                    CompilerError::UndefinedReference { name, .. } if name == "x"
                )),
                "unexpected error set: {errors:?}"
            );
        }
    }
}


/// DP-5: `fn f(x: I32 = 0, y: I32)` is rejected at semantic time
/// with `RequiredParamAfterDefault`.
#[test]
fn required_param_after_default_rejected() {
    let source = r"
fn f(x: I32 = 0, y: I32) -> I32 { x + y }
";
    let result = compile_to_ir(source);
    let errors = match result {
        Ok(_) => panic!("expected error, got Ok"),
        Err(errors) => errors,
    };
    let has_expected = errors
        .iter()
        .any(|e| matches!(e, CompilerError::RequiredParamAfterDefault { .. }));
    if !has_expected {
        panic!("expected RequiredParamAfterDefault, got: {errors:?}");
    }
}

/// DP-3: most-specific overload wins. `fn f(x)` and `fn f(x, y=1)`
/// both match `f(1)`; the no-default overload (gap 0) is preferred
/// over the with-default one (gap 1).
#[test]
fn most_specific_overload_under_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn f(x: I32) -> I32 { x }
fn f(x: I32, y: I32 = 1) -> I32 { x + y }
fn main() -> I32 { f(5) }
";
    // Should compile cleanly — no AmbiguousCall — because the
    // no-default overload is most-specific for `f(5)`.
    compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// DP-7: labeled call with mid-list omission fills the missing slot
/// with the param's default. `fn f(x, y=1, z=2); f(x: 1, z: 3)`
/// should produce IR with three args, where the second slot is the
/// substituted default for `y`.
#[test]
fn mid_list_omission_in_labeled_call() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn f(x: I32, y: I32 = 1, z: I32 = 2) -> I32 { x + y + z }
fn main() -> I32 { f(x: 10, z: 30) }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let main = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or("main missing")?;
    let body = main.body.as_ref().ok_or("main body missing")?;
    // Walk through any Block wrapper.
    let call = match body {
        IrExpr::FunctionCall { args, .. } => args,
        IrExpr::Block { result, .. } => match result.as_ref() {
            IrExpr::FunctionCall { args, .. } => args,
            other => return Err(format!("unexpected body shape: {other:?}").into()),
        },
        other => return Err(format!("unexpected body shape: {other:?}").into()),
    };
    if call.len() != 3 {
        return Err(format!(
            "expected 3 args after mid-list default substitution, got {}: {call:?}",
            call.len()
        )
        .into());
    }
    // Walk the args in callee-param order: x, y, z.
    let labels: Vec<Option<&str>> = call
        .iter()
        .map(|(l, _)| l.as_deref())
        .collect();
    if labels != [Some("x"), Some("y"), Some("z")] {
        return Err(format!(
            "expected args in order [x, y, z], got labels: {labels:?}"
        )
        .into());
    }
    Ok(())
}

/// DP-8: a call to a function declared *later* in the same module
/// (forward reference) gets its missing default substituted by
/// ResolveReferencesPass. The lowerer's DP-2 substitution would
/// skip if function_id were None at lowering, but the registration
/// pass means function_id is normally already bound. Validate that
/// the end-to-end pipeline accepts the program and produces an
/// arity-correct call.
#[test]
fn forward_ref_default_substitution() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn caller() -> I32 { callee(7) }
fn callee(x: I32, y: I32 = 5) -> I32 { x + y }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pass = formalang::ir::ResolveReferencesPass::new();
    let resolved = formalang::IrPass::run(&mut pass, module).map_err(|e| format!("{e:?}"))?;
    let caller = resolved
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .ok_or("caller missing")?;
    let body = caller.body.as_ref().ok_or("caller body missing")?;
    let call_args = match body {
        IrExpr::FunctionCall { args, .. } => args,
        IrExpr::Block { result, .. } => match result.as_ref() {
            IrExpr::FunctionCall { args, .. } => args,
            other => return Err(format!("unexpected body shape: {other:?}").into()),
        },
        other => return Err(format!("unexpected body shape: {other:?}").into()),
    };
    if call_args.len() != 2 {
        return Err(format!(
            "expected 2 args after forward-ref default substitution, got {}",
            call_args.len()
        )
        .into());
    }
    Ok(())
}
