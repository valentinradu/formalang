//! Adversarial programs for the IR passes.
//!
//! Each test holds one program and the answer that its `probe` function
//! must give. The test runs the program before any pass, and then after
//! each pipeline in [`pipelines`]. Every run must give the same stated
//! answer. A second run of the same pipeline must give it too.
//!
//! The programs aim at shapes that the other suites do not reach:
//! polymorphic recursion, a generic parameter that only a generic
//! struct argument gives, closures that read `self`, and user names
//! that the passes also generate.
//!
//! Each pipeline runs in a child process with a time limit. A pass that
//! does not stop, or that uses all the memory, then fails one test and
//! does not stop the suite.

#![expect(clippy::panic, reason = "a test reports its failure by failing loudly")]

use crate::common::interpreter::{Interpreter, Value};
use crate::common::with_a_large_stack;

use formalang::compile_to_ir;
use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::pipeline::Pipeline;

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The environment variable that gives the child its program.
const SOURCE_VAR: &str = "FORMALANG_PASS_ADVERSARIAL_SOURCE";
/// The environment variable that gives the child its pipeline.
const PIPELINE_VAR: &str = "FORMALANG_PASS_ADVERSARIAL_PIPELINE";
/// The time that one pipeline gets on one small program. The limit
/// only catches a hang. A normal run takes milliseconds, but under the
/// load of the full suite a child process can wait more than 10 s to
/// start, so the limit is large.
const LIMIT: Duration = Duration::from_secs(60);

/// The pipelines that each program goes through, by name.
///
/// The list holds each pass alone, the codegen pipeline, the opt-in
/// passes after it, other orders of the passes, and each pass twice.
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
            Pipeline::new().pass(DeadCodeEliminationPass::new()),
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
        ("for_codegen", Pipeline::for_codegen()),
        (
            "for_codegen, then defunctionalise, fold and DCE",
            Pipeline::for_codegen()
                .pass(DefunctionalisePass)
                .pass(ConstantFoldingPass::new())
                .pass(DeadCodeEliminationPass::new()),
        ),
        (
            "fold before monomorphise",
            Pipeline::new()
                .pass(ConstantFoldingPass::new())
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new()),
        ),
        (
            "DCE before closure conversion",
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(DeadCodeEliminationPass::new())
                .pass(ClosureConversionPass::new()),
        ),
        (
            "each pass twice",
            Pipeline::new()
                .pass(MonomorphisePass::default())
                .pass(MonomorphisePass::default())
                .pass(ResolveReferencesPass::new())
                .pass(ResolveReferencesPass::new())
                .pass(ClosureConversionPass::new())
                .pass(ClosureConversionPass::new())
                .pass(DeadCodeEliminationPass::new())
                .pass(DeadCodeEliminationPass::new())
                .pass(ConstantFoldingPass::new())
                .pass(ConstantFoldingPass::new()),
        ),
    ]
}

/// Run `probe` and give its answer as text.
fn answer(module: &IrModule) -> String {
    let mut interpreter = Interpreter::new(module);
    match interpreter.run("probe") {
        Ok(Value::Int(n)) => n.to_string(),
        Ok(other) => format!("{other:?}"),
        Err(fault) => format!("a fault: {fault}"),
    }
}

/// The work of the child process: run one pipeline twice over one
/// program, and print the two answers.
fn child_work(source: &str, pipeline: &str) -> String {
    let source = source.to_string();
    let pipeline = pipeline.to_string();
    with_a_large_stack(move || {
        let module = match compile_to_ir(&source) {
            Ok(m) => m,
            Err(errors) => return format!("did not compile: {errors:?}"),
        };
        let Some((_, mut first)) = pipelines().into_iter().find(|(n, _)| *n == pipeline) else {
            return format!("no pipeline is called {pipeline}");
        };
        let once = match first.run(module) {
            Ok(m) => m,
            Err(errors) => return format!("the pipeline failed: {errors:?}"),
        };
        let Some((_, mut second)) = pipelines().into_iter().find(|(n, _)| *n == pipeline) else {
            return format!("no pipeline is called {pipeline}");
        };
        let twice = match second.run(once.clone()) {
            Ok(m) => m,
            Err(errors) => return format!("the second run failed: {errors:?}"),
        };
        format!("{}|{}", answer(&once), answer(&twice))
    })
}

