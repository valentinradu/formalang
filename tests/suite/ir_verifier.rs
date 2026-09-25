//! Every module the compiler accepts must keep the IR contract.
//!
//! `crate::common::verifier` checks a lowered `IrModule` against the
//! type contract in `docs/developer/ir/expressions.md`: each id names
//! a definition, each literal fits its type, each instance names a
//! real variant with the right fields, and the operands of each
//! operator agree. These tests run that check over four corpora — the
//! examples, the conformance cases that compile, the value-by-context
//! matrix, and the complete fixture — straight from lowering and after
//! each optimising pass. One test runs one corpus through one stage,
//! so a failure names both.
//!
//! The last group holds single programs that reach one defect each.

#![expect(
    clippy::panic,
    reason = "a module that breaks the contract fails the test with the list of problems"
)]

use crate::common::matrix::{program as matrix_program, CONTEXTS, VALUES};
use crate::common::verifier::{verify, verify_resolved};
use crate::common::{with_a_large_stack, Checked};

use formalang::compile_to_ir;
use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::pipeline::Pipeline;

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Corpora
// ---------------------------------------------------------------------------

fn read_dir_fv(dir: &Path, out: &mut Vec<(String, String)>, root: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            read_dir_fv(&path, out, root);
        } else if path.extension().and_then(|e| e.to_str()) == Some("fv") {
            if let Ok(source) = std::fs::read_to_string(&path) {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                out.push((name, source));
            }
        }
    }
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn examples() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let root = manifest().join("examples");
    read_dir_fv(&root, &mut out, &root);
    out
}

/// The conformance cases that say they compile.
fn conformance() -> Vec<(String, String)> {
    let mut all = Vec::new();
    let root = manifest().join("tests/conformance");
    read_dir_fv(&root, &mut all, &root);
    all.into_iter()
        .filter(|(_, s)| {
            s.lines()
                .find(|l| l.trim_start().starts_with("// expect:"))
                .is_some_and(|l| {
                    let rest = l.trim_start().trim_start_matches("// expect:").trim();
                    rest.starts_with("run") || rest.starts_with("compile")
                })
        })
        .collect()
}

fn matrix() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for value in VALUES {
        for context in CONTEXTS {
            if value.skip.contains(&context.what) {
                continue;
            }
            out.push((
                format!("{} through {}", value.name, context.what),
                matrix_program(value, context),
            ));
        }
    }
    out
}

fn fixture() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let root = manifest().join("tests/fixtures");
    read_dir_fv(&root, &mut out, &root);
    out
}

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

/// A pipeline, and whether it runs `ResolveReferencesPass`.
fn stage(name: &str) -> (Pipeline, bool) {
    match name {
        "lowered" => (Pipeline::new(), false),
        "monomorphise" => (Pipeline::new().pass(MonomorphisePass::default()), false),
        "resolve" => (Pipeline::new().pass(ResolveReferencesPass::new()), true),
        "fold" => (Pipeline::new().pass(ConstantFoldingPass::new()), false),
        "dce" => (
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(DeadCodeEliminationPass::new()),
            true,
        ),
        "closure_conversion" => (
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new()),
            true,
        ),
        "defunctionalise" => (
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new())
                .pass(DefunctionalisePass),
            true,
        ),
        "codegen" => (Pipeline::for_codegen(), true),
        "codegen_then_fold" => (
            Pipeline::for_codegen().pass(ConstantFoldingPass::new()),
            true,
        ),
        other => panic!("no stage called {other}"),
    }
}

