//! Integration tests for default parameter values (DP-1 through DP-5).

use formalang::compile_to_ir;
use formalang::error::CompilerError;
use formalang::ir::{IrBlockStatement, IrExpr};

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

/// DP-4: `fn f(x, y = x + 1); f(some_expr)` wraps the call in a
/// Block with a Let binding for `x` so the default's `Reference{x}`
/// resolves to the binding (not the callee's stale binding-id).
#[test]
fn let_wrapper_for_earlier_param_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn f(x: I32, y: I32 = x) -> I32 { y }
fn main() -> I32 { f(5) }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let main = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or("main missing")?;
    let body = main.body.as_ref().ok_or("main body missing")?;
    match body {
        IrExpr::Block { statements, result, .. } => {
            // Should have at least one Let binding (for `x`).
            let has_x_let = statements.iter().any(|s| matches!(s, IrBlockStatement::Let { name, .. } if name == "x"));
            if !has_x_let {
                return Err(format!("expected a Let binding for `x`, got: {statements:?}").into());
            }
            // Result should be the FunctionCall.
            if !matches!(result.as_ref(), IrExpr::FunctionCall { .. }) {
                return Err(format!("expected Block.result to be FunctionCall, got: {result:?}").into());
            }
        }
        _ => return Err(format!("expected Block wrapping for earlier-param-ref default, got: {body:?}").into()),
    }
    Ok(())
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
