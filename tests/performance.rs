//! Guards on how the compiler scales.
//!
//! These are not benchmarks. A benchmark answers "how fast"; these
//! answer "what shape", and a shape regression is a defect. Each one
//! compares two input sizes and checks that the cost grew with the
//! input rather than with something else.
//!
//! The thresholds carry a large margin — ten times or more — on
//! purpose. A shared CI runner is slow and noisy, so a tight bound
//! would flake. The defects these guard against were not marginal:
//! they were 16x per 4x of input, and 2x per level of nesting.
//!
//! `benches/scaling.rs` measures the same dimensions in detail.

#![expect(
    clippy::format_push_string,
    reason = "the input generators build source text with format!"
)]

use std::time::{Duration, Instant};

use formalang::{compile_to_ir, parse_only, Lexer};

/// Wrap `inner` in `n` levels of `open`/`close`, inside a function.
fn nest(n: usize, open: &str, close: &str, inner: &str) -> String {
    let mut body = String::from(inner);
    for _ in 0..n {
        body = format!("{open}{body}{close}");
    }
    format!("pub fn f(x: I32) -> I32 {{\n    {body}\n}}\n")
}

/// `n` structs, each with two fields.
fn many_structs(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!(
            "pub struct S{i} {{\n    a: I32,\n    b: String\n}}\n\n"
        ));
    }
    s
}

/// Time one parse.
fn time_parse(source: &str) -> Duration {
    let start = Instant::now();
    let _ = parse_only(source);
    start.elapsed()
}

// ---------------------------------------------------------------------------
// Nesting depth
// ---------------------------------------------------------------------------

/// Parse cost must follow the size of the source, not the depth of its
/// nesting.
///
/// The parser used to try two alternatives that shared a whole
/// expression prefix — an assignment against a bare expression in
/// `block_item`, and a dictionary entry against an array element in
/// `array_or_dict`. Each level of nesting doubled the work, so a
/// 114-byte file with 20 nested blocks took 14.7 seconds and a
/// 90-byte file with 24 nested arrays took 57 seconds. Both now read
/// their shared prefix once and branch on what follows.
#[test]
fn parse_time_follows_the_source_size_not_the_nesting_depth() {
    for (name, open, close) in [
        ("block", "{ ", " }"),
        ("array", "[", "]"),
        ("if", "if x > 0 { ", " } else { 0 }"),
        ("for", "for i in 0..1 { ", " }.count()"),
    ] {
        // Four more levels add a handful of bytes. Under the old
        // parser they multiplied the time by sixteen.
        let shallow = time_parse(&nest(8, open, close, "1"));
        let deep = time_parse(&nest(12, open, close, "1"));
        let ratio = deep.as_secs_f64() / shallow.as_secs_f64().max(1e-9);
        assert!(
            ratio < 8.0,
            "{name}: four more levels of nesting multiplied the parse time by \
             {ratio:.1}, which is exponential growth"
        );
    }
}

