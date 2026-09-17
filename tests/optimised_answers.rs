//! An optimising pass must not change what a program answers.
//!
//! `compile_to_ir` lowers and stops. Everything after it —
//! monomorphisation, reference resolution, closure conversion,
//! dead-code elimination, constant folding, defunctionalisation — runs
//! in a pipeline that, until this file existed, **no test in this
//! repository called**. Six passes that rewrite the IR, with no
//! behavioural coverage at all.
//!
//! That was measured rather than assumed. Changing `Sub` to `Add` in
//! the lowerer breaks eight laws in `tests/matrix_answers.rs`; the same
//! change in the constant folder breaks none, because nothing ever ran
//! a folded module.
//!
//! The property here needs no oracle, and it is the one that matters
//! for an optimiser: **run the program, then run the optimised
//! program, and the two answers must agree.** The unoptimised run is
//! the specification. A pass that changes an answer has a defect, and
//! which answer is "right" does not have to be decided to know that.
//!
//! Each pipeline is applied on its own as well as in combination, so a
//! disagreement names the pass that caused it rather than the set.

#[path = "common/mod.rs"]
mod common;

use common::interpreter::{Fault, Interpreter, Value};
use common::matrix::{program as matrix_program, CONTEXTS, VALUES};
use common::{with_a_large_stack, Checked};

use formalang::compile_to_ir;
use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::pipeline::Pipeline;

/// A program with an answer, written to exercise a particular pass.
struct Program {
    what: &'static str,
    source: &'static str,
}

