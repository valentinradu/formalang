//! What the library promises a multi-threaded caller.
//!
//! The compiler itself is single-threaded, but its callers are not: a
//! build tool compiles many files at once, and an editor runs the
//! analyser on a worker thread. Three things must hold.
//!
//! - The public types cross thread boundaries. A type that is not
//!   `Send` cannot be moved onto a worker at all, and the failure is a
//!   compile error in the caller's code, not here.
//! - Compiling on many threads gives the same answers as compiling on
//!   one. A difference means some hidden global state.
//! - Rendering a diagnostic does not disturb another thread's
//!   rendering.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_collect,
    reason = "tests assert their fixtures hold; the thread handles must all be \
              spawned before any is joined, so the collect is load-bearing"
)]

use std::sync::{Arc, Barrier};
use std::thread;

use formalang::{compile_to_ir, report_errors, IrModule, Pipeline};

// ---------------------------------------------------------------------------
// Thread-safety of the public types
// ---------------------------------------------------------------------------

const fn assert_send<T: Send>() {}
const fn assert_sync<T: Sync>() {}

/// The published data types must cross thread boundaries.
///
/// These are compile-time assertions; the test body only pins them in
/// place so a change that removes `Send` fails here instead of in a
/// downstream crate.
#[test]
fn public_types_are_send_and_sync() {
    assert_send::<IrModule>();
    assert_sync::<IrModule>();
    assert_send::<formalang::File>();
    assert_sync::<formalang::File>();
    assert_send::<formalang::CompilerError>();
    assert_sync::<formalang::CompilerError>();
    assert_send::<formalang::Span>();
    assert_sync::<formalang::Span>();
    assert_send::<formalang::ResolvedType>();
    assert_sync::<formalang::ResolvedType>();
    assert_send::<formalang::FileSystemResolver>();
    assert_sync::<formalang::FileSystemResolver>();
}

/// A `Vec<CompilerError>` is what every entry point returns on
/// failure. It must be movable to the thread that will report it.
#[test]
fn the_error_type_moves_between_threads() {
    let errors = compile_to_ir("fn (").unwrap_err();
    let handle = thread::spawn(move || errors.len());
    assert!(
        handle.join().expect("the worker must not panic") > 0,
        "the error list arrived empty on the worker thread"
    );
}

// ---------------------------------------------------------------------------
// Parallel compilation
// ---------------------------------------------------------------------------

const SOURCES: &[&str] = &[
    "pub struct A { a: I32 }",
    "pub enum E { one, two(x: I32) }",
    "pub fn add(x: I32, y: I32) -> I32 { x + y }",
    "pub trait T { name: String }\npub struct S { name: String }\nimpl T for S {}",
    "pub fn apply(n: I32) -> I32 {\n    let f = (v: I32) -> v + n\n    f(1)\n}",
    "pub struct Box<T> { value: T }\npub fn take() -> I32 {\n    let b = Box<I32>(value: 1)\n    b.value\n}",
    "pub fn total(xs: [I32]) -> I32 {\n    for x in xs { x }.fold(initial: 0, f: (a, b) -> a + b)\n}",
    "pub mod inner {\n    pub struct P { x: I32 }\n}\npub fn make() -> inner::P {\n    inner::P(x: 1)\n}",
];

fn json(module: &IrModule) -> String {
    serde_json::to_string(module).unwrap_or_default()
}

/// Compiling on eight threads must give what compiling on one gives.
///
/// The threads start together on a barrier, so they overlap. If any
/// phase reads process-global state, the parallel answer differs from
/// the serial one.
#[test]
fn parallel_compilation_matches_serial_compilation() {
    let serial: Vec<Option<String>> = SOURCES
        .iter()
        .map(|s| compile_to_ir(s).ok().as_ref().map(json))
        .collect();

    let barrier = Arc::new(Barrier::new(SOURCES.len()));
    let handles: Vec<_> = SOURCES
        .iter()
        .map(|source| {
            let barrier = Arc::clone(&barrier);
            let source = (*source).to_string();
            thread::spawn(move || {
                barrier.wait();
                compile_to_ir(&source).ok().as_ref().map(json)
            })
        })
        .collect();

    for (i, handle) in handles.into_iter().enumerate() {
        let parallel = handle.join().expect("a compile thread panicked");
        assert_eq!(
            serial.get(i).cloned().flatten(),
            parallel,
            "source {i} compiled differently on a worker thread than on the \
             main thread"
        );
    }
}

/// The codegen pipeline, run on many threads over the same input.
#[test]
fn parallel_pipelines_agree() {
    let source = SOURCES.get(4).copied().unwrap_or_default();
    let Ok(module) = compile_to_ir(source) else {
        panic!("the fixture must compile");
    };
    let expected = Pipeline::for_codegen()
        .run(module.clone())
        .ok()
        .as_ref()
        .map(json);

    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let module = module.clone();
            thread::spawn(move || {
                barrier.wait();
                Pipeline::for_codegen().run(module).ok().as_ref().map(json)
            })
        })
        .collect();

    for handle in handles {
        let got = handle.join().expect("a pipeline thread panicked");
        assert_eq!(
            expected, got,
            "the codegen pipeline produced different IR on a worker thread"
        );
    }
}

/// Rendering diagnostics on many threads must not panic and must
/// produce a non-empty report on every thread.
#[test]
fn parallel_reporting_produces_a_report_on_every_thread() {
    let source = "pub fn broken( {";
    let errors = Arc::new(compile_to_ir(source).unwrap_err());

    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let errors = Arc::clone(&errors);
            thread::spawn(move || {
                barrier.wait();
                report_errors(&errors, source, "input.fv")
            })
        })
        .collect();

    for handle in handles {
        let report = handle.join().expect("a reporting thread panicked");
        assert!(!report.is_empty(), "a thread rendered an empty report");
    }
}
