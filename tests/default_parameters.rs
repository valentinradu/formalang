//! Integration tests for default parameter values (DP-1 through DP-5).

use formalang::compile_to_ir;
use formalang::error::CompilerError;
use formalang::ir::{IrBlockStatement, IrExpr};

/// Return the arguments of a `FunctionCall` directly, or descend through
/// a `Block` whose result is a `FunctionCall`. `None` for any other shape.
fn function_call_args(expr: &IrExpr) -> Option<&Vec<(Option<String>, IrExpr)>> {
    match expr {
        IrExpr::FunctionCall { args, .. } => Some(args),
        IrExpr::Block { result, .. } => function_call_args(result),
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
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
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. } => None,
    }
}

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
    let args =
        function_call_args(body).ok_or_else(|| format!("unexpected main body shape: {body:?}"))?;
    if args.len() != 2 {
        return Err(format!(
            "expected 2 args after default substitution, got {}",
            args.len()
        )
        .into());
    }
    Ok(())
}

/// DP-4: when a default references an earlier param, the lowerer
/// scopes preceding params before lowering each default, and the
/// call site is wrapped in a Block whose Let statements bind those
/// preceding non-defaulted args so the default's `Reference{name}`
/// resolves to the let binding (not the callee's stale binding-id).
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
    let IrExpr::Block {
        statements, result, ..
    } = body
    else {
        return Err(format!(
            "expected Block wrapping for earlier-param-ref default, got: {body:?}"
        )
        .into());
    };
    let has_x_let = statements
        .iter()
        .any(|s| matches!(s, IrBlockStatement::Let { name, .. } if name == "x"));
    if !has_x_let {
        return Err(format!("expected a Let binding for `x`, got: {statements:?}").into());
    }
    if !matches!(result.as_ref(), IrExpr::FunctionCall { .. }) {
        return Err(format!("expected Block.result to be FunctionCall, got: {result:?}").into());
    }
    Ok(())
}

/// DP-5: `fn f(x: I32 = 0, y: I32)` is rejected at semantic time
/// with `RequiredParamAfterDefault`.
#[test]
fn required_param_after_default_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn f(x: I32 = 0, y: I32) -> I32 { x + y }
";
    let errors = compile_to_ir(source)
        .err()
        .ok_or("expected compile error, got Ok")?;
    let has_expected = errors
        .iter()
        .any(|e| matches!(e, CompilerError::RequiredParamAfterDefault { .. }));
    if !has_expected {
        return Err(format!("expected RequiredParamAfterDefault, got: {errors:?}").into());
    }
    Ok(())
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
    let call =
        function_call_args(body).ok_or_else(|| format!("unexpected body shape: {body:?}"))?;
    if call.len() != 3 {
        return Err(format!(
            "expected 3 args after mid-list default substitution, got {}: {call:?}",
            call.len()
        )
        .into());
    }
    // Walk the args in callee-param order: x, y, z.
    let labels: Vec<Option<&str>> = call.iter().map(|(l, _)| l.as_deref()).collect();
    if labels != [Some("x"), Some("y"), Some("z")] {
        return Err(format!("expected args in order [x, y, z], got labels: {labels:?}").into());
    }
    Ok(())
}

/// DP-8: a call to a function declared *later* in the same module
/// (forward reference) gets its missing default substituted by
/// `ResolveReferencesPass`. The lowerer's DP-2 substitution would
/// skip if `function_id` were None at lowering, but the registration
/// pass means `function_id` is normally already bound. Validate that
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
    let call_args =
        function_call_args(body).ok_or_else(|| format!("unexpected body shape: {body:?}"))?;
    if call_args.len() != 2 {
        return Err(format!(
            "expected 2 args after forward-ref default substitution, got {}",
            call_args.len()
        )
        .into());
    }
    Ok(())
}
