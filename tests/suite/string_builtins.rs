//! Integration tests for the v1 String builtin surface (SB-1 through SB-5).
//!
//! Cover:
//! - The prelude is auto-loaded so `extern impl String { ... }` is
//!   visible to every program without explicit `use`.
//! - `s.len()` and the other prelude methods dispatch via
//!   `ImplTarget::Primitive(String)`.
//! - `s[i]` desugars to a `MethodCall` on `byte_at`.
//! - User code can declare `extern impl <Primitive>` blocks for
//!   other primitive types (e.g. `I32`).

use formalang::compile_to_ir;
use formalang::ir::{ImplTarget, IrExpr};

/// True iff `expr` (or any recursively-reachable child) is a `MethodCall`.
fn contains_method_call(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::MethodCall { .. } => true,
        IrExpr::Block { result, .. } => contains_method_call(result),
        IrExpr::If {
            then_branch,
            else_branch,
            ..
        } => {
            contains_method_call(then_branch)
                || else_branch.as_deref().is_some_and(contains_method_call)
        }
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
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::FunctionCall { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. } => false,
    }
}

/// True iff `expr` is a `MethodCall` whose method is `"byte_at"`, or a
/// `Block` whose result is.
fn finds_byte_at(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::MethodCall { method, .. } => method == "byte_at",
        IrExpr::Block { result, .. } => finds_byte_at(result),
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
        | IrExpr::FunctionCall { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. } => false,
    }
}

/// SB-1 + SB-4: The prelude's `extern impl String` is present in
/// every compiled module — `String::len` and friends exist as
/// methods on `ImplTarget::Primitive(PrimitiveType::String)`.
#[test]
fn prelude_extern_impl_string_loaded() -> Result<(), Box<dyn std::error::Error>> {
    let module = compile_to_ir("fn main() -> I32 { 0 }").map_err(|e| format!("{e:?}"))?;
    let prelude_impl = module
        .impls
        .iter()
        .find(|i| matches!(i.target, ImplTarget::Primitive(self::ast_re::PRIM_STRING)))
        .ok_or("no extern impl String found in module")?;
    let methods: Vec<&str> = prelude_impl
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    for required in [
        "len",
        "is_empty",
        "slice",
        "starts_with",
        "contains",
        "byte_at",
    ] {
        if !methods.contains(&required) {
            return Err(
                format!("prelude missing String::{required}; found methods: {methods:?}").into(),
            );
        }
    }
    Ok(())
}

mod ast_re {
    pub(crate) const PRIM_STRING: formalang::ast::PrimitiveType =
        formalang::ast::PrimitiveType::String;
}

/// SB-3 + SB-5: `s.len()` resolves to a method on the prelude impl;
/// `s[i]` desugars to a method call on `byte_at`.
#[test]
fn string_method_call_lowers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn measure(s: String) -> I32 { s.len() }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let measure = module
        .functions
        .iter()
        .find(|f| f.name == "measure")
        .ok_or("measure missing")?;
    let body = measure.body.as_ref().ok_or("measure body missing")?;
    if !contains_method_call(body) {
        return Err(format!("expected MethodCall in measure body, got: {body:?}").into());
    }
    Ok(())
}

/// SB-5: `s[i]` desugars to `s.byte_at(i)` `MethodCall`.
#[test]
fn string_index_desugars_to_byte_at() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn first_byte(s: String) -> I32 { s[0] }
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "first_byte")
        .ok_or("first_byte missing")?;
    let body = func.body.as_ref().ok_or("first_byte body missing")?;
    if !finds_byte_at(body) {
        return Err(format!("expected byte_at MethodCall, got: {body:?}").into());
    }
    Ok(())
}

/// SB-2: `extern impl <Primitive>` declarations for non-String
/// primitives compile and lower to `ImplTarget::Primitive`.
#[test]
fn extern_impl_on_i32() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
extern impl I32 {
  fn abs(self) -> I32
}
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let i32_impl = module
        .impls
        .iter()
        .find(|i| {
            matches!(
                i.target,
                ImplTarget::Primitive(formalang::ast::PrimitiveType::I32)
            )
        })
        .ok_or("expected ImplTarget::Primitive(I32) impl")?;
    let abs_method = i32_impl
        .functions
        .iter()
        .find(|f| f.name == "abs")
        .ok_or("abs method missing on I32 impl")?;
    if !abs_method.is_extern() {
        return Err("abs method should be extern (no body)".into());
    }
    Ok(())
}