/// The entry point of the child process.
///
/// The parent starts the test binary again with this test as the only
/// filter, and gives it the program and the pipeline in the
/// environment. Without them this test has no work.
#[test]
#[expect(
    clippy::print_stdout,
    reason = "the child gives its result to the parent on standard output"
)]
fn child_entry() {
    let (Ok(source), Ok(pipeline)) = (std::env::var(SOURCE_VAR), std::env::var(PIPELINE_VAR))
    else {
        assert!(std::env::var(SOURCE_VAR).is_err() || std::env::var(PIPELINE_VAR).is_err());
        return;
    };
    println!("CHILD-RESULT<{}>", child_work(&source, &pipeline));
}

/// Run one pipeline over one program in a child process, with a time
/// limit. Give the output of the child, or the reason it has none.
fn in_a_child(source: &str, pipeline: &str) -> String {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("cannot find the test binary: {e}"),
    };
    let child = Command::new(exe)
        .env(SOURCE_VAR, source)
        .env(PIPELINE_VAR, pipeline)
        .arg(crate::common::test_name(module_path!(), "child_entry"))
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => panic!("cannot start the child process: {e}"),
    };
    // Read both pipes on their own threads, so a child that writes
    // much text does not block on a full pipe.
    let stdout = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let stderr = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return format!("did not finish in {} s", LIMIT.as_secs());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("cannot wait for the child process: {e}"),
        }
    }
    let out = stdout.and_then(|t| t.join().ok()).unwrap_or_default();
    let err = stderr.and_then(|t| t.join().ok()).unwrap_or_default();
    match (out.find("CHILD-RESULT<"), out.rfind('>')) {
        (Some(start), Some(end)) if end > start => out
            .get(start..end)
            .and_then(|s| s.strip_prefix("CHILD-RESULT<"))
            .unwrap_or_default()
            .to_string(),
        _ => {
            // Keep the first lines of the panic message. They name the
            // defect, for example an ill-typed IR.
            let panic: Vec<&str> = err
                .lines()
                .skip_while(|l| !l.contains("panicked at"))
                .skip(1)
                .take(4)
                .collect();
            format!("the child process stopped with: {}", panic.join(" / "))
        }
    }
}

/// Check that `source` answers `expected` before any pass and after
/// every pipeline, once and twice.
fn check(source: &str, expected: i128) {
    let expected = expected.to_string();
    let mut wrong = Vec::new();

    let src = source.to_string();
    let before = with_a_large_stack(move || match compile_to_ir(&src) {
        Ok(m) => answer(&m),
        Err(errors) => format!("did not compile: {errors:?}"),
    });
    if before != expected {
        wrong.push(format!("before any pass: {before}"));
    }

    for (name, _) in pipelines() {
        let got = in_a_child(source, name);
        let want = format!("{expected}|{expected}");
        if got != want {
            wrong.push(format!("{name}: {got} (once|twice)"));
        }
    }

    assert!(
        wrong.is_empty(),
        "the program must answer {expected} before and after every pass:\n{source}\n  {}",
        wrong.join("\n  ")
    );
}

macro_rules! program {
    ($name:ident, $expected:expr, $source:expr) => {
        #[test]
        fn $name() {
            check($source, $expected);
        }
    };
}

// Monomorphisation.

