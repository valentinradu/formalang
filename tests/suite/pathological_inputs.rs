//! Input that no one writes by hand, but that a tool, a paste or a
//! corrupt file can give the compiler.
//!
//! Every test here states one of three promises:
//!
//! - the compiler returns a result, and does not panic;
//! - every error points at a real place in the source: the byte range
//!   is inside the source, each end is on a character boundary, and the
//!   line and the column agree with the byte offset;
//! - the time to compile grows with the size of the input at a linear
//!   rate. When the input grows 16 times, the time must not grow more
//!   than 48 times, plus a small constant. See `assert_linear`.
//!
//! Deep nesting can abort the process, so it is not here. See
//! `depth_ladder.rs`.

#![expect(
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::format_collect,
    clippy::format_push_string,
    reason = "a broken promise must fail the test loudly; the generators \
              build sources with small numbers, and format reads best there"
)]

use formalang::{compile_to_ir, parse_only, report_errors, CompilerError, Location, Span};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The line and the column (both one-based, the column in characters)
/// of a byte offset, computed from the source alone.
fn expected_position(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

/// Check one location against the source. Return a description of
/// each problem.
fn location_problems(source: &str, what: &str, at: Location) -> Vec<String> {
    let mut problems = Vec::new();
    if at.offset > source.len() {
        problems.push(format!(
            "{what} offset {} is past the end ({})",
            at.offset,
            source.len()
        ));
        return problems;
    }
    if !source.is_char_boundary(at.offset) {
        problems.push(format!("{what} offset {} is inside a character", at.offset));
        return problems;
    }
    let (line, column) = expected_position(source, at.offset);
    if (at.line, at.column) != (line, column) {
        problems.push(format!(
            "{what} offset {} says line {} column {}, but the source says line {line} column {column}",
            at.offset, at.line, at.column
        ));
    }
    problems
}

/// Check one span against the source.
fn span_problems(source: &str, span: Span) -> Vec<String> {
    let mut problems = location_problems(source, "start", span.start);
    problems.extend(location_problems(source, "end", span.end));
    if span.start.offset > span.end.offset {
        problems.push(format!(
            "the span starts at {} and ends before it, at {}",
            span.start.offset, span.end.offset
        ));
    }
    problems
}

/// Compile `source` and require that it fails, that no error is an
/// internal error, and that each error has a good span.
fn assert_clean_rejection(name: &str, source: &str) -> Vec<CompilerError> {
    let Err(errors) = compile_to_ir(source) else {
        panic!("{name}: the program compiled, but it is not valid");
    };
    assert!(!errors.is_empty(), "{name}: a rejection came with no error");
    let mut problems = Vec::new();
    for error in &errors {
        if matches!(error, CompilerError::InternalError { .. }) {
            problems.push(format!("internal error: {error}"));
        }
        for p in span_problems(source, error.span()) {
            problems.push(format!("{error}: {p}"));
        }
    }
    assert!(
        problems.is_empty(),
        "{name}: bad errors:\n  {}",
        problems.join("\n  ")
    );
    // The renderer must accept every error too.
    let _ = report_errors(&errors, source, "input.fv");
    errors
}

/// The fastest of five runs of `f`. Other tests run at the same time,
/// and the fastest run is the one that they disturb least.
fn fastest(f: &dyn Fn()) -> Duration {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// Only one timing test measures at a time. When the timing tests run
/// in parallel, each one adds load to the others and the ratio drifts.
static TIMING: Mutex<()> = Mutex::new(());

/// Time `run` over `generate(n)` and `generate(16n)`. A linear phase
/// takes about 16 times as long, and a quadratic phase about 256 times.
/// Allow 48 times, plus a small constant. The gap between the two is
/// wide, so load from other processes does not make a linear phase
/// fail, and a quadratic phase still fails by a wide margin.
fn assert_linear(name: &str, n: usize, generate: fn(usize) -> String, run: fn(&str)) {
    let _turn = TIMING.lock().unwrap_or_else(PoisonError::into_inner);
    let small = generate(n);
    let large = generate(n * 16);
    // Measure the two sizes in turns, so that a spell of load from
    // other tests falls on both of them.
    let mut t_small = Duration::MAX;
    let mut t_large = Duration::MAX;
    for _ in 0..5 {
        t_small = t_small.min(fastest(&|| run(&small)));
        t_large = t_large.min(fastest(&|| run(&large)));
    }
    let bound = t_small * 48 + Duration::from_millis(40);
    assert!(
        t_large <= bound,
        "{name}: {n} took {t_small:?} and {} took {t_large:?}, which is more than 48 \
         times as long; the growth is not linear",
        n * 16
    );
}

fn compile(source: &str) {
    let _ = compile_to_ir(source);
}

fn parse(source: &str) {
    let _ = parse_only(source);
}

fn compile_and_report(source: &str) {
    if let Err(errors) = compile_to_ir(source) {
        let _ = report_errors(&errors, source, "input.fv");
    }
}

// ---------------------------------------------------------------------------
// Encodings and line endings
// ---------------------------------------------------------------------------

/// A UTF-8 byte order mark at the start of a file is not part of the
/// program. Editors on Windows write it. Rust, Go, Swift and
/// `TypeScript` all skip it.
#[test]
fn a_byte_order_mark_is_not_part_of_the_program() {
    let source = "\u{feff}pub let x: I32 = 1\n";
    let result = compile_to_ir(source);
    assert!(
        result.is_ok(),
        "a file that starts with a byte order mark did not compile: {:?}",
        result.err()
    );
}

/// CRLF line endings do not change what a program means.
#[test]
fn crlf_line_endings_compile_like_lf() {
    let lf =
        "pub struct P {\n    x: I32,\n    y: I32\n}\n\npub fn f(p: P) -> I32 {\n    p.x + p.y\n}\n";
    let crlf = lf.replace('\n', "\r\n");
    assert!(compile_to_ir(lf).is_ok(), "the LF form must compile");
    let result = compile_to_ir(&crlf);
    assert!(
        result.is_ok(),
        "the CRLF form did not compile: {:?}",
        result.err()
    );
}

/// An error in a CRLF file has the same line and column as the same
/// error in the LF file.
#[test]
fn crlf_errors_have_the_lf_positions() {
    let lf = "pub fn f() -> I32 {\n    let a = 1\n    let b: String = a\n    0\n}\n";
    let crlf = lf.replace('\n', "\r\n");
    let lf_errors = assert_clean_rejection("lf", lf);
    let crlf_errors = assert_clean_rejection("crlf", &crlf);
    let lines = |errors: &[CompilerError]| {
        errors
            .iter()
            .map(|e| (e.span().start.line, e.span().start.column))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lines(&lf_errors),
        lines(&crlf_errors),
        "the same error sits at another line or column in a CRLF file"
    );
}

/// A lone carriage return is a line ending in old Mac files. The line
/// numbers then must still count something sane: every span stays in
/// bounds.
#[test]
fn a_lone_carriage_return_does_not_break_positions() {
    let source = "pub fn f() -> I32 {\r    let b: String = 1\r    0\r}\r";
    let result = compile_to_ir(source);
    if let Err(errors) = result {
        for error in &errors {
            let span = error.span();
            assert!(
                span.start.offset <= source.len()
                    && span.end.offset <= source.len()
                    && source.is_char_boundary(span.start.offset)
                    && source.is_char_boundary(span.end.offset),
                "{error}: the span is not a real byte range"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Bytes that do not belong
// ---------------------------------------------------------------------------

/// A NUL byte outside a string is an error that points at it.
#[test]
fn a_nul_byte_is_an_error_at_the_nul() {
    let source = "pub let x: I32 = 1\u{0}\n";
    let errors = assert_clean_rejection("nul", source);
    let nul = source.find('\u{0}').unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.span().start.offset == nul),
        "no error points at the NUL byte (offset {nul}): {errors:?}"
    );
}

/// A bidirectional override in a comment makes the source show one
/// program and compile another (CVE-2021-42574, "Trojan Source").
/// rustc rejects it by default.
#[test]
fn a_bidirectional_override_in_a_comment_is_rejected() {
    let source = "pub fn f(admin: Boolean) -> Boolean {\n    \
                  /* \u{202e} } \u{2066}if admin\u{2069} \u{2066} begin */\n    admin\n}\n";
    assert_clean_rejection("bidi override in a comment", source);
}

/// The same in a string literal.
#[test]
fn a_bidirectional_override_in_a_string_is_rejected() {
    let source = "pub let s: String = \"user\u{202e}\u{2066}// admin\u{2069}\u{2066}\"\n";
    assert_clean_rejection("bidi override in a string", source);
}

// ---------------------------------------------------------------------------
// Input that stops too early
// ---------------------------------------------------------------------------

macro_rules! rejected_cleanly {
    ($($name:ident => $source:expr;)*) => {
        $(
            #[test]
            fn $name() {
                assert_clean_rejection(stringify!($name), $source);
            }
        )*
    };
}

rejected_cleanly! {
    unterminated_string_at_the_end => "pub let x = \"abc";
    unterminated_string_after_a_backslash => "pub let x = \"abc\\";
    unterminated_string_after_multibyte_text => "pub let x = \"h\u{e9}llo \u{4e2d}";
    unterminated_string_after_an_emoji => "pub let x = \"\u{1f600}";
    unterminated_block_comment_at_the_end => "pub let x: I32 = 1\n/* abc";
    unterminated_block_comment_after_a_star => "pub let x: I32 = 1\n/* abc *";
    unterminated_block_comment_after_multibyte_text => "/* \u{4e2d}\u{6587}";
    unterminated_unicode_escape => "pub let x = \"\\u12";
    empty_unicode_escape => "pub let x = \"\\u\"";
    surrogate_unicode_escape => "pub let x = \"\\uD800\"";
    low_surrogate_unicode_escape => "pub let x = \"\\uDFFF\"";
    unknown_escape => "pub let x = \"\\q\"";
    an_integer_beyond_i128 => "pub let x = 999999999999999999999999999999999999999999\n";
    an_integer_of_a_hundred_thousand_digits => &format!("pub let x = {}\n", "1".repeat(100_000));
    a_lone_operator_at_the_end => "pub let x: I32 = 1 +";
    a_lone_dot_at_the_end => "pub let x: I32 = 1.";
    an_open_generic_at_the_end => "pub let x: Array<";
    an_open_call_at_the_end => "pub fn f(a: I32) -> I32 { a }\npub let x = f(a: ";
    an_open_match_at_the_end => "pub fn f(a: I32) -> I32 { match a { ";
    an_open_closure_at_the_end => "pub let f = (a: I32) ->";
    a_keyword_alone => "pub";
    an_empty_struct_body_without_a_brace => "pub struct S {";
}

/// A string literal with a lone surrogate escape must not come out as
/// a string. A Rust `String` cannot hold a surrogate, so the only other
/// outcome is a replacement character: the program then holds another
/// value than the source says.
#[test]
fn a_surrogate_escape_does_not_compile_to_a_replacement_character() {
    let source = "pub let x: String = \"\\uD83D\\uDE00\"\n";
    if let Ok(module) = compile_to_ir(source) {
        let json = serde_json::to_string(&module).unwrap_or_default();
        assert!(
            !json.contains('\u{fffd}') && !json.contains("\\ufffd"),
            "a surrogate pair escape compiled to a replacement character"
        );
    }
}

// ---------------------------------------------------------------------------
// Positions of errors in text with tabs and wide characters
// ---------------------------------------------------------------------------

/// Programs with one mistake each, around text that makes columns hard
/// to count.
const BROKEN_WITH_WIDE_TEXT: &[(&str, &str)] = &[
    (
        "tab indent",
        "pub fn f() -> I32 {\n\tlet s: String = 1\n\t0\n}\n",
    ),
    (
        "multibyte before the error",
        "// h\u{e9}llo \u{4e2d}\u{6587}\npub let \u{e9}t\u{e9}: I32 = \"x\"\n",
    ),
    (
        "emoji in a string before the error",
        "pub let s = \"\u{1f600}\u{1f600}\" + 1\n",
    ),
    ("error at the last byte", "pub let x: I32 = y"),
    (
        "error after a combining mark",
        "pub let e\u{301}: I32 = true\n",
    ),
    (
        "undefined name after CRLF",
        "pub let a: I32 = 1\r\npub let b: I32 = zz\r\n",
    ),
    ("an unknown character", "pub let a: I32 = 1 \u{a7} 2\n"),
    (
        "an unknown multibyte character",
        "pub let a: I32 = 1 \u{2603} 2\n",
    ),
    ("a stray backtick", "pub let a: I32 = `1`\n"),
    ("a stray hash", "pub let a: I32 = #1\n"),
];

/// Every error in those programs points at a real place.
#[test]
fn error_positions_are_right_around_wide_text() {
    let mut problems = Vec::new();
    for (name, source) in BROKEN_WITH_WIDE_TEXT {
        let Err(errors) = compile_to_ir(source) else {
            problems.push(format!("{name}: the program compiled"));
            continue;
        };
        for error in &errors {
            for p in span_problems(source, error.span()) {
                problems.push(format!("{name}: {error}: {p}"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "bad positions:\n  {}",
        problems.join("\n  ")
    );
}

/// An error that is not at the start of the file must not point at
/// line 1 column 1. A span without a position is the default span, and
/// the user then sees the error on the first line.
#[test]
fn no_error_falls_back_to_the_first_character() {
    let cases: &[&str] = &[
        "\n\n\npub let x: I32 = \"s\"\n",
        "\n\n\npub fn f() -> I32 { true }\n",
        "\n\n\npub struct S { a: I32 }\npub let s = S(a: 1, b: 2)\n",
        "\n\n\npub enum E { a }\npub let e: E = E.b\n",
        "\n\n\npub fn f(a: I32) -> I32 { a }\npub let x: I32 = f(a: \"s\")\n",
        "\n\n\npub let x: [I32] = [1, \"a\"]\n",
        "\n\n\npub let x = 1\npub let x = 2\n",
        "\n\n\npub trait T { fn m(self) -> I32 }\npub struct S { a: I32 }\nimpl T for S {}\n",
        "\n\n\npub fn f() -> I32 {\n    let a = 1\n    a = 2\n    a\n}\n",
        "\n\n\npub let x: Missing = 1\n",
        "\n\n\nuse nowhere::thing\n",
    ];
    let mut problems = Vec::new();
    for source in cases {
        let Err(errors) = compile_to_ir(source) else {
            problems.push(format!("compiled: {source:?}"));
            continue;
        };
        for error in &errors {
            let span = error.span();
            if span.start.offset < 3 || span.start.line <= 3 {
                problems.push(format!(
                    "{error} points at line {} offset {} in {source:?}",
                    span.start.line, span.start.offset
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "errors without a real position:\n  {}",
        problems.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// A one-megabyte identifier compiles, and fast.
#[test]
fn a_very_long_identifier_compiles_quickly() {
    let name = "a".repeat(1 << 20);
    let source = format!("pub let {name}: I32 = 1\npub let b: I32 = {name}\n");
    let start = Instant::now();
    let result = compile_to_ir(&source);
    let took = start.elapsed();
    assert!(
        result.is_ok(),
        "the program did not compile: {:?}",
        result.err()
    );
    assert!(took < Duration::from_secs(10), "it took {took:?}");
}

/// Ten thousand broken lines give errors in bounded time, and each
/// error has a good span.
#[test]
fn ten_thousand_broken_lines_are_reported_quickly() {
    let source = "pub let x: I32 = \"s\"\n".repeat(10_000);
    let start = Instant::now();
    let result = compile_to_ir(&source);
    let took = start.elapsed();
    assert!(took < Duration::from_secs(20), "it took {took:?}");
    let Err(errors) = result else {
        panic!("ten thousand duplicate, ill-typed lets compiled");
    };
    for error in errors.iter().take(200) {
        assert!(
            span_problems(&source, error.span()).is_empty(),
            "{error}: bad span"
        );
    }
}

// ---------------------------------------------------------------------------
// Growth
// ---------------------------------------------------------------------------

fn many_lets(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub let v{i}: I32 = {i}\n"))
        .collect()
}

fn many_functions(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub fn f{i}(a: I32) -> I32 {{ a + {i} }}\n"))
        .collect()
}

fn function_call_chain(n: usize) -> String {
    let mut s = String::from("pub fn f0(a: I32) -> I32 { a }\n");
    for i in 1..n {
        s.push_str(&format!(
            "pub fn f{i}(a: I32) -> I32 {{ f{}(a: a) }}\n",
            i - 1
        ));
    }
    s
}

fn many_fields(n: usize) -> String {
    let fields: String = (0..n).map(|i| format!("    f{i}: I32,\n")).collect();
    format!("pub struct S {{\n{fields}}}\n")
}

fn many_variants(n: usize) -> String {
    let variants: String = (0..n).map(|i| format!("    v{i},\n")).collect();
    format!("pub enum E {{\n{variants}}}\n")
}

fn many_match_arms(n: usize) -> String {
    let variants: String = (0..n).map(|i| format!("v{i}, ")).collect();
    let arms: String = (0..n).map(|i| format!("        .v{i}: {i},\n")).collect();
    format!(
        "pub enum E {{ {variants} }}\npub fn f(e: E) -> I32 {{\n    match e {{\n{arms}    }}\n}}\n"
    )
}

fn many_statements(n: usize) -> String {
    let body: String = (0..n).map(|i| format!("    let v{i} = {i}\n")).collect();
    format!("pub fn f() -> I32 {{\n{body}    0\n}}\n")
}

fn many_methods(n: usize) -> String {
    let methods: String = (0..n)
        .map(|i| format!("    fn m{i}(self) -> I32 {{ self.a + {i} }}\n"))
        .collect();
    format!("pub struct S {{ a: I32 }}\nimpl S {{\n{methods}}}\n")
}

fn many_parameters(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("p{i}: I32")).collect();
    format!("pub fn f({}) -> I32 {{ p0 }}\n", params.join(", "))
}

fn many_arguments(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("p{i}: I32")).collect();
    let args: Vec<String> = (0..n).map(|i| format!("p{i}: {i}")).collect();
    format!(
        "pub fn f({}) -> I32 {{ p0 }}\npub let x: I32 = f({})\n",
        params.join(", "),
        args.join(", ")
    )
}

fn long_array_literal(n: usize) -> String {
    let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    format!("pub let a: [I32] = [{}]\n", items.join(", "))
}

fn long_dictionary_literal(n: usize) -> String {
    let items: Vec<String> = (0..n).map(|i| format!("\"k{i}\": {i}")).collect();
    format!("pub let d: [String: I32] = [{}]\n", items.join(", "))
}

fn many_generic_instantiations(n: usize) -> String {
    let mut s = String::from("pub struct Box<T> { v: T }\npub fn id<T>(x: T) -> T { x }\n");
    for i in 0..n {
        s.push_str(&format!("pub let b{i}: I32 = id(x: Box(v: {i})).v\n"));
    }
    s
}

fn many_closures(n: usize) -> String {
    let body: String = (0..n)
        .map(|i| format!("    let c{i} = (a: I32) -> a + {i}\n"))
        .collect();
    format!("pub fn f() -> I32 {{\n{body}    0\n}}\n")
}

fn many_comment_lines(n: usize) -> String {
    let mut s = "// a comment line of some length\n".repeat(n);
    s.push_str("pub let x: I32 = 1\n");
    s
}

/// A tuple that holds a tuple, `n` levels deep. A closure and a tuple
/// both start with `(name:`, and the parser once read the rest of the
/// nest again at each level.
fn nested_tuples(n: usize) -> String {
    format!("pub let x = {}1{}\n", "(a: ".repeat(n), ")".repeat(n))
}

fn many_type_errors(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub let v{i}: I32 = \"s\"\n"))
        .collect()
}

fn many_parse_errors(n: usize) -> String {
    "pub let = = 1\n".repeat(n)
}

fn many_undefined_names(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub let v{i}: I32 = missing{i}\n"))
        .collect()
}

fn many_structs_referencing_the_previous(n: usize) -> String {
    let mut s = String::from("pub struct S0 { a: I32 }\n");
    for i in 1..n {
        s.push_str(&format!("pub struct S{i} {{ a: S{} }}\n", i - 1));
    }
    s
}

fn many_modules(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub mod m{i} {{\n    pub let x: I32 = {i}\n}}\n"))
        .collect()
}

fn many_trait_impls(n: usize) -> String {
    let mut s = String::from("pub trait T { fn m(self) -> I32 }\n");
    for i in 0..n {
        s.push_str(&format!(
            "pub struct S{i} {{ a: I32 }}\nimpl T for S{i} {{\n    fn m(self) -> I32 {{ self.a }}\n}}\n"
        ));
    }
    s
}

macro_rules! linear {
    ($($name:ident => $n:expr, $generate:ident, $run:ident;)*) => {
        $(
            #[test]
            fn $name() {
                assert_linear(stringify!($generate), $n, $generate, $run);
            }
        )*
    };
}

linear! {
    compiling_lets_is_linear => 125, many_lets, compile;
    compiling_functions_is_linear => 75, many_functions, compile;
    compiling_a_call_chain_is_linear => 50, function_call_chain, compile;
    compiling_struct_fields_is_linear => 125, many_fields, compile;
    compiling_enum_variants_is_linear => 125, many_variants, compile;
    compiling_match_arms_is_linear => 50, many_match_arms, compile;
    compiling_statements_is_linear => 75, many_statements, compile;
    compiling_methods_is_linear => 50, many_methods, compile;
    compiling_parameters_is_linear => 75, many_parameters, compile;
    compiling_arguments_is_linear => 75, many_arguments, compile;
    compiling_array_literals_is_linear => 250, long_array_literal, compile;
    compiling_dictionary_literals_is_linear => 125, long_dictionary_literal, compile;
    compiling_generic_instantiations_is_linear => 25, many_generic_instantiations, compile;
    compiling_closures_is_linear => 50, many_closures, compile;
    compiling_comments_is_linear => 500, many_comment_lines, compile;
    compiling_struct_chains_is_linear => 50, many_structs_referencing_the_previous, compile;
    compiling_modules_is_linear => 50, many_modules, compile;
    compiling_trait_impls_is_linear => 25, many_trait_impls, compile;
    parsing_lets_is_linear => 250, many_lets, parse;
    parsing_nested_tuples_is_linear => 60, nested_tuples, parse;
    parsing_parse_errors_is_linear => 125, many_parse_errors, parse;
    reporting_type_errors_is_linear => 75, many_type_errors, compile_and_report;
    reporting_parse_errors_is_linear => 75, many_parse_errors, compile_and_report;
    reporting_undefined_names_is_linear => 75, many_undefined_names, compile_and_report;
}

/// Parse time must not grow with the nesting depth faster than the
/// input does. `TESTING.md` names this defect FL-1: it was exponential.
///
/// The depth stays low, so the parse does not run out of stack, and it
/// runs on a large stack for the same reason.
fn assert_nesting_is_linear(name: &'static str, generate: fn(usize) -> String) {
    crate::common::with_a_large_stack(move || {
        let t = |depth: usize| {
            let source = generate(depth);
            fastest(&|| parse(&source))
        };
        let t_small = t(8);
        let t_large = t(32);
        assert!(
            t_large <= t_small * 8 + Duration::from_millis(40),
            "{name}: depth 8 took {t_small:?} and depth 32 took {t_large:?}"
        );
    });
}

#[test]
fn parse_time_is_linear_in_paren_depth() {
    assert_nesting_is_linear("parens", |n| {
        format!("pub let x: I32 = {}1{}\n", "(".repeat(n), ")".repeat(n))
    });
}

#[test]
fn parse_time_is_linear_in_array_depth() {
    assert_nesting_is_linear("arrays", |n| {
        format!("pub let x = {}1{}\n", "[".repeat(n), "]".repeat(n))
    });
}

#[test]
fn parse_time_is_linear_in_if_depth() {
    assert_nesting_is_linear("ifs", |n| {
        format!(
            "pub let x: I32 = {}1{}\n",
            "if true { ".repeat(n),
            " } else { 2 }".repeat(n)
        )
    });
}

#[test]
fn parse_time_is_linear_in_call_depth() {
    assert_nesting_is_linear("calls", |n| {
        format!("pub let x: I32 = {}1{}\n", "f(v: ".repeat(n), ")".repeat(n))
    });
}

#[test]
fn parse_time_is_linear_in_closure_depth() {
    assert_nesting_is_linear("closures", |n| {
        format!("pub let x = {}1\n", "(a: I32) -> ".repeat(n))
    });
}

#[test]
fn parse_time_is_linear_in_generic_type_depth() {
    assert_nesting_is_linear("generic types", |n| {
        format!(
            "pub let x: {}I32{} = nil\n",
            "Box<".repeat(n),
            ">".repeat(n)
        )
    });
}

#[test]
fn parse_time_is_linear_in_tuple_depth() {
    assert_nesting_is_linear("tuples", |n| {
        format!("pub let x = {}1{}\n", "(a: ".repeat(n), ")".repeat(n))
    });
}

#[test]
fn parse_time_is_linear_in_block_depth() {
    assert_nesting_is_linear("blocks", |n| {
        format!(
            "pub fn f() -> I32 {{\n    {}1{}\n}}\n",
            "{ ".repeat(n),
            " }".repeat(n)
        )
    });
}

/// A nest of `if` blocks that are not closed must be refused in time
/// that grows with the input. `FOUND_DEFECTS.md` item 5: each unclosed
/// `if` made the refusal three times slower.
#[test]
fn refusing_unclosed_ifs_is_linear() {
    let generate = |n: usize| -> String {
        format!(
            "pub fn answer() -> I32 {{\n{}",
            "let d = if v { let v = v * 2".repeat(n)
        )
    };
    // A small input overflows a test thread's stack. `depth_ladder.rs`
    // holds that defect; this test measures time only, so it runs on a
    // large stack.
    let (t_small, t_large) = crate::common::with_a_large_stack(move || {
        let small = generate(4);
        let large = generate(8);
        (fastest(&|| compile(&small)), fastest(&|| compile(&large)))
    });
    assert!(
        t_large <= t_small * 4 + Duration::from_millis(40),
        "4 unclosed ifs took {t_small:?} and 8 took {t_large:?}; the input only \
         doubled"
    );
}
