//! Tests for `ClosureConversionPass` (PR 2).
//!
//! Each microcommit adds a snapshot test for the new behaviour it
//! introduces; together they document the cumulative shape of the
//! converted IR.

#![allow(clippy::expect_used)]

use formalang::ir::{ClosureConversionPass, DeadCodeEliminationPass, IrExpr};
use formalang::{compile_to_ir, IrPass, Pipeline};

/// Standard two-closure fixture used by every microcommit's snapshot
/// test, kept in one place so each snapshot exercises the same input.
const TWO_CLOSURES_SOURCE: &str = r"
    pub fn make_adder(sink n: I32) -> (I32) -> I32 {
        |x: I32| x + n
    }

    let format_tag: String -> String = |t: String| t
";

/// mc3 — synthesizes one capture-environment struct per closure.
///
/// `make_adder` returns a closure that captures `n`; `format_tag` is
/// a module-level closure with no captures. After the pass, two
/// `__ClosureEnv<N>` structs should exist in the module.
#[test]
fn mc3_synthesizes_capture_env_structs() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_struct_count = module.structs.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let new_structs: Vec<_> = converted
        .structs
        .iter()
        .skip(original_struct_count)
        .collect();
    insta::assert_debug_snapshot!("mc3_capture_env_structs", new_structs);
}

/// mc4 — synthesizes one lifted function per closure, prepended with
/// an `__env` parameter that points at the corresponding env struct.
///
/// The lifted function's body is the closure body verbatim; mc5
/// rewrites captured-name reads to load from `__env` fields.
#[test]
fn mc4_synthesizes_lifted_functions() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let lifted: Vec<_> = converted
        .functions
        .iter()
        .skip(original_function_count)
        .collect();
    insta::assert_debug_snapshot!("mc4_lifted_functions", lifted);
}

/// mc5 — captured-name reads inside lifted bodies become field
/// accesses on `__env`, while parameter-name reads stay as raw
/// references.
///
/// In the `make_adder` closure body `x + n`, `x` is a parameter
/// (untouched) but `n` is a capture and must become
/// `__env.n`. The trivial `format_tag` body `t` references only its
/// own param.
#[test]
fn mc5_rewrites_captured_refs_to_env_field_access() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let lifted_bodies: Vec<_> = converted
        .functions
        .iter()
        .skip(original_function_count)
        .map(|f| f.body.as_ref())
        .collect();
    insta::assert_debug_snapshot!("mc5_lifted_bodies", lifted_bodies);
}

/// mc6 — `IrExpr::Closure` is replaced by `IrExpr::ClosureRef` at
/// every site, with `env_struct` constructing the matching env via
/// `StructInst` whose field values reference the captured names in
/// the outer scope.
#[test]
fn mc6_replaces_closure_with_closure_ref_at_sites() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    // `make_adder`'s body should now be a ClosureRef pointing at
    // __closure0 with an env constructor for __ClosureEnv0(n: <ref to make_adder's n param>).
    let make_adder_body = converted
        .functions
        .iter()
        .find(|f| f.name == "make_adder")
        .and_then(|f| f.body.as_ref())
        .expect("make_adder body");

    // `format_tag` is a module-level let; its value should be a
    // ClosureRef pointing at __closure1 with an empty env.
    let format_tag_value = &converted
        .lets
        .iter()
        .find(|l| l.name == "format_tag")
        .expect("format_tag let")
        .value;

    insta::assert_debug_snapshot!(
        "mc6_site_rewrites",
        (make_adder_body, format_tag_value)
    );
}

/// mc6 — sanity: after the pass, no `IrExpr::Closure` remains in the
/// module. The mc8 invariant inside the pass enforces the same
/// property; this test additionally verifies the *converted* module
/// is closure-free as observed from outside the pass.
#[test]
fn mc6_no_residual_closure_nodes() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    assert_module_closure_free(&converted);
}