/// Polymorphic recursion: `grow<T>` calls `grow<Box<T>>`. The program
/// is correct and answers 3, but monomorphisation cannot copy it: each
/// level needs a new copy, with no end. A pipeline that monomorphises
/// must refuse it with `InstantiationDepthExceeded`, in bounded time. It
/// may not hang, and it may not give an internal error.
#[test]
fn polymorphic_recursion_through_a_generic_struct() {
    let source = "struct Box<T> { value: T }\n\
                  fn grow<T>(x: T, n: I32) -> I32 { if n <= 0 { 0 } else { 1 + grow(x: Box(value: x), n: n - 1) } }\n\
                  pub fn probe() -> I32 { grow(x: 1, n: 3) }\n";
    let no_monomorphisation = [
        "ResolveReferencesPass",
        "ConstantFoldingPass",
        "DeadCodeEliminationPass",
    ];
    let src = source.to_string();
    let before = with_a_large_stack(move || match compile_to_ir(&src) {
        Ok(m) => answer(&m),
        Err(errors) => format!("did not compile: {errors:?}"),
    });
    let mut wrong = Vec::new();
    if before != "3" {
        wrong.push(format!("before any pass: {before}"));
    }
    for (name, _) in pipelines() {
        let got = in_a_child(source, name);
        let ok = if no_monomorphisation.contains(&name) {
            got == "3|3"
        } else {
            got.starts_with("the pipeline failed:")
                && got.contains("InstantiationDepthExceeded")
                && !got.contains("InternalError")
        };
        if !ok {
            wrong.push(format!("{name}: {got}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "polymorphic recursion must answer 3 without monomorphisation, and \
         give InstantiationDepthExceeded with it:\n  {}",
        wrong.join("\n  ")
    );
}

program!(
    a_type_parameter_that_only_a_generic_struct_argument_gives,
    6,
    "struct Box<T> { value: T }\n\
     fn unbox<T>(b: Box<T>) -> T { b.value }\n\
     pub fn probe() -> I32 { unbox(b: Box(value: 6)) }\n"
);

program!(
    two_type_parameters_that_a_generic_struct_argument_gives,
    5,
    "struct Pair<A, B> { a: A, b: B }\n\
     fn swap<A, B>(p: Pair<A, B>) -> Pair<B, A> { Pair(a: p.b, b: p.a) }\n\
     pub fn probe() -> I32 { swap(p: Pair(a: \"x\", b: 5)).a }\n"
);

program!(
    a_generic_struct_argument_that_gives_the_first_of_two_parameters,
    4,
    "struct Pair<A, B> { a: A, b: B }\n\
     fn first<A, B>(p: Pair<A, B>) -> A { p.a }\n\
     pub fn probe() -> I32 { first(p: Pair(a: 4, b: \"x\")) }\n"
);

program!(
    a_generic_struct_argument_at_two_types,
    2,
    "struct Box<T> { value: T }\n\
     fn unbox<T>(b: Box<T>) -> T { b.value }\n\
     pub fn probe() -> I32 { if unbox(b: Box(value: \"s\")) == \"s\" { unbox(b: Box(value: 2)) } else { 0 } }\n"
);

program!(
    a_generic_struct_argument_read_through_a_method,
    2,
    "struct Box<T> { value: T }\n\
     impl Box<T> { fn get(self) -> T { self.value } }\n\
     fn unbox<T>(b: Box<T>) -> T { b.get() }\n\
     pub fn probe() -> I32 { unbox(b: Box(value: 2)) }\n"
);

program!(
    a_nested_generic_struct_argument,
    13,
    "struct Box<T> { value: T }\n\
     fn deep<T>(b: Box<Box<T>>) -> T { b.value.value }\n\
     pub fn probe() -> I32 { deep(b: Box(value: Box(value: 13))) }\n"
);

program!(
    a_generic_enum_argument,
    5,
    "enum Maybe<T> { some(v: T), none }\n\
     fn get<T>(m: Maybe<T>, d: T) -> T { match m { .some(v): v, .none: d } }\n\
     pub fn probe() -> I32 { get(m: Maybe.some(v: 5), d: 0) }\n"
);

program!(
    a_generic_function_that_returns_a_generic_struct,
    11,
    "struct Box<T> { value: T }\n\
     fn wrap<T>(x: T) -> Box<T> { Box(value: x) }\n\
     pub fn probe() -> I32 { wrap(x: 11).value }\n"
);

program!(
    a_generic_over_an_array_element,
    7,
    "fn head<T>(xs: [T], d: T) -> T { if let v = xs[0] { v } else { d } }\n\
     pub fn probe() -> I32 { head(xs: [7, 8], d: 0) }\n"
);

program!(
    a_generic_over_an_optional,
    5,
    "fn unwrap<T>(o: T?, d: T) -> T { if let v = o { v } else { d } }\n\
     pub fn probe() -> I32 {\n    let o: I32? = 5\n    unwrap(o: o, d: 1)\n}\n"
);

program!(
    a_generic_over_a_dictionary_value,
    2,
    "fn lookup<V>(d: [String: V], k: String, dflt: V) -> V { if let v = d[k] { v } else { dflt } }\n\
     pub fn probe() -> I32 { lookup(d: [\"a\": 1, \"b\": 2], k: \"b\", dflt: 0) }\n"
);

program!(
    a_generic_over_a_named_tuple,
    3,
    "fn fst<A, B>(t: (a: A, b: B)) -> A { t.a }\n\
     pub fn probe() -> I32 { fst(t: (a: 3, b: \"x\")) }\n"
);

program!(
    a_generic_that_calls_a_generic,
    9,
    "fn pass<T>(x: T) -> T { x }\n\
     fn outer<T>(x: T) -> T { pass(x: x) }\n\
     pub fn probe() -> I32 { outer(x: 9) }\n"
);

program!(
    a_generic_with_recursion_at_two_types,
    5,
    "fn depth<T>(x: T, n: I32) -> I32 { if n <= 0 { 0 } else { 1 + depth(x: x, n: n - 1) } }\n\
     pub fn probe() -> I32 { depth(x: \"s\", n: 3) + depth(x: 1, n: 2) }\n"
);

program!(
    a_generic_instantiated_with_a_closure,
    2,
    "fn id<T>(x: T) -> T { x }\n\
     pub fn probe() -> I32 {\n    let f = id(x: (n: I32) -> n + 1)\n    f(1)\n}\n"
);

program!(
    a_generic_instantiated_with_nil,
    9,
    "fn id<T>(x: T) -> T { x }\n\
     pub fn probe() -> I32 {\n    let o: I32? = id(x: nil)\n    if let v = o { v } else { 9 }\n}\n"
);

program!(
    a_generic_struct_that_holds_a_closure,
    15,
    "struct Box<T> { value: T }\n\
     pub fn probe() -> I32 {\n    let b = Box(value: (n: I32) -> n * 3)\n    let f = b.value\n    f(5)\n}\n"
);

program!(
    a_generic_closure_parameter,
    5,
    "fn apply<T>(f: (T) -> T, x: T) -> T { f(x) }\n\
     pub fn probe() -> I32 { apply(f: (n: I32) -> n + 1, x: 4) }\n"
);

program!(
    a_generic_method_on_a_nested_generic,
    3,
    "struct Box<T> { value: T }\n\
     impl Box<T> { fn get(self) -> T { self.value } }\n\
     pub fn probe() -> I32 { Box(value: Box(value: 3)).get().get() }\n"
);

program!(
    a_trait_bound_at_two_types,
    12,
    "trait Named { fn name(self) -> I32 }\n\
     struct A { }\n\
     struct B { }\n\
     impl Named for A { fn name(self) -> I32 { 1 } }\n\
     impl Named for B { fn name(self) -> I32 { 2 } }\n\
     fn get<T: Named>(x: T) -> I32 { x.name() }\n\
     pub fn probe() -> I32 { get(x: A()) * 10 + get(x: B()) }\n"
);

// Closure conversion and defunctionalisation.

program!(
    a_closure_that_reads_self_escapes_its_method,
    10,
    "struct Counter { v: I32 }\n\
     impl Counter { fn adder(self) -> (I32) -> I32 { (n: I32) -> n + self.v } }\n\
     pub fn probe() -> I32 {\n    let add = Counter(v: 8).adder()\n    add(2)\n}\n"
);

program!(
    a_user_function_named_like_the_generated_dispatcher,
    1002,
    "fn __call_Fn0(x: I32) -> I32 { x + 1000 }\n\
     pub fn probe() -> I32 {\n    let f = (n: I32) -> n + 1\n    f(1) + __call_Fn0(x: 0)\n}\n"
);

program!(
    user_definitions_named_like_the_generated_closure_parts,
    108,
    "struct __ClosureEnv0 { v: I32 }\n\
     fn __closure0(x: I32) -> I32 { x * 100 }\n\
     pub fn probe() -> I32 {\n    let k = 2\n    let f = (n: I32) -> n + k\n    \
     f(1) + __closure0(x: 1) + __ClosureEnv0(v: 5).v\n}\n"
);

program!(
    a_capture_named_like_the_generated_env_parameter,
    8,
    "pub fn probe() -> I32 {\n    let __env = 7\n    let f = (n: I32) -> n + __env\n    f(1)\n}\n"
);

program!(
    a_closure_that_calls_a_closure,
    12,
    "pub fn probe() -> I32 {\n    let x = 5\n    let f = (n: I32) -> n + x\n    \
     let g = (m: I32) -> f(m) * 2\n    g(1)\n}\n"
);

program!(
    a_closure_inside_a_closure,
    113,
    "pub fn probe() -> I32 {\n    let a = 3\n    let f = (n: I32) -> {\n        \
     let g = (m: I32) -> m + n + a\n        g(10)\n    }\n    f(100)\n}\n"
);

program!(
    closures_of_two_arities,
    37,
    "fn apply(f: (I32) -> I32, x: I32) -> I32 { f(x) }\n\
     fn apply2(f: (I32, I32) -> I32) -> I32 { f(3, 4) }\n\
     pub fn probe() -> I32 { apply(f: (n: I32) -> n * n, x: 5) + apply2(f: (a: I32, b: I32) -> a * b) }\n"
);

program!(
    closures_of_two_shapes,
    11,
    "pub fn probe() -> I32 {\n    let s = \"hi\"\n    \
     let f = (n: I32) -> if s == \"hi\" { n } else { 0 }\n    \
     let g = (b: Boolean) -> if b { 7 } else { 8 }\n    f(3) + g(false)\n}\n"
);

program!(
    a_closure_chosen_by_a_branch,
    11,
    "pub fn probe() -> I32 {\n    let f = (n: I32) -> n + 1\n    let g = (n: I32) -> n * 2\n    \
     let h = if true { f } else { g }\n    h(10)\n}\n"
);

program!(
    a_closure_that_shadows_a_capture,
    51,
    "pub fn probe() -> I32 {\n    let x = 1\n    let f = (n: I32) -> {\n        \
     let x = 50\n        n + x\n    }\n    f(x)\n}\n"
);

program!(
    closures_in_an_array,
    42,
    "pub fn probe() -> I32 {\n    let fs = [(n: I32) -> n + 1, (n: I32) -> n * 2]\n    \
     if let g = fs[1] { g(21) } else { 0 }\n}\n"
);

program!(
    closures_in_a_dictionary,
    42,
    "pub fn probe() -> I32 {\n    let d = [\"a\": (n: I32) -> n + 1, \"b\": (n: I32) -> n + 2]\n    \
     if let f = d[\"b\"] { f(40) } else { 0 }\n}\n"
);

program!(
    a_closure_in_a_loop_with_a_capture,
    36,
    "pub fn probe() -> I32 {\n    let xs = [1, 2, 3]\n    let k = 10\n    \
     for x in xs { x + k }.fold(initial: 0, f: (a, b) -> a + b)\n}\n"
);

// Dead-code elimination.

program!(
    a_function_reached_only_from_a_parameter_default,
    6,
    "fn helper() -> I32 { 5 }\n\
     fn add(a: I32, b: I32 = helper()) -> I32 { a + b }\n\
     pub fn probe() -> I32 { add(a: 1) }\n"
);

program!(
    a_function_reached_only_from_a_field_default,
    9,
    "struct Point { x: I32 = seed() }\n\
     fn seed() -> I32 { 9 }\n\
     pub fn probe() -> I32 { Point().x }\n"
);

program!(
    a_function_reached_only_from_a_trait_impl,
    9,
    "trait Shape { fn area(self) -> I32 }\n\
     struct Sq { s: I32 }\n\
     impl Shape for Sq { fn area(self) -> I32 { helper(v: self.s) } }\n\
     fn helper(v: I32) -> I32 { v * v }\n\
     fn total<T: Shape>(x: T) -> I32 { x.area() }\n\
     pub fn probe() -> I32 { total(x: Sq(s: 3)) }\n"
);

program!(
    mutual_recursion,
    1,
    "fn even(n: I32) -> Boolean { if n == 0 { true } else { odd(n: n - 1) } }\n\
     fn odd(n: I32) -> Boolean { if n == 0 { false } else { even(n: n - 1) } }\n\
     pub fn probe() -> I32 { if even(n: 10) { 1 } else { 0 } }\n"
);

program!(
    an_enum_built_behind_a_call,
    4,
    "enum E { a, b(v: I32) }\n\
     fn mk(n: I32) -> E { if n > 0 { E.b(v: n) } else { E.a } }\n\
     pub fn probe() -> I32 { match mk(n: 4) { .a: 0, .b(v): v } }\n"
);

// Constant folding.

program!(
    a_division_by_zero_in_a_dead_branch,
    3,
    "pub fn probe() -> I32 {\n    if false { 1 / 0 } else { 3 }\n}\n"
);

program!(
    integer_division_truncates_to_zero,
    27,
    "pub fn probe() -> I32 { (7 / 2) * 10 + (0 - 7) / 2 }\n"
);

program!(
    the_remainder_takes_the_sign_of_the_dividend,
    9,
    "pub fn probe() -> I32 { (0 - 7) % 3 + 10 }\n"
);

program!(
    floating_point_sums_are_not_exact,
    0,
    "pub fn probe() -> I32 { if 0.1 + 0.2 == 0.3 { 1 } else { 0 } }\n"
);
