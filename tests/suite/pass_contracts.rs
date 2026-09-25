//! The post-conditions that the IR passes state in their documentation.
//!
//! Each test reads one claim from the doc comment of a pass, or one
//! rule that any pass must keep, and checks it on the module that the
//! pass gives. The checks read the module as JSON, so they see every
//! node and do not depend on the shape of the Rust types.

#![expect(clippy::panic, reason = "a test reports its failure by failing loudly")]

use crate::common::with_a_large_stack;

use formalang::compile_to_ir;
use formalang::ir::{
    ClosureConversionPass, DeadCodeEliminationPass, DefunctionalisePass, IrModule,
    MonomorphisePass, ResolveReferencesPass,
};
use formalang::pipeline::Pipeline;

use serde_json::Value as Json;

/// Programs with generics, closures and user names that the passes
/// also generate. Each one compiles.
const PROGRAMS: &[(&str, &str)] = &[
    (
        "a generic function over a generic struct argument",
        "struct Box<T> { value: T }\n\
         fn unbox<T>(b: Box<T>) -> T { b.value }\n\
         pub fn probe() -> I32 { unbox(b: Box(value: 6)) }\n",
    ),
    (
        "a generic swap of a pair",
        "struct Pair<A, B> { a: A, b: B }\n\
         fn swap<A, B>(p: Pair<A, B>) -> Pair<B, A> { Pair(a: p.b, b: p.a) }\n\
         pub fn probe() -> I32 { swap(p: Pair(a: \"x\", b: 5)).a }\n",
    ),
    (
        "a generic identity",
        "fn identity<T>(x: T) -> T { x }\n\
         pub fn probe() -> I32 { identity(x: 7) + identity(x: 1) }\n",
    ),
    (
        "a generic struct built by a generic function",
        "struct Box<T> { value: T }\n\
         fn wrap<T>(x: T) -> Box<T> { Box(value: x) }\n\
         pub fn probe() -> I32 { wrap(x: 11).value }\n",
    ),
    (
        "a closure that reads self",
        "struct Counter { v: I32 }\n\
         impl Counter { fn adder(self) -> (I32) -> I32 { (n: I32) -> n + self.v } }\n\
         pub fn probe() -> I32 {\n    let add = Counter(v: 8).adder()\n    add(2)\n}\n",
    ),
    (
        "a user function named like the generated dispatcher",
        "fn __call_Fn0(x: I32) -> I32 { x + 1000 }\n\
         pub fn probe() -> I32 {\n    let f = (n: I32) -> n + 1\n    f(1) + __call_Fn0(x: 0)\n}\n",
    ),
    (
        "user definitions named like the generated closure parts",
        "struct __ClosureEnv0 { v: I32 }\n\
         fn __closure0(x: I32) -> I32 { x * 100 }\n\
         pub fn probe() -> I32 {\n    let k = 2\n    let f = (n: I32) -> n + k\n    \
         f(1) + __closure0(x: 1) + __ClosureEnv0(v: 5).v\n}\n",
    ),
    (
        "closures in an array",
        "pub fn probe() -> I32 {\n    let fs = [(n: I32) -> n + 1, (n: I32) -> n * 2]\n    \
         if let g = fs[1] { g(21) } else { 0 }\n}\n",
    ),
    (
        "a trait bound",
        "trait Shape { fn area(self) -> I32 }\n\
         struct Sq { s: I32 }\n\
         impl Shape for Sq { fn area(self) -> I32 { self.s * self.s } }\n\
         fn total<T: Shape>(x: T) -> I32 { x.area() }\n\
         pub fn probe() -> I32 { total(x: Sq(s: 3)) }\n",
    ),
];

/// Compile one program of [`PROGRAMS`].
fn compile(what: &str, source: &str) -> IrModule {
    match compile_to_ir(source) {
        Ok(m) => m,
        Err(errors) => panic!("{what}: the program must compile: {errors:?}"),
    }
}

/// Run a pipeline, on a thread with a large stack.
fn run(module: IrModule, make: fn() -> Pipeline) -> Result<IrModule, String> {
    with_a_large_stack(move || make().run(module).map_err(|e| format!("{e:?}")))
}