/// Compile each program, run it through `stage_name`, and verify it.
/// A program that does not compile is not this test's business; a pass
/// that fails is reported, because the module compiled.
fn sweep(corpus: Vec<(String, String)>, stage_name: &'static str, floor: usize) {
    let failures = with_a_large_stack(move || {
        let mut checked = Checked::new("modules verified", floor);
        let mut failures = Vec::new();
        for (name, source) in corpus {
            let Ok(module) = compile_to_ir(&source) else {
                continue;
            };
            let (mut pipeline, resolved) = stage(stage_name);
            let module: IrModule = match pipeline.run(module) {
                Ok(m) => m,
                Err(errors) => {
                    failures.push(format!(
                        "{name}: the {stage_name} stage failed on an accepted module: {:?}",
                        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
                    ));
                    continue;
                }
            };
            let problems = if resolved {
                verify_resolved(&module)
            } else {
                verify(&module)
            };
            checked.hit();
            if !problems.is_empty() {
                failures.push(format!("{name}:\n    {}", problems.join("\n    ")));
            }
        }
        failures
    });
    assert!(
        failures.is_empty(),
        "{} module(s) break the IR contract after the {stage_name} stage:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

macro_rules! sweeps {
    ($($test:ident: $corpus:ident, $stage:literal, $floor:literal;)*) => {
        $(
            #[test]
            fn $test() {
                sweep($corpus(), $stage, $floor);
            }
        )*
    };
}

sweeps! {
    examples_lowered: examples, "lowered", 15;
    examples_monomorphised: examples, "monomorphise", 15;
    examples_resolved: examples, "resolve", 15;
    examples_folded: examples, "fold", 15;
    examples_after_dce: examples, "dce", 15;
    examples_closure_converted: examples, "closure_conversion", 15;
    examples_defunctionalised: examples, "defunctionalise", 15;
    examples_codegen: examples, "codegen", 15;
    examples_codegen_then_fold: examples, "codegen_then_fold", 15;

    conformance_lowered: conformance, "lowered", 200;
    conformance_monomorphised: conformance, "monomorphise", 200;
    conformance_resolved: conformance, "resolve", 200;
    conformance_folded: conformance, "fold", 200;
    conformance_after_dce: conformance, "dce", 200;
    conformance_closure_converted: conformance, "closure_conversion", 200;
    conformance_defunctionalised: conformance, "defunctionalise", 200;
    conformance_codegen: conformance, "codegen", 200;
    conformance_codegen_then_fold: conformance, "codegen_then_fold", 200;

    matrix_lowered: matrix, "lowered", 200;
    matrix_monomorphised: matrix, "monomorphise", 200;
    matrix_resolved: matrix, "resolve", 200;
    matrix_folded: matrix, "fold", 200;
    matrix_after_dce: matrix, "dce", 200;
    matrix_closure_converted: matrix, "closure_conversion", 200;
    matrix_defunctionalised: matrix, "defunctionalise", 200;
    matrix_codegen: matrix, "codegen", 200;
    matrix_codegen_then_fold: matrix, "codegen_then_fold", 200;

    fixture_lowered: fixture, "lowered", 1;
    fixture_monomorphised: fixture, "monomorphise", 1;
    fixture_resolved: fixture, "resolve", 1;
    fixture_folded: fixture, "fold", 1;
    fixture_codegen: fixture, "codegen", 1;
}

// ---------------------------------------------------------------------------
// One defect each
// ---------------------------------------------------------------------------

/// Compile `source`; it must compile. Run `stage_name`, then verify.
fn verified(source: &str, stage_name: &str) -> Vec<String> {
    let module = match compile_to_ir(source) {
        Ok(m) => m,
        Err(e) => panic!("the program must compile for this check: {e:?}"),
    };
    let (mut pipeline, resolved) = stage(stage_name);
    let module = match pipeline.run(module) {
        Ok(m) => m,
        Err(e) => panic!("the {stage_name} stage failed: {e:?}"),
    };
    if resolved {
        verify_resolved(&module)
    } else {
        verify(&module)
    }
}

/// Compile `source` and require that it is either rejected or lowered
/// to a module that keeps the contract.
fn rejected_or_well_formed(source: &str) {
    let Ok(module) = compile_to_ir(source) else {
        return;
    };
    let problems = verify(&module);
    assert!(
        problems.is_empty(),
        "the compiler accepted a program and lowered it to an ill-formed module:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn folding_keeps_an_i32_sum_inside_32_bits() {
    let problems = verified("pub let x: I32 = 2147483647 + 1\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_an_i32_product_inside_32_bits() {
    let problems = verified("pub let x: I32 = 65536 * 65536\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_an_i32_difference_inside_32_bits() {
    let problems = verified("pub let x: I32 = -2147483647 - 2\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_an_i64_sum_inside_64_bits() {
    let problems = verified("pub let x: I64 = 9223372036854775807I64 + 1I64\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_the_negation_of_i32_min_inside_32_bits() {
    let problems = verified("pub let x: I32 = -(-2147483647 - 1)\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_i32_min_divided_by_minus_one_inside_32_bits() {
    let problems = verified("pub let x: I32 = (-2147483647 - 1) / -1\n", "fold");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn folding_keeps_an_i32_sum_in_a_function_inside_32_bits() {
    let problems = verified(
        "pub fn f() -> I32 { 2000000000 + 2000000000 }\n",
        "codegen_then_fold",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

const SHAPE: &str = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n";

#[test]
fn a_dot_variant_that_does_not_exist_is_not_lowered() {
    rejected_or_well_formed(&format!("{SHAPE}pub fn a() -> Shape {{ .nope }}\n"));
}

#[test]
fn a_dot_variant_with_a_wrong_field_type_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}pub fn b() -> Shape {{ .circle(r: \"x\") }}\n"
    ));
}

#[test]
fn a_dot_variant_with_an_extra_field_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}pub fn c() -> Shape {{ .circle(r: 1, q: 2) }}\n"
    ));
}

#[test]
fn a_dot_variant_with_a_missing_field_is_not_lowered() {
    rejected_or_well_formed(&format!("{SHAPE}pub fn d() -> Shape {{ .rect(w: 1) }}\n"));
}

#[test]
fn a_dot_variant_with_an_i32_for_an_i64_field_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}pub fn e() -> Shape {{ .rect(w: 1, h: 2I32) }}\n"
    ));
}

#[test]
fn a_dot_variant_without_fields_given_a_field_is_not_lowered() {
    rejected_or_well_formed(&format!("{SHAPE}pub fn f() -> Shape {{ .empty(r: 1) }}\n"));
}

#[test]
fn a_match_binding_added_to_a_boolean_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}pub fn a(s: Shape) -> I32 {{ match s {{ .circle(r): r + true, _: 0 }} }}\n"
    ));
}

