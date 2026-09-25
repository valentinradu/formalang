//! The claims that the public API makes in its documentation.
//!
//! Each test quotes one claim from a doc comment in `src/lib.rs`,
//! `src/pipeline.rs` or a pass module, and checks it.

#![expect(clippy::panic, reason = "a test reports its failure by failing loudly")]

use crate::common::interpreter::{Interpreter, Value};
use crate::common::with_a_large_stack;

use formalang::ir::{
    ClosureConversionPass, DeadCodeEliminationPass, IrModule, MonomorphisePass,
    ResolveReferencesPass,
};
use formalang::pipeline::{Backend, Pipeline};
use formalang::{compile_to_ir, compile_to_ir_with_resolver, FileSystemResolver};

fn compile(source: &str) -> IrModule {
    match compile_to_ir(source) {
        Ok(m) => m,
        Err(errors) => panic!("the program must compile: {errors:?}\n{source}"),
    }
}

fn run(module: IrModule, make: fn() -> Pipeline) -> IrModule {
    match with_a_large_stack(move || make().run(module).map_err(|e| format!("{e:?}"))) {
        Ok(m) => m,
        Err(e) => panic!("the pipeline must accept the program: {e}"),
    }
}

fn json(module: &IrModule) -> serde_json::Value {
    match serde_json::to_value(module) {
        Ok(v) => v,
        Err(e) => panic!("an IrModule must encode as JSON: {e}"),
    }
}

fn answer(module: IrModule) -> String {
    with_a_large_stack(move || match Interpreter::new(&module).run("probe") {
        Ok(Value::Int(n)) => n.to_string(),
        Ok(other) => format!("{other:?}"),
        Err(fault) => format!("a fault: {fault}"),
    })
}

fn dce() -> Pipeline {
    Pipeline::new().pass(DeadCodeEliminationPass::new())
}

// `DeadCodeEliminationPass`: "This module removes code that doesn't
// affect program output: unreachable branches (constant false
// conditions), unused struct definitions, unused let bindings."

#[test]
fn dead_code_elimination_removes_an_unused_module_let() {
    let module = compile("let unused: I32 = 5\npub fn probe() -> I32 { 3 }\n");
    let done = run(module, dce);
    let lets: Vec<String> = done.lets.iter().map(|l| l.name.clone()).collect();
    assert!(
        !lets.iter().any(|n| n == "unused"),
        "the documentation says DCE removes unused let bindings, but `unused` stays: {lets:?}"
    );
}

#[test]
fn dead_code_elimination_removes_an_unused_local_let() {
    let module =
        compile("pub fn probe() -> I32 {\n    let unused = 5\n    let used = 3\n    used\n}\n");
    let done = run(module, dce);
    let text = json(&done).to_string();
    assert!(
        !text.contains("\"name\":\"unused\""),
        "the documentation says DCE removes unused let bindings, but `let unused` stays"
    );
}

#[test]
fn dead_code_elimination_removes_an_unused_struct() {
    let module = compile("struct Unused { data: String }\npub fn probe() -> I32 { 3 }\n");
    let done = run(module, dce);
    assert!(
        done.structs.iter().all(|s| s.name != "Unused"),
        "the documentation says DCE removes unused struct definitions"
    );
}

#[test]
fn dead_code_elimination_removes_a_constant_false_branch() {
    let module = compile("pub fn probe() -> I32 {\n    if false { 1 } else { 3 }\n}\n");
    let done = run(module, dce);
    let text = json(&done).to_string();
    assert!(
        !text.contains("\"If\""),
        "the documentation says DCE removes branches with a constant false condition"
    );
}

/// The `# Example` in the module documentation of `ir::dce` is a
/// `FormaLang` program. An example must compile.
#[test]
fn the_dead_code_elimination_example_compiles() {
    let example = "pub struct Used { value: I32 }\n\
                   struct Unused { data: String }  // removed: nothing refers to it\n\
                   let unused_value: I32 = 5       // removed: private, and nothing reads it\n\
                   pub fn make() -> Used { Used(value: 1) }\n";
    let result = compile_to_ir(example);
    assert!(
        result.is_ok(),
        "the example in the documentation of ir::dce does not compile: {:?}",
        result.err()
    );
}

// `compile_to_ir_with_resolver`: "Single-file consumers should prefer
// `compile_to_ir` — that path skips the pipeline since there are no
// imports to inline." So for a program with no import, the two entry
// points must give programs with the same answer.

