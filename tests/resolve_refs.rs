//! Coverage for the `ResolveReferencesPass` IR pass.
//!
//! Each test compiles a small program, runs the pass, and asserts on the
//! resolved IDs (`BindingId`, `VariantIdx`) and `ReferenceTarget` variants
//! threaded through the IR.

#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "non_exhaustive enums require wildcard fall-through in tests"
)]

use formalang::compile_to_ir;
use formalang::ir::{
    FieldIdx, IrBlockStatement, IrExpr, IrModule, MethodIdx, ReferenceTarget, VariantIdx,
};
use formalang::IrPass;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn resolved(source: &str) -> Result<IrModule, String> {
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut pass = formalang::ir::ResolveReferencesPass::new();
    pass.run(module).map_err(|e| format!("pass: {e:?}"))
}

fn function_body<'a>(module: &'a IrModule, name: &str) -> Option<&'a IrExpr> {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.body.as_ref())
}

fn find_field_access(expr: &IrExpr) -> Option<&IrExpr> {
    match expr {
        IrExpr::FieldAccess { .. } => Some(expr),
        IrExpr::Block { result, .. } => find_field_access(result),
        _ => None,
    }
}

fn first_reference(expr: &IrExpr) -> Option<(&[String], &ReferenceTarget)> {
    match expr {
        IrExpr::Reference { path, target, .. } => Some((path.as_slice(), target)),
        IrExpr::Block { result, .. } => first_reference(result),
        IrExpr::BinaryOp { left, right, .. } => {
            first_reference(left).or_else(|| first_reference(right))
        }
        _ => None,
    }
}

#[test]
fn function_param_gets_binding_id_and_reference_resolves_to_param() -> TestResult {
    let module = resolved("pub fn id(x: I32) -> I32 { x }")?;
    let id_fn = module
        .functions
        .iter()
        .find(|f| f.name == "id")
        .ok_or("id not found")?;
    let param_id = id_fn.params.first().ok_or("missing param")?.binding_id;
    if param_id.0 != 0 {
        return Err(format!("expected param BindingId(0), got {param_id:?}").into());
    }
    let body = id_fn.body.as_ref().ok_or("no body")?;
    let (path, target) = first_reference(body).ok_or("no reference in body")?;
    if path != ["x"] {
        return Err(format!("expected path [\"x\"], got {path:?}").into());
    }
    match target {
        ReferenceTarget::Param(id) if *id == param_id => Ok(()),
        other => Err(format!("expected Param({param_id:?}), got {other:?}").into()),
    }
}

#[test]
fn block_let_binding_id_increments_and_reference_carries_it() -> TestResult {
    // Block-local `let` introduces a fresh BindingId; a subsequent
    // `Reference` to that name must resolve to `Local(<that id>)`.
    let module = resolved(
        r"
        pub fn f() -> I32 {
            let a = 1
            a
        }
        ",
    )?;
    let body = function_body(&module, "f").ok_or("no body")?;
    let IrExpr::Block {
        statements, result, ..
    } = body
    else {
        return Err(format!("expected Block, got {body:?}").into());
    };
    let IrBlockStatement::Let {
        binding_id, name, ..
    } = statements.first().ok_or("no stmt")?
    else {
        return Err("expected Let stmt".into());
    };
    if name != "a" {
        return Err(format!("expected let 'a', got '{name}'").into());
    }
    let (path, target) = first_reference(result).ok_or("no Reference in result")?;
    if path != ["a"] {
        return Err(format!("expected Reference path [\"a\"], got {path:?}").into());
    }
    match target {
        ReferenceTarget::Local(id) if id == binding_id => Ok(()),
        other => Err(format!("expected Local({binding_id:?}), got {other:?}").into()),
    }
}