/// A deeply nested program must parse in a reasonable time in
/// absolute terms, not only relative to a smaller one.
#[test]
fn a_deeply_nested_program_parses_quickly() {
    for (name, open, close) in [("block", "{ ", " }"), ("array", "[", "]")] {
        let source = nest(40, open, close, "1");
        let elapsed = time_parse(&source);
        assert!(
            elapsed < Duration::from_secs(2),
            "{name}: parsing {} bytes nested 40 deep took {elapsed:?}",
            source.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Source length
// ---------------------------------------------------------------------------

/// Lexing must be linear in the source length.
///
/// The lexer used to convert each token's byte offsets to a line and
/// column with a helper that rescanned the source from byte zero, so
/// lexing cost a pass per token. A 26 KB file took 172 ms.
/// `LineIndex` now indexes the source once and the lexer reuses it.
#[test]
fn lexing_is_linear_in_the_source_length() {
    let small = many_structs(64);
    let large = many_structs(256);

    // Warm the caches so the first call does not pay for both.
    let _ = Lexer::tokenize_all_with_errors(&small);

    let start = Instant::now();
    let _ = Lexer::tokenize_all_with_errors(&small);
    let small_time = start.elapsed().as_secs_f64();

    let start = Instant::now();
    let _ = Lexer::tokenize_all_with_errors(&large);
    let large_time = start.elapsed().as_secs_f64();

    let ratio = large_time / small_time.max(1e-9);
    assert!(
        ratio < 12.0,
        "a 4x larger source took {ratio:.1}x longer to lex; linear would be \
         about 4x, and quadratic about 16x"
    );
}

/// Parsing must be linear in the number of AST nodes.
///
/// Filling in each node's line and column used to resolve its offsets
/// against the source text, and each resolution scanned the whole
/// source. One span per node made parsing cost
/// `O(nodes * source length)`: 1024 structs took 296 ms. The walk now
/// shares one `LineIndex`.
///
/// The check uses two shapes, because the cost followed the node count
/// rather than the byte count — a file of nothing but comments was
/// always fast.
#[test]
fn parsing_is_linear_in_the_node_count() {
    for (name, generate) in [
        ("structs", many_structs as fn(usize) -> String),
        ("locals", many_locals as fn(usize) -> String),
    ] {
        let small = generate(64);
        let large = generate(256);
        let _ = parse_only(&small);

        let small_time = time_parse(&small).as_secs_f64();
        let large_time = time_parse(&large).as_secs_f64();
        let ratio = large_time / small_time.max(1e-9);

        assert!(
            ratio < 12.0,
            "{name}: a 4x larger source took {ratio:.1}x longer to parse; linear \
             would be about 4x, and quadratic about 16x"
        );
    }
}

/// One function holding `n` sequential `let` bindings.
fn many_locals(n: usize) -> String {
    let mut s = String::from("pub fn locals() -> I32 {\n");
    for i in 0..n {
        s.push_str(&format!("    let v{i} = {i}\n"));
    }
    s.push_str("    0\n}\n");
    s
}

/// Compiling must be roughly linear in the number of definitions.
#[test]
fn compiling_is_roughly_linear_in_the_definition_count() {
    let small = many_structs(32);
    let large = many_structs(128);
    let _ = compile_to_ir(&small);

    let start = Instant::now();
    let _ = compile_to_ir(&small);
    let small_time = start.elapsed().as_secs_f64();

    let start = Instant::now();
    let _ = compile_to_ir(&large);
    let large_time = start.elapsed().as_secs_f64();

    let ratio = large_time / small_time.max(1e-9);
    assert!(
        ratio < 12.0,
        "a 4x larger source took {ratio:.1}x longer to compile"
    );
}

// ---------------------------------------------------------------------------
// The fixed cost of one compile
// ---------------------------------------------------------------------------

/// The smallest program the compiler accepts must compile quickly.
///
/// Every entry point prepends the compiler-shipped prelude to the
/// user's source, so every compile pays a fixed cost before it reads
/// a byte the caller wrote. This test bounds that fixed cost in
/// absolute terms, the way
/// `a_deeply_nested_program_parses_quickly` bounds the nesting pair.
///
/// The first call in a process pays a one-time prelude parse, so this
/// test warms it up first and then measures what a caller pays per
/// compile.
///
/// The bound is deliberately loose. A debug build of a minimal
/// compile takes about 1.1 ms on a developer machine and about 3.2 ms
/// on a shared CI runner, and a loaded runner can be several times
/// worse again. A bound tight enough to catch a 3x regression would
/// flake, so this one only catches an order-of-magnitude blow-up.
/// The precise guard on the prelude — the regression that made the
/// fixed cost 94% of a minimal compile — is
/// `the_prelude_is_parsed_once_per_process`, which compares the two
/// costs against each other and needs no absolute number.
#[test]
fn compiling_a_minimal_program_is_fast() {
    const SOURCE: &str = "pub struct A { a: I32 }";

    // Pay the one-time prelude parse outside the measurement.
    let _ = compile_to_ir(SOURCE);

    // Average over several runs: a single run on a loaded CI runner
    // can be scheduled out halfway through.
    let runs = 20;
    let start = Instant::now();
    for _ in 0..runs {
        let _ = compile_to_ir(SOURCE);
    }
    let each = start.elapsed() / runs;

    assert!(
        each < Duration::from_millis(50),
        "the smallest program the compiler accepts took {each:?} per compile"
    );
}

/// The prelude must be parsed once per process, not once per compile.
///
/// A thousand compiles of a one-line program must not cost a thousand
/// prelude parses. Parsing the prelude on its own is the comparison:
/// if the cache works, the per-compile cost stays well under it.
#[test]
fn the_prelude_is_parsed_once_per_process() {
    const SOURCE: &str = "pub struct A { a: I32 }";
    let prelude = include_str!("../src/prelude.fv");

    let _ = compile_to_ir(SOURCE);

    let start = Instant::now();
    let _ = parse_only(prelude);
    let prelude_parse = start.elapsed().as_secs_f64();

    let runs = 20;
    let start = Instant::now();
    for _ in 0..runs {
        let _ = compile_to_ir(SOURCE);
    }
    let each = start.elapsed().as_secs_f64() / f64::from(runs);

    assert!(
        each < prelude_parse,
        "one compile of a one-line program costs {each:.6}s, and parsing the \
         prelude alone costs {prelude_parse:.6}s; the prelude is being \
         re-parsed on every call"
    );
}