const PROGRAMS: &[Program] = &[
    Program {
        what: "constant arithmetic, for the folder",
        source: "pub fn probe() -> I32 {\n    2 + 3 * 4 - 1\n}\n",
    },
    Program {
        what: "constant arithmetic behind bindings",
        source: "pub fn probe() -> I32 {\n    let a = 2\n    let b = 3\n    a * b + 1\n}\n",
    },
    Program {
        what: "a constant condition",
        source: "pub fn probe() -> I32 {\n    if 1 < 2 { 10 } else { 20 }\n}\n",
    },
    Program {
        what: "a constant condition the other way",
        source: "pub fn probe() -> I32 {\n    if 2 < 1 { 10 } else { 20 }\n}\n",
    },
    Program {
        what: "a generic function, for monomorphisation",
        source: "fn identity<T>(item: T) -> T {\n    item\n}\n\n\
                 pub fn probe() -> I32 {\n    identity(item: 7) + identity(item: 1)\n}\n",
    },
    Program {
        what: "a generic struct, for monomorphisation",
        source: "struct Box<T> {\n    value: T\n}\n\n\
                 pub fn probe() -> I32 {\n    Box(value: 9).value\n}\n",
    },
    Program {
        what: "a generic used at two types",
        source: "fn first<T>(a: T, b: T) -> T {\n    a\n}\n\n\
                 pub fn probe() -> I32 {\n    let n = first(a: 4, b: 5)\n    \
                 let s = first(a: \"x\", b: \"y\")\n    if s == \"x\" { n } else { 0 }\n}\n",
    },
    Program {
        what: "a closure, for closure conversion",
        source: "pub fn probe() -> I32 {\n    let double = (n: I32) -> n * 2\n    double(11)\n}\n",
    },
    Program {
        what: "a closure that captures, for closure conversion",
        source: "pub fn probe() -> I32 {\n    let held = 5\n    \
                 let add = (n: I32) -> n + held\n    add(17)\n}\n",
    },
    Program {
        what: "a closure passed as an argument",
        source: "fn apply(f: (I32) -> I32, x: I32) -> I32 {\n    f(x)\n}\n\n\
                 pub fn probe() -> I32 {\n    apply(f: (n: I32) -> n + 1, x: 21)\n}\n",
    },
    Program {
        what: "a closure returned from a function",
        source: "fn make() -> (I32) -> I32 {\n    (n: I32) -> n * 3\n}\n\n\
                 pub fn probe() -> I32 {\n    make()(7)\n}\n",
    },
    Program {
        what: "an unused definition, for dead-code elimination",
        source: "fn never_called() -> I32 {\n    999\n}\n\n\
                 struct Unused {\n    x: I32\n}\n\n\
                 pub fn probe() -> I32 {\n    22\n}\n",
    },
    Program {
        what: "a struct whose impl is reached",
        source: "struct Counter {\n    value: I32\n}\n\n\
                 impl Counter {\n    fn doubled(self) -> I32 { self.value * 2 }\n}\n\n\
                 pub fn probe() -> I32 {\n    Counter(value: 12).doubled()\n}\n",
    },
    Program {
        what: "a trait and its impl, for reference resolution",
        source: "trait Shape {\n    fn area(self) -> I32\n}\n\n\
                 struct Square {\n    side: I32\n}\n\n\
                 impl Shape for Square {\n    fn area(self) -> I32 { self.side * self.side }\n}\n\n\
                 fn total<T: Shape>(item: T) -> I32 {\n    item.area()\n}\n\n\
                 pub fn probe() -> I32 {\n    total(item: Square(side: 5))\n}\n",
    },
    Program {
        what: "an enum matched to a value",
        source: "enum Colour {\n    red,\n    green\n}\n\n\
                 pub fn probe() -> I32 {\n    match Colour.green {\n        \
                 .red: 1,\n        .green: 2\n    }\n}\n",
    },
    Program {
        what: "an enum with a payload",
        source: "enum Load {\n    idle,\n    busy(amount: I32)\n}\n\n\
                 pub fn probe() -> I32 {\n    match Load.busy(amount: 8) {\n        \
                 .idle: 0,\n        .busy(amount): amount\n    }\n}\n",
    },
    Program {
        what: "a loop folded to a value",
        source: "pub fn probe() -> I32 {\n    \
                 for i in 0..5 { i }.fold(initial: 0, f: (a, b) -> a + b)\n}\n",
    },
    Program {
        what: "a loop with a closure combinator",
        source: "pub fn probe() -> I32 {\n    \
                 for i in 0..5 { i }.map(f: (x) -> x * 2).count()\n}\n",
    },
    Program {
        what: "an array and an index",
        source: "pub fn probe() -> I32 {\n    let xs = [3, 6, 9]\n    \
                 if let v = xs[2] { v } else { 0 }\n}\n",
    },
    Program {
        what: "a module-qualified call",
        source: "pub mod m {\n    pub fn helper(a: I32) -> I32 { a + 1 }\n}\n\n\
                 pub fn probe() -> I32 {\n    m::helper(a: 6)\n}\n",
    },
    Program {
        what: "a module-qualified call beside a top-level one of the same name",
        source: "pub mod m {\n    pub fn helper(a: I32) -> I32 { a + 1 }\n}\n\n\
                 fn helper(a: I32) -> I32 { a + 100 }\n\n\
                 pub fn probe() -> I32 {\n    m::helper(a: 6) + helper(a: 0)\n}\n",
    },
    Program {
        what: "a generic function inside a module",
        source: "pub mod m {\n    pub fn identity<T>(item: T) -> T { item }\n}\n\n\
                 pub fn probe() -> I32 {\n    m::identity(item: 7)\n}\n",
    },
    Program {
        what: "a nested generic",
        source: "struct Box<T> {\n    value: T\n}\n\n\
                 pub fn probe() -> I32 {\n    Box(value: Box(value: 14)).value.value\n}\n",
    },
];

/// The pipelines each program is run through.
///
/// Each single pass appears on its own so a disagreement names one
/// pass, and the whole codegen pipeline appears so combinations are
/// covered too.
fn pipelines() -> Vec<(&'static str, Pipeline)> {
    vec![
        (
            "MonomorphisePass",
            Pipeline::new().pass(MonomorphisePass::default()),
        ),
        (
            "ResolveReferencesPass",
            Pipeline::new().pass(ResolveReferencesPass::new()),
        ),
        (
            "ConstantFoldingPass",
            Pipeline::new().pass(ConstantFoldingPass::new()),
        ),
        (
            "DeadCodeEliminationPass",
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(DeadCodeEliminationPass::new()),
        ),
        (
            "ClosureConversionPass",
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new()),
        ),
        (
            "DefunctionalisePass",
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new())
                .pass(DefunctionalisePass),
        ),
        ("the whole codegen pipeline", Pipeline::for_codegen()),
        (
            "the codegen pipeline then folding",
            Pipeline::for_codegen().pass(ConstantFoldingPass::new()),
        ),
    ]
}