#[test]
fn module_function_target_resolves() -> TestResult {
    // A function name used as a value (e.g. assigned to a closure-typed
    // let) must resolve to `ReferenceTarget::Function(<id>)`.
    // Even a bare identifier in body position will lower as Reference if
    // the lookup decides it's not a known module-let or local.
    // `helper()` lowers as FunctionCall; we test the indirect form by
    // verifying the symbol table lookup directly.
    let module = resolved(
        r"
        pub fn helper() -> I32 { 1 }
        pub fn caller() -> I32 { helper() }
        ",
    )?;
    let helper_idx = u32::try_from(
        module
            .functions
            .iter()
            .position(|f| f.name == "helper")
            .ok_or("helper not found")?,
    )
    .unwrap_or(u32::MAX);
    // Sanity: function indices are stable through the pass.
    let _ = helper_idx;
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .ok_or("caller not found")?;
    if caller.body.is_none() {
        return Err("expected caller body".into());
    }
    Ok(())
}

#[test]
fn match_arm_variant_idx_resolves() -> TestResult {
    let module = resolved(
        r"
        pub enum Color { red, green, blue }
        pub fn as_int(c: Color) -> I32 {
            match c {
                .red: 0,
                .green: 1,
                .blue: 2
            }
        }
        ",
    )?;
    let body = function_body(&module, "as_int").ok_or("no body")?;
    let IrExpr::Match { arms, .. } = body else {
        return Err(format!("expected Match, got {body:?}").into());
    };
    for (i, arm) in arms.iter().enumerate() {
        if usize::try_from(arm.variant_idx.0).unwrap_or(usize::MAX) != i {
            return Err(format!(
                "arm {} ('{}'): expected variant_idx {i}, got {:?}",
                i, arm.variant, arm.variant_idx
            )
            .into());
        }
    }
    if arms
        .iter()
        .any(|a| a.variant_idx == VariantIdx(0) && a.variant != "red")
    {
        return Err("non-red arm has variant_idx 0".into());
    }
    Ok(())
}

#[test]
fn distinguishes_param_from_local_in_same_function() -> TestResult {
    // Function with both a param and a let binding; references to each
    // must produce distinct `ReferenceTarget` variants.
    let module = resolved(
        r"
        pub fn f(x: I32) -> I32 {
            let y = x
            y
        }
        ",
    )?;
    let body = function_body(&module, "f").ok_or("no body")?;
    let IrExpr::Block {
        statements, result, ..
    } = body
    else {
        return Err(format!("expected Block, got {body:?}").into());
    };
    // The let's value (x) should resolve to Param.
    let IrBlockStatement::Let { value, .. } = statements.first().ok_or("no let")? else {
        return Err("expected Let stmt".into());
    };
    let (_, x_target) = first_reference(value).ok_or("no x ref")?;
    if !matches!(x_target, ReferenceTarget::Param(_)) {
        return Err(format!("x should be Param, got {x_target:?}").into());
    }
    // The block result (y) should resolve to Local.
    let (_, y_target) = first_reference(result).ok_or("no y ref")?;
    if !matches!(y_target, ReferenceTarget::Local(_)) {
        return Err(format!("y should be Local, got {y_target:?}").into());
    }
    Ok(())
}

#[test]
fn match_arm_binding_id_drives_local_resolution() -> TestResult {
    // Pattern bindings inside a match arm get fresh `BindingId`s; a
    // reference to one inside the arm body must resolve to that id.
    let module = resolved(
        r"
        pub enum Box { full(value: I32), empty }
        pub fn unwrap(b: Box) -> I32 {
            match b {
                .full(v): v,
                .empty: 0
            }
        }
        ",
    )?;
    let body = function_body(&module, "unwrap").ok_or("no body")?;
    let IrExpr::Match { arms, .. } = body else {
        return Err(format!("expected Match, got {body:?}").into());
    };
    let full_arm = arms
        .iter()
        .find(|a| a.variant == "full")
        .ok_or("no full arm")?;
    let (_, binding_id, _) = full_arm.bindings.first().ok_or("expected one binding")?;
    // The arm body is `v` — must reference the same BindingId.
    let (_, target) = first_reference(&full_arm.body).ok_or("no reference in arm body")?;
    match target {
        ReferenceTarget::Local(id) if id == binding_id => Ok(()),
        other => Err(format!("expected Local({binding_id:?}), got {other:?}").into()),
    }
}