#[test]
fn a_match_arm_of_type_i64_in_an_i32_function_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}pub fn b(s: Shape) -> I32 {{ match s {{ .rect(w, h): h, _: 0 }} }}\n"
    ));
}

#[test]
fn a_dot_variant_in_a_let_annotation_that_does_not_exist_is_not_lowered() {
    rejected_or_well_formed(&format!("{SHAPE}pub let s: Shape = .nope\n"));
}

#[test]
fn a_dot_variant_as_an_argument_with_a_wrong_field_is_not_lowered() {
    rejected_or_well_formed(&format!(
        "{SHAPE}fn take(s: Shape) -> I32 {{ 0 }}\npub fn g() -> I32 {{ take(s: .circle(r: true)) }}\n"
    ));
}

#[test]
fn a_default_that_reads_an_earlier_default_is_bound_at_the_call() {
    let problems = verified(
        "fn f(a: I32 = 3, b: I32 = a + 1) -> I32 { a * 10 + b }\n\
         pub fn g() -> I32 { f() }\n",
        "lowered",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn resolving_gives_the_none_arm_of_an_optional_its_own_index() {
    let problems = verified(
        "pub fn f(x: I32?) -> I32 { match x { .some(v): v, .none: 0 } }\n",
        "resolve",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn resolving_gives_an_arm_of_a_generic_enum_its_own_index() {
    let problems = verified(
        "pub enum R<T> { ok(v: T), error(e: String) }\n\
         pub fn f(r: R<I32>) -> I32 { match r { .ok(v): v, .error(e): 0 } }\n",
        "resolve",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn resolving_gives_a_field_of_a_generic_struct_its_own_index() {
    let problems = verified(
        "pub struct Pair<A, B> { first: A, second: B }\n\
         pub fn f(p: Pair<I32, I32>) -> I32 { p.second }\n",
        "resolve",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn resolving_reaches_a_struct_instance_in_a_field_default() {
    let problems = verified(
        "pub struct Point { x: I32, y: I32 }\n\
         pub struct Holder { p: Point = Point(y: 2, x: 1) }\n",
        "resolve",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn monomorphising_points_a_struct_instance_at_its_own_specialisation() {
    let problems = verified(
        "pub struct Box<T> { value: T }\n\
         pub fn a() -> I32 { Box(value: 1).value }\n\
         pub fn b() -> Boolean { Box(value: true).value }\n",
        "monomorphise",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_codegen_pipeline_points_a_struct_instance_at_its_own_specialisation() {
    let problems = verified(
        "pub struct Box<T> { value: T }\n\
         pub struct Doc { name: String }\n\
         pub fn a() -> I32 { Box(value: 1).value }\n\
         pub fn b() -> String { Doc(name: \"d\").name }\n\
         pub fn c() -> Boolean { Box(value: true).value }\n",
        "codegen",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_method_call_before_its_generic_impl_names_the_right_impl() {
    let problems = verified(
        "struct B { n: I32 }\n\
         fn first(b: Bx<I32>) -> I32 { b.get() }\n\
         impl B { fn twice(self) -> I32 { self.n * 2 } }\n\
         struct Bx<T> { v: T }\n\
         impl Bx<T> { fn get(self) -> T { self.v } }\n",
        "lowered",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn an_overload_call_carries_the_id_of_the_overload_it_picks() {
    let problems = verified(
        "fn process(v: I32) -> String { \"number\" }\n\
         fn process(v: String) -> String { \"string\" }\n\
         pub fn g() -> String { process(\"a\") }\n",
        "lowered",
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The codegen pipeline must give the same module each time it runs on
/// one input. A backend that caches or diffs its output depends on it.
#[test]
fn the_codegen_pipeline_is_deterministic_on_generic_structs() {
    let source = "pub struct Box<T> { value: T }\n\
                  pub fn a() -> I32 { Box(value: 1).value }\n\
                  pub fn b() -> Boolean { Box(value: true).value }\n\
                  pub fn c() -> F64 { Box(value: 1.5).value }\n\
                  pub fn d() -> String { Box(value: \"s\").value }\n";
    let render = || {
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"));
        let module =
            module.and_then(|m| Pipeline::for_codegen().run(m).map_err(|e| format!("{e:?}")));
        module.and_then(|m| serde_json::to_string(&m).map_err(|e| e.to_string()))
    };
    let first = render();
    for _ in 0..20 {
        assert_eq!(
            render(),
            first,
            "two runs of the codegen pipeline on one source give different modules"
        );
    }
}