/// mc10 — end-to-end check that a closure-rich, multi-feature
/// program survives `ClosureConversionPass` + `DeadCodeEliminationPass`.
///
/// The fixture exercises every interesting capture pattern the IR
/// produces today:
/// - a closure with no captures (in a `let`),
/// - a closure capturing a `Sink`-mode parameter (returned from a
///   function),
/// - a closure capturing module-level `Let` and `Mut` bindings.
///
/// After the pipeline:
/// - the module is closure-free (mc8 invariant transitively
///   verified),
/// - one synthesized `__closure<N>` lifted function exists per
///   closure that DCE retained, paired with a `__ClosureEnv<N>`
///   env struct,
/// - every surviving `ClosureRef.funcref` resolves to a real
///   top-level function in the module.
#[test]
fn mc10_closure_rich_fixture_survives_pipeline() {
    let source = r"
        // No-capture closure.
        let trivial: I32 -> I32 = |x: I32| x

        // Sink-capture closure (returns a closure capturing the param).
        pub fn make_adder(sink n: I32) -> (I32) -> I32 {
            |x: I32| x + n
        }

        // Module-level Let + Mut captures.
        let base: I32 = 10
        let mut counter: I32 = 0
        let bump: () -> I32 = () -> base + counter
    ";
    let module = compile_to_ir(source).expect("fixture should compile to IR");

    let closure_count_before = count_closure_exprs(&module);
    assert!(
        closure_count_before >= 3,
        "fixture should contain at least 3 closure expressions, got {closure_count_before}"
    );

    let mut pipeline = Pipeline::new()
        .pass(ClosureConversionPass::new())
        .pass(DeadCodeEliminationPass::new());
    let converted = pipeline
        .run(module)
        .expect("ClosureConv + DCE pipeline should succeed");

    assert_module_closure_free(&converted);

    // Every surviving ClosureRef's funcref names an extant function
    // in the module.
    let mut surviving_closure_refs = 0usize;
    for l in &converted.lets {
        if let IrExpr::ClosureRef { funcref, .. } = &l.value {
            surviving_closure_refs = surviving_closure_refs.saturating_add(1);
            let name = funcref.first().expect("non-empty funcref path");
            assert!(
                converted.functions.iter().any(|f| &f.name == name),
                "ClosureRef funcref `{name}` should name an existing function"
            );
        }
    }
    assert!(
        surviving_closure_refs > 0,
        "at least one ClosureRef should survive DCE"
    );
}

fn count_closure_exprs(module: &formalang::ir::IrModule) -> usize {
    let mut count = 0usize;
    let mut visit = |e: &IrExpr| {
        if matches!(e, IrExpr::Closure { .. }) {
            count = count.saturating_add(1);
        }
    };
    walk_module_exprs(module, &mut visit);
    count
}

fn walk_module_exprs(
    module: &formalang::ir::IrModule,
    visit: &mut dyn FnMut(&IrExpr),
) {
    for l in &module.lets {
        visit_all(&l.value, visit);
    }
    for f in &module.functions {
        if let Some(body) = &f.body {
            visit_all(body, visit);
        }
    }
    for i in &module.impls {
        for f in &i.functions {
            if let Some(body) = &f.body {
                visit_all(body, visit);
            }
        }
    }
    for s in &module.structs {
        for field in &s.fields {
            if let Some(d) = &field.default {
                visit_all(d, visit);
            }
        }
    }
}

fn visit_all(e: &IrExpr, visit: &mut dyn FnMut(&IrExpr)) {
    visit(e);
    walk_sub_exprs(e, &mut |sub| visit_all(sub, visit));
}

fn assert_module_closure_free(module: &formalang::ir::IrModule) {
    for f in &module.functions {
        if let Some(body) = &f.body {
            assert_no_closure(body).expect("function body");
        }
    }
    for l in &module.lets {
        assert_no_closure(&l.value).expect("let value");
    }
    for i in &module.impls {
        for f in &i.functions {
            if let Some(body) = &f.body {
                assert_no_closure(body).expect("impl method body");
            }
        }
    }
    for s in &module.structs {
        for field in &s.fields {
            if let Some(d) = &field.default {
                assert_no_closure(d).expect("struct field default");
            }
        }
    }
}

