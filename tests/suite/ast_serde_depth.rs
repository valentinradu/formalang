//! How deep an AST can nest and still be read back.
//!
//! `File` carries a `format_version` because it is a wire format:
//! an embedder may write the AST to disk and read it back, or hand it
//! to a tool in another language.
//!
//! That round trip has a ceiling the compiler does not impose.
//! `serde_json` refuses to read more than 128 nested levels, and each
//! AST node costs more than one level, so a deeply nested expression
//! serialises cleanly and then fails to decode. The compiler itself is
//! happy either way — it never reads its own JSON.
//!
//! These tests pin where the ceiling falls, so a change that lowers it
//! is caught here rather than by a consumer. They are also the place
//! to update if the wire format is ever reshaped to nest less.

#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a fixture that stops parsing should fail loudly"
)]

use formalang::{parse_only, File};

/// Run `body` on a thread with a large stack.
///
/// Parsing is recursive, and a debug build — more so an instrumented
/// one, as `cargo llvm-cov` produces — uses far more stack per frame
/// than a release build. The deep fixtures below overflow the default
/// 2 MB test-thread stack under instrumentation. Compilers handle this
/// the same way: run the recursive work on a thread sized for it.
fn with_a_large_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    const STACK: usize = 64 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(body)
        .expect("the worker thread must start")
        .join()
        .expect("the worker thread must not panic")
}

/// Depths that every shape below must survive. Chosen well under the
/// worst measured ceiling — 37 nested array literals — so the
/// guarantee holds whatever an expression is made of.
const GUARANTEED_DEPTH: usize = 32;

/// Parse `source` and report whether its AST survives a JSON round
/// trip.
fn round_trips(source: &str) -> Option<bool> {
    let file = parse_only(source).ok()?;
    let json = serde_json::to_string(&file).ok()?;
    Some(serde_json::from_str::<File>(&json).is_ok())
}

fn nested_unary(n: usize) -> String {
    format!("pub fn f() -> I32 {{\n    {}1\n}}\n", "-".repeat(n))
}

fn nested_array(n: usize) -> String {
    format!(
        "pub fn f() -> I32 {{\n    {}1{}\n    0\n}}\n",
        "[".repeat(n),
        "]".repeat(n)
    )
}

fn nested_group(n: usize) -> String {
    format!(
        "pub fn f() -> I32 {{\n    {}1{}\n}}\n",
        "(".repeat(n),
        ")".repeat(n)
    )
}

fn chained_addition(n: usize) -> String {
    let mut body = String::from("1");
    for _ in 0..n {
        body.push_str(" + 1");
    }
    format!("pub fn f() -> I32 {{\n    {body}\n}}\n")
}

fn nested_block(n: usize) -> String {
    let mut body = String::from("1");
    for _ in 0..n {
        body = format!("{{ {body} }}");
    }
    format!("pub fn f() -> I32 {{\n    {body}\n}}\n")
}

type Generator = fn(usize) -> String;

const SHAPES: &[(&str, Generator)] = &[
    ("unary", nested_unary),
    ("array", nested_array),
    ("group", nested_group),
    ("addition", chained_addition),
    ("block", nested_block),
];

/// Every shape round-trips at the guaranteed depth.
#[test]
fn every_shape_round_trips_at_the_guaranteed_depth() {
    for (name, generate) in SHAPES {
        let source = generate(GUARANTEED_DEPTH);
        assert_eq!(
            round_trips(&source),
            Some(true),
            "{name}: an AST nested {GUARANTEED_DEPTH} deep did not survive a JSON \
             round trip"
        );
    }
}

/// Parsing itself has no such ceiling: the compiler accepts far deeper
/// input than its JSON form can be read back at.
///
/// That asymmetry is the point of this file. Nothing is wrong with the
/// tree; the reader is what gives up.
#[test]
fn parsing_accepts_more_nesting_than_the_json_reader() {
    with_a_large_stack(|| {
        for (name, generate) in SHAPES {
            let source = generate(200);
            assert!(
                parse_only(&source).is_ok(),
                "{name}: the parser rejected an AST nested 200 deep"
            );
        }
    });
}

/// The ceiling is the reader's, not the writer's: serialising always
/// works, however deep the tree.
#[test]
fn serialising_has_no_depth_ceiling() {
    with_a_large_stack(|| {
        for (name, generate) in SHAPES {
            let source = generate(200);
            let Ok(file) = parse_only(&source) else {
                panic!("{name}: the fixture must parse");
            };
            assert!(
                serde_json::to_string(&file).is_ok(),
                "{name}: serialising an AST nested 200 deep failed"
            );
        }
    });
}

/// Report where each shape's ceiling actually falls, and check it has
/// not dropped below the guarantee.
///
/// The measured ceilings, at the time of writing: 58 nested unary
/// operators, 58 nested groups, 58 chained additions, 37 nested array
/// literals. Nested blocks have no ceiling below 140 — a block adds
/// fewer JSON levels per source level than an expression does.
#[test]
fn the_measured_ceiling_is_above_the_guarantee() {
    with_a_large_stack(|| {
        for (name, generate) in SHAPES {
            let mut deepest = 0;
            for depth in 1..=140 {
                match round_trips(&generate(depth)) {
                    Some(true) => deepest = depth,
                    Some(false) | None => break,
                }
            }
            assert!(
                deepest >= GUARANTEED_DEPTH,
                "{name}: round-trips only to depth {deepest}, below the guaranteed \
             {GUARANTEED_DEPTH}"
            );
        }
    });
}