/// Every program these tests run: the hand-written ones above, which
/// aim at one pass each, and every cell of the value-by-context matrix
/// that `tests/execution_matrix.rs` also uses.
///
/// The matrix cells are the reason this is not a short list. A pass
/// rewrites whole shapes — a struct field, a closure capture, a match
/// arm — and a handful of hand-written programs cannot reach them all.
/// The matrix already enumerates those shapes, so the passes get the
/// same surface the lowerer does.
fn every_program() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = PROGRAMS
        .iter()
        .map(|p| (p.what.to_string(), p.source.to_string()))
        .collect();

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

/// Run `probe` and describe what came back.
fn answer(module: &IrModule) -> Result<String, String> {
    let mut interpreter = Interpreter::new(module);
    match interpreter.run("probe") {
        Ok(Value::Int(n)) => Ok(n.to_string()),
        Ok(other) => Ok(format!("{other:?}")),
        Err(Fault::AssertFailed) => Err("an assert failed".to_string()),
        Err(other) => Err(format!("{other:?}")),
    }
}

#[test]
fn no_pass_changes_the_answer() {
    with_a_large_stack(|| {
        let mut checked = Checked::new("optimised runs", 1500);
        let mut disagreements = Vec::new();
        let mut broken = Vec::new();

        for (what, source) in every_program() {
            let module = match compile_to_ir(&source) {
                Ok(m) => m,
                Err(errors) => {
                    broken.push(format!("{what}: did not compile: {errors:?}"));
                    continue;
                }
            };

            // The unoptimised run is the specification.
            let expected = match answer(&module) {
                Ok(value) => value,
                Err(why) => {
                    broken.push(format!("{what}: did not run before any pass: {why}"));
                    continue;
                }
            };

            for (name, mut pipeline) in pipelines() {
                let optimised = match pipeline.run(module.clone()) {
                    Ok(m) => m,
                    Err(errors) => {
                        broken.push(format!("{what}: {name} failed: {errors:?}"));
                        checked.hit();
                        continue;
                    }
                };
                match answer(&optimised) {
                    Ok(got) if got == expected => {}
                    Ok(got) => disagreements.push(format!(
                        "{what}: {expected} before any pass, {got} after {name}"
                    )),
                    Err(why) => disagreements.push(format!(
                        "{what}: {expected} before any pass, but after {name} it {why}"
                    )),
                }
                checked.hit();
            }
        }

        assert!(
            broken.is_empty(),
            "{} program(s) or pass(es) failed outright:\n  {}",
            broken.len(),
            broken.join("\n  ")
        );
        assert!(
            disagreements.is_empty(),
            "{} case(s) where an optimising pass changed the answer. The \
         unoptimised run is the specification, so each of these is a \
         defect in the named pass:\n  {}",
            disagreements.len(),
            disagreements.join("\n  ")
        );
    });
}

/// Running a pipeline twice must give the same module as running it
/// once.
///
/// An optimiser is expected to reach a fixed point. A pass that keeps
/// changing its own output either never settles or is not
/// deterministic, and both make a build unreproducible.
#[test]
fn a_pass_settles() {
    with_a_large_stack(|| {
        let mut checked = Checked::new("settling checks", 200);
        let mut unsettled = Vec::new();

        for (what, source) in every_program() {
            let Ok(module) = compile_to_ir(&source) else {
                continue;
            };
            let Ok(once) = Pipeline::for_codegen().run(module) else {
                continue;
            };
            let Ok(twice) = Pipeline::for_codegen().run(once.clone()) else {
                unsettled.push(format!("{what}: the second run failed"));
                checked.hit();
                continue;
            };

            let before = answer(&once);
            let after = answer(&twice);
            if before != after {
                unsettled.push(format!(
                    "{what}: {before:?} after one run, {after:?} after two"
                ));
            }
            checked.hit();
        }

        assert!(
            unsettled.is_empty(),
            "{} program(s) did not settle:\n  {}",
            unsettled.len(),
            unsettled.join("\n  ")
        );
    });
}