const SINGLE_FILE: &[(&str, &str)] = &[
    (
        "a generic identity",
        "fn identity<T>(x: T) -> T { x }\npub fn probe() -> I32 { identity(x: 7) }\n",
    ),
    (
        "a generic function over a generic struct argument",
        "struct Box<T> { value: T }\nfn unbox<T>(b: Box<T>) -> T { b.value }\n\
         pub fn probe() -> I32 { unbox(b: Box(value: 6)) }\n",
    ),
    (
        "a generic swap",
        "struct Pair<A, B> { a: A, b: B }\n\
         fn swap<A, B>(p: Pair<A, B>) -> Pair<B, A> { Pair(a: p.b, b: p.a) }\n\
         pub fn probe() -> I32 { swap(p: Pair(a: \"x\", b: 5)).a }\n",
    ),
    (
        "a nested generic method",
        "struct Box<T> { value: T }\nimpl Box<T> { fn get(self) -> T { self.value } }\n\
         pub fn probe() -> I32 { Box(value: Box(value: 3)).get().get() }\n",
    ),
    (
        "a closure",
        "pub fn probe() -> I32 {\n    let k = 2\n    let f = (n: I32) -> n * k\n    f(21)\n}\n",
    ),
];

#[test]
fn both_entry_points_agree_on_a_single_file() {
    let mut wrong = Vec::new();
    for (what, source) in SINGLE_FILE {
        let plain = answer(compile(source));
        let with_resolver = match compile_to_ir_with_resolver(
            source,
            FileSystemResolver::new(std::path::PathBuf::from(".")),
        ) {
            Ok(m) => answer(m),
            Err(errors) => format!("did not compile: {errors:?}"),
        };
        if plain != with_resolver {
            wrong.push(format!(
                "{what}: compile_to_ir answers {plain}, compile_to_ir_with_resolver answers {with_resolver}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// `IrPass::run`: "Stateful passes that should not carry state between
// runs must reset themselves at the top of `run`." A pipeline that
// runs a second module must give what a new pipeline gives.

#[test]
fn a_reused_pipeline_gives_what_a_new_one_gives() {
    let first = compile("pub fn probe() -> I32 {\n    let f = (n: I32) -> n + 1\n    f(1)\n}\n");
    let second = compile(
        "pub fn probe() -> I32 {\n    let k = 3\n    let g = (n: I32) -> n * k\n    g(2)\n}\n",
    );
    let make = || {
        Pipeline::new()
            .pass(MonomorphisePass::default())
            .pass(ResolveReferencesPass::new())
            .pass(ClosureConversionPass::new())
            .pass(DeadCodeEliminationPass::new())
    };
    let second_copy = second.clone();
    let (after_reuse, fresh) = with_a_large_stack(move || {
        let mut reused = make();
        let _ = reused.run(first);
        let after_reuse = reused.run(second).map_err(|e| format!("{e:?}"));
        let fresh = make().run(second_copy).map_err(|e| format!("{e:?}"));
        (after_reuse, fresh)
    });
    let (Ok(after_reuse), Ok(fresh)) = (after_reuse, fresh) else {
        panic!("both pipelines must accept the program");
    };
    assert_eq!(
        json(&after_reuse),
        json(&fresh),
        "a pipeline that ran once before must give the same module as a new pipeline"
    );
}

/// `Pipeline::emit` runs the passes, then gives the backend the module
/// that they produce.
#[test]
fn emit_gives_the_backend_the_module_after_the_passes() {
    struct Names;
    impl Backend for Names {
        type Output = Vec<String>;
        type Error = std::fmt::Error;
        fn generate(&self, module: &IrModule) -> Result<Vec<String>, std::fmt::Error> {
            Ok(module.structs.iter().map(|s| s.name.clone()).collect())
        }
    }
    let module = compile("struct Unused { data: String }\npub fn probe() -> I32 { 3 }\n");
    let names = with_a_large_stack(move || {
        Pipeline::new()
            .pass(DeadCodeEliminationPass::new())
            .emit(module, &Names)
            .map_err(|e| format!("{e}"))
    });
    match names {
        Ok(names) => assert!(
            !names.iter().any(|n| n == "Unused"),
            "the backend saw a struct that DCE removes: {names:?}"
        ),
        Err(e) => panic!("emit must succeed: {e}"),
    }
}