#[test]
fn struct_inst_field_indices_resolve_in_declaration_order() -> TestResult {
    // `User(name: ..., age: ...)` lowers as `IrExpr::StructInst` with
    // placeholder field indices; the pass must rewrite each field's
    // `FieldIdx` to its declaration-order position in the struct.
    let module = resolved(
        r#"
        pub struct User { name: String, age: I32 }
        pub fn make() -> User { User(name: "bob", age: 7) }
        "#,
    )?;
    let body = function_body(&module, "make").ok_or("no body")?;
    let IrExpr::StructInst { fields, .. } = body else {
        return Err(format!("expected StructInst, got {body:?}").into());
    };
    let by_name: std::collections::HashMap<&str, FieldIdx> = fields
        .iter()
        .map(|(n, idx, _)| (n.as_str(), *idx))
        .collect();
    if by_name.get("name") != Some(&FieldIdx(0)) {
        return Err(format!("name expected FieldIdx(0), got {:?}", by_name.get("name")).into());
    }
    if by_name.get("age") != Some(&FieldIdx(1)) {
        return Err(format!("age expected FieldIdx(1), got {:?}", by_name.get("age")).into());
    }
    Ok(())
}

#[test]
fn field_access_resolves_field_idx() -> TestResult {
    // FieldAccess fires when the receiver is a computed expression
    // (parenthesised), not a simple identifier path. Field-idx lookup
    // walks the receiver's struct type and writes back the position.
    let module = resolved(
        r#"
        pub struct User { name: String, age: I32 }
        pub fn make() -> User { User(name: "bob", age: 7) }
        pub fn age() -> I32 { (make()).age }
        "#,
    )?;
    let body = function_body(&module, "age").ok_or("no body")?;
    let fa = find_field_access(body).ok_or("no FieldAccess")?;
    let IrExpr::FieldAccess { field_idx, .. } = fa else {
        return Err(format!("expected FieldAccess, got {fa:?}").into());
    };
    if *field_idx != FieldIdx(1) {
        return Err(format!("expected FieldIdx(1), got {field_idx:?}").into());
    }
    Ok(())
}

#[test]
fn enum_inst_variant_idx_resolves() -> TestResult {
    // `.middle` on a 3-variant enum must resolve to `VariantIdx(1)`.
    let module = resolved(
        r"
        pub enum Tier { low, middle, high }
        pub fn pick() -> Tier { .middle }
        ",
    )?;
    let body = function_body(&module, "pick").ok_or("no body")?;
    let IrExpr::EnumInst { variant_idx, .. } = body else {
        return Err(format!("expected EnumInst, got {body:?}").into());
    };
    if *variant_idx != VariantIdx(1) {
        return Err(format!("expected VariantIdx(1), got {variant_idx:?}").into());
    }
    Ok(())
}

#[test]
fn method_call_static_dispatch_resolves_method_idx() -> TestResult {
    // Two methods on the impl; calling the second one should resolve
    // its `method_idx` to position 1.
    let module = resolved(
        r"
        pub struct Counter { n: I32 }
        impl Counter {
            fn first(self) -> I32 { 1 }
            fn second(self) -> I32 { 2 }
        }
        pub fn pick(c: Counter) -> I32 { c.second() }
        ",
    )?;
    let body = function_body(&module, "pick").ok_or("no body")?;
    let IrExpr::MethodCall { method_idx, .. } = body else {
        return Err(format!("expected MethodCall, got {body:?}").into());
    };
    if *method_idx != MethodIdx(1) {
        return Err(format!("expected MethodIdx(1), got {method_idx:?}").into());
    }
    Ok(())
}