fn json(module: &IrModule) -> Json {
    match serde_json::to_value(module) {
        Ok(v) => v,
        Err(e) => panic!("an IrModule must encode as JSON: {e}"),
    }
}

/// Call `visit` on every JSON object in `value`, with the key that
/// holds it.
fn walk<'a>(value: &'a Json, key: &str, visit: &mut impl FnMut(&str, &'a Json)) {
    match value {
        Json::Object(map) => {
            visit(key, value);
            for (k, v) in map {
                walk(v, k, visit);
            }
        }
        Json::Array(items) => {
            for item in items {
                walk(item, key, visit);
            }
        }
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => {}
    }
}

fn names(module: &Json, list: &str) -> Vec<String> {
    module[list]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn mono() -> Pipeline {
    Pipeline::new().pass(MonomorphisePass::default())
}

fn up_to_closure_conversion() -> Pipeline {
    mono()
        .pass(ResolveReferencesPass::new())
        .pass(ClosureConversionPass::new())
}

fn up_to_defunctionalise() -> Pipeline {
    up_to_closure_conversion().pass(DefunctionalisePass)
}

fn up_to_dce() -> Pipeline {
    up_to_defunctionalise().pass(DeadCodeEliminationPass::new())
}

/// A pipeline with its name.
type Named = (&'static str, fn() -> Pipeline);

/// The pipelines that the structural checks below run over.
const PIPELINES: &[Named] = &[
    ("MonomorphisePass", mono),
    ("up to ClosureConversionPass", up_to_closure_conversion),
    ("up to DefunctionalisePass", up_to_defunctionalise),
    ("up to DeadCodeEliminationPass", up_to_dce),
    ("Pipeline::for_codegen", Pipeline::for_codegen),
];

/// `MonomorphisePass` says that after it runs, the only `Generic`
/// types left are the five built-in carriers of the prelude: `Array`,
/// `Seq`, `Dictionary`, `Range` and `Optional`.
#[test]
fn monomorphisation_leaves_no_generic_type() {
    let module = compile(
        "a generic struct",
        "pub struct Box<T> { value: T }\npub let b: Box<I32> = Box(value: 1)\n",
    );
    let Ok(done) = run(module, mono) else {
        panic!("MonomorphisePass must accept the example from its own documentation");
    };
    let module = json(&done);
    let structs = names(&module, "structs");
    let enums = names(&module, "enums");
    let carrier = |base: &Json| {
        let named = |list: &[String], id: &Json| {
            id.as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| list.get(i))
                .cloned()
                .unwrap_or_default()
        };
        let name = match (base.get("Struct"), base.get("Enum")) {
            (Some(id), _) => named(&structs, id),
            (_, Some(id)) => named(&enums, id),
            _ => String::new(),
        };
        matches!(
            name.as_str(),
            "Array" | "Seq" | "Dictionary" | "Range" | "Optional"
        )
    };
    let mut left = Vec::new();
    walk(&module, "", &mut |key, node| {
        if key == "Generic" && !carrier(&node["base"]) {
            left.push(node.to_string());
        }
    });
    assert!(
        left.is_empty(),
        "the documentation of MonomorphisePass promises that only the built-in \
         carriers stay Generic, but {} other Generic types remain: {left:?}",
        left.len()
    );
}

/// A call names its callee two ways: by `path` and by `function_id`.
/// After any pass, the two must name the same function, and that
/// function must exist. A backend that dispatches on the id calls the
/// function at that index.
#[test]
fn a_call_id_names_the_function_that_its_path_names() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        for (pipeline, make) in PIPELINES {
            let Ok(done) = run(compile(what, source), *make) else {
                continue;
            };
            let module = json(&done);
            let functions = names(&module, "functions");
            walk(&module, "", &mut |key, node| {
                if key != "FunctionCall" {
                    return;
                }
                let path: Vec<&str> = node["path"]
                    .as_array()
                    .map(|p| p.iter().filter_map(Json::as_str).collect())
                    .unwrap_or_default();
                let Some(last) = path.last() else { return };
                let Some(id) = node["function_id"].as_u64() else {
                    return;
                };
                let callee = usize::try_from(id).ok().and_then(|i| functions.get(i));
                match callee {
                    Some(name) if name == last || *name == path.join("::") => {}
                    Some(name) => wrong.push(format!(
                        "{what}, after {pipeline}: a call to `{}` has the id of `{name}`",
                        path.join("::")
                    )),
                    None => wrong.push(format!(
                        "{what}, after {pipeline}: a call to `{}` has the id {id}, \
                         and no function has it",
                        path.join("::")
                    )),
                }
            });
        }
    }
    assert!(
        wrong.is_empty(),
        "{} call(s) name one function by path and another by id:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// After any pass, each call must name a function that is still in the
/// module. A pass that removes a function must not leave a call to it.
#[test]
fn no_pass_removes_a_function_that_a_call_still_names() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        for (pipeline, make) in PIPELINES {
            let Ok(done) = run(compile(what, source), *make) else {
                continue;
            };
            let module = json(&done);
            let functions = names(&module, "functions");
            walk(&module, "", &mut |key, node| {
                if key != "FunctionCall" {
                    return;
                }
                let path: Vec<&str> = node["path"]
                    .as_array()
                    .map(|p| p.iter().filter_map(Json::as_str).collect())
                    .unwrap_or_default();
                let full = path.join("::");
                let Some(last) = path.last() else { return };
                if !functions.iter().any(|f| f == last || *f == full) {
                    wrong.push(format!("{what}, after {pipeline}: `{full}` is gone"));
                }
            });
        }
    }
    assert!(
        wrong.is_empty(),
        "{} call(s) name a function that a pass removed:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// Two definitions of one kind must not share a name after any pass.
/// A backend emits each definition under its name, so two of one name
/// are one symbol defined twice.
#[test]
fn definition_names_stay_unique_after_every_pass() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        for (pipeline, make) in PIPELINES {
            let Ok(done) = run(compile(what, source), *make) else {
                continue;
            };
            let module = json(&done);
            for list in ["functions", "structs", "enums", "traits"] {
                let mut seen = names(&module, list);
                seen.sort();
                for pair in seen.windows(2) {
                    if let [a, b] = pair {
                        if a == b {
                            wrong.push(format!(
                                "{what}, after {pipeline}: two {list} are called `{a}`"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} name(s) are defined twice:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// A closure that reads `self.field` uses `self`, so `self` must be in
/// its capture list. Without it the closure has no `self` once it
/// leaves the method.
#[test]
fn a_closure_that_reads_self_captures_self() {
    let module = compile(
        "a closure that reads self",
        "struct Counter { v: I32 }\n\
         impl Counter { fn adder(self) -> (I32) -> I32 { (n: I32) -> n + self.v } }\n\
         pub fn probe() -> I32 {\n    let add = Counter(v: 8).adder()\n    add(2)\n}\n",
    );
    let mut wrong = Vec::new();
    walk(&json(&module), "", &mut |key, node| {
        if key != "Closure" {
            return;
        }
        let mut reads_self = false;
        walk(&node["body"], "", &mut |k, _| {
            if k == "SelfFieldRef" {
                reads_self = true;
            }
        });
        let captures = node["captures"].to_string();
        if reads_self && !captures.contains("\"self\"") {
            wrong.push(format!(
                "a closure reads self.field, and captures {captures}"
            ));
        }
    });
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// `ClosureConversionPass` lifts each closure body to a top-level
/// function whose first parameter is `__env`. Such a function has no
/// `self`, so its body must not read a field of `self`.
#[test]
fn a_lifted_closure_does_not_read_self() {
    let module = compile(
        "a closure that reads self",
        "struct Counter { v: I32 }\n\
         impl Counter { fn adder(self) -> (I32) -> I32 { (n: I32) -> n + self.v } }\n\
         pub fn probe() -> I32 {\n    let add = Counter(v: 8).adder()\n    add(2)\n}\n",
    );
    let done = match run(module, up_to_closure_conversion) {
        Ok(m) => m,
        Err(e) => panic!("closure conversion must accept the program: {e}"),
    };
    let module = json(&done);
    let mut wrong = Vec::new();
    let functions = module.get("functions").and_then(Json::as_array);
    for function in functions.into_iter().flatten() {
        let name = function["name"].as_str().unwrap_or_default();
        let has_self = function["params"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["name"] == "self"));
        let mut reads_self = false;
        walk(&function["body"], "", &mut |k, _| {
            if k == "SelfFieldRef" {
                reads_self = true;
            }
        });
        if reads_self && !has_self {
            wrong.push(name.to_string());
        }
    }
    assert!(
        wrong.is_empty(),
        "these top-level functions read self.field but have no self: {wrong:?}"
    );
}

/// `ResolveReferencesPass` says it rewrites each `Reference` to carry
/// a resolved target. So no `Unresolved` target remains after it.
#[test]
fn reference_resolution_leaves_no_unresolved_target() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        let Ok(done) = run(compile(what, source), || {
            mono().pass(ResolveReferencesPass::new())
        }) else {
            continue;
        };
        let text = json(&done).to_string();
        let left = text.matches("\"Unresolved\"").count();
        if left > 0 {
            wrong.push(format!("{what}: {left} reference(s) stay unresolved"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// `ResolveReferencesPass` says it is idempotent: "running it twice
/// produces the same output as running it once".
#[test]
fn reference_resolution_is_idempotent() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        let Ok(once) = run(compile(what, source), || {
            mono().pass(ResolveReferencesPass::new())
        }) else {
            continue;
        };
        let first = json(&once);
        let Ok(twice) = run(once, || Pipeline::new().pass(ResolveReferencesPass::new())) else {
            wrong.push(format!("{what}: the second run failed"));
            continue;
        };
        if json(&twice) != first {
            wrong.push(format!("{what}: the second run changed the module"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// An `IrModule` after any pass must come back from its JSON unchanged.
/// A backend in another process reads the module from that JSON.
#[test]
fn a_module_after_any_pass_round_trips_through_json() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        for (pipeline, make) in PIPELINES {
            let Ok(done) = run(compile(what, source), *make) else {
                continue;
            };
            let text = json(&done).to_string();
            match serde_json::from_str::<IrModule>(&text) {
                Ok(back) if json(&back) == json(&done) => {}
                Ok(_) => wrong.push(format!("{what}, after {pipeline}: the JSON changed")),
                Err(e) => wrong.push(format!("{what}, after {pipeline}: {e}")),
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every pipeline must accept every program of [`PROGRAMS`]. Each one
/// compiles, so a pass that refuses it refuses a valid program.
#[test]
fn every_pipeline_accepts_every_program() {
    let mut wrong = Vec::new();
    for (what, source) in PROGRAMS {
        for (pipeline, make) in PIPELINES {
            if let Err(e) = run(compile(what, source), *make) {
                wrong.push(format!("{what}: {pipeline} failed: {e}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A call that binds one type parameter to two types is a conflict.
/// `MonomorphisePass` reports it: it does not drop one of the two
/// bindings and copy the function for the other.
///
/// The semantic pass rejects such a call, so the test changes the type
/// of one argument in the IR after lowering.
#[test]
fn monomorphisation_reports_a_conflicting_type_argument() {
    let mut module = compile(
        "a generic call",
        "fn pick<T>(a: T, b: T) -> T {\n    a\n}\n\npub fn probe() -> I32 {\n    pick(a: 1, b: 2)\n}\n",
    );
    let Some(probe) = module.functions.iter_mut().find(|f| f.name == "probe") else {
        panic!("the program declares probe");
    };
    let Some(formalang::ir::IrExpr::FunctionCall { args, .. }) = probe.body.as_mut() else {
        panic!("the body of probe is one call: {:?}", probe.body);
    };
    let Some((_, formalang::ir::IrExpr::Literal { ty, .. })) = args.get_mut(1) else {
        panic!("the second argument is a literal: {args:?}");
    };
    *ty = formalang::ir::ResolvedType::Primitive(formalang::ast::PrimitiveType::String);
    match run(module, mono) {
        Ok(_) => panic!("a conflict must be an error"),
        Err(errors) => assert!(
            errors.contains("binds the type parameter `T`"),
            "the error must name the conflict: {errors}"
        ),
    }
}