fn assert_no_closure(e: &formalang::ir::IrExpr) -> Result<(), String> {
    use formalang::ir::IrExpr;

    if matches!(e, IrExpr::Closure { .. }) {
        return Err(format!("residual Closure: {e:?}"));
    }
    let mut result: Result<(), String> = Ok(());
    walk_sub_exprs(e, &mut |sub| {
        if result.is_ok() {
            result = assert_no_closure(sub);
        }
    });
    result
}

fn walk_sub_exprs(e: &formalang::ir::IrExpr, visit: &mut dyn FnMut(&formalang::ir::IrExpr)) {
    use formalang::ir::{IrBlockStatement, IrExpr};

    match e {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, v) in fields {
                visit(v);
            }
        }
        IrExpr::Array { elements, .. } => {
            for v in elements {
                visit(v);
            }
        }
        IrExpr::FieldAccess { object, .. } => visit(object),
        IrExpr::BinaryOp { left, right, .. } => {
            visit(left);
            visit(right);
        }
        IrExpr::UnaryOp { operand, .. } => visit(operand),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit(condition);
            visit(then_branch);
            if let Some(e) = else_branch {
                visit(e);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            visit(collection);
            visit(body);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            visit(scrutinee);
            for arm in arms {
                visit(&arm.body);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, v) in args {
                visit(v);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            visit(receiver);
            for (_, v) in args {
                visit(v);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                visit(k);
                visit(v);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            visit(dict);
            visit(key);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => visit(value),
                    IrBlockStatement::Assign { target, value } => {
                        visit(target);
                        visit(value);
                    }
                    IrBlockStatement::Expr(e) => visit(e),
                }
            }
            visit(result);
        }
        IrExpr::Closure { body, .. } => visit(body),
        IrExpr::ClosureRef { env_struct, .. } => visit(env_struct),
    }
}

/// mc9 — capture conventions are preserved on env-struct fields.
///
/// `Sink` captures (a closure returned from a `sink` parameter
/// scope) land with `convention: Sink`; `Mut` captures (a closure
/// over a module-level `let mut` binding) land with
/// `convention: Mut` *and* `mutable: true`, so convention-blind
/// backends still get the borrow hint.
///
/// (Module-level `let mut` is the canonical source of `Mut`
/// captures — capturing a `mut` *parameter* is rejected by semantic
/// analysis because the closure would outlive the local frame.)
#[test]
fn mc9_env_field_convention_preserves_sink_and_mut() {
    let source = r"
        pub fn make_sink_adder(sink n: I32) -> (I32) -> I32 {
            |x: I32| x + n
        }

        let mut counter: I32 = 0
        let bump: () -> I32 = () -> counter
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    let original_struct_count = module.structs.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let env_structs: Vec<_> = converted
        .structs
        .iter()
        .skip(original_struct_count)
        .collect();
    insta::assert_debug_snapshot!("mc9_env_field_conventions", env_structs);
}

/// mc5 — `let` shadowing: an inner `let` binding with the same name
/// as a capture must mask the env access for references *after* the
/// `let`.
#[test]
fn mc5_let_shadowing_blocks_env_rewrite() {
    let source = r"
        pub fn make(sink n: I32) -> (I32) -> I32 {
            |x: I32| (
                let n: I32 = 100
                in x + n
            )
        }
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    // The lifted closure's body is a Block whose result `x + n` reads
    // a *local* `n`. The shadowing rule should leave that read as a
    // raw `Reference { path: ["n"] }`, not rewrite it to `__env.n`.
    let lifted_body = converted
        .functions
        .iter()
        .skip(original_function_count)
        .find_map(|f| f.body.as_ref())
        .expect("at least one lifted closure body");
    insta::assert_debug_snapshot!("mc5_let_shadowing", lifted_body);
}