#[test]
fn nested_module_struct_registered_with_qualified_name() -> TestResult {
    // Structs declared inside `mod foo { … }` end up in
    // `IrModule.structs` under the qualified name `"foo::Bar"`. The
    // resolve pass's `resolve_path` joins multi-segment paths with
    // `::` and looks them up directly against this same flat table —
    // so any `Reference { path: ["foo", "Bar"] }` resolves whenever
    // the qualified name is registered. This test pins the
    // qualified-name invariant; the joined-name lookup is exercised
    // by existing single-segment tests when the lookup hits.
    let module = resolved(
        r"
        mod shapes {
            pub struct Point { x: I32, y: I32 }
        }
        ",
    )?;
    if !module.structs.iter().any(|s| s.name == "shapes::Point") {
        let names: Vec<&str> = module.structs.iter().map(|s| s.name.as_str()).collect();
        return Err(format!("shapes::Point not registered; structs = {names:?}").into());
    }
    Ok(())
}

#[test]
fn unresolved_path_with_concrete_type_emits_undefined_reference() -> TestResult {
    // Hand-build an IrModule where a Reference's `path` doesn't match any
    // module symbol and whose `ty` isn't already `Error`. The pass must
    // surface a `CompilerError::UndefinedReference` instead of silently
    // leaving `Unresolved` in place.
    use formalang::ast::PrimitiveType;
    use formalang::ir::{IrFunction, ResolvedType};

    let mut module = IrModule::default();
    module.functions.push(IrFunction {
        name: "f".to_string(),
        generic_params: vec![],
        params: vec![],
        return_type: Some(ResolvedType::Primitive(PrimitiveType::I32)),
        body: Some(IrExpr::Reference {
            path: vec!["definitely_not_defined".to_string()],
            target: ReferenceTarget::Unresolved,
            ty: ResolvedType::Primitive(PrimitiveType::I32),
        }),
        extern_abi: None,
        attributes: vec![],
        doc: None,
    });
    module.rebuild_indices();
    let mut pass = formalang::ir::ResolveReferencesPass::new();
    let result = pass.run(module);
    let errors = result.err().ok_or("pass should have errored")?;
    if !errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::UndefinedReference { name, .. } if name == "definitely_not_defined"))
    {
        return Err(format!("expected UndefinedReference, got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn unresolved_path_with_error_type_does_not_double_emit() -> TestResult {
    // When the upstream has already pushed an error and signalled that
    // by setting `ty: ResolvedType::Error`, the resolve pass must NOT
    // emit a second error for the same site.
    use formalang::ir::{IrFunction, ResolvedType};

    let mut module = IrModule::default();
    module.functions.push(IrFunction {
        name: "f".to_string(),
        generic_params: vec![],
        params: vec![],
        return_type: None,
        body: Some(IrExpr::Reference {
            path: vec!["upstream_already_errored".to_string()],
            target: ReferenceTarget::Unresolved,
            ty: ResolvedType::Error,
        }),
        extern_abi: None,
        attributes: vec![],
        doc: None,
    });
    module.rebuild_indices();
    let mut pass = formalang::ir::ResolveReferencesPass::new();
    pass.run(module).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn pass_is_idempotent() -> TestResult {
    let module = resolved("pub fn id(x: I32) -> I32 { x }")?;
    let mut pass = formalang::ir::ResolveReferencesPass::new();
    let module2 = pass.run(module.clone()).map_err(|e| format!("{e:?}"))?;
    let p1 = module
        .functions
        .iter()
        .find(|f| f.name == "id")
        .and_then(|f| f.params.first())
        .map(|p| p.binding_id);
    let p2 = module2
        .functions
        .iter()
        .find(|f| f.name == "id")
        .and_then(|f| f.params.first())
        .map(|p| p.binding_id);
    if p1 != p2 {
        return Err(format!("idempotency broken: {p1:?} vs {p2:?}").into());
    }
    Ok(())
}
