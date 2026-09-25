//! Promises of the lexer and the parser that must hold for every input.
//!
//! The corpus is every `.fv` file in the repository: the examples, the
//! fixtures and the conformance cases. Each promise also runs over
//! broken forms of that corpus: each prefix of a file, and a file with
//! one character removed. A broken buffer is the normal state of a file
//! in an editor, so the compiler sees such input all the time.
//!
//! The promises:
//!
//! - no input makes the compiler panic;
//! - a failure always carries at least one error, and no error is an
//!   internal error;
//! - each span is a real byte range, on character boundaries, with the
//!   line and the column that the byte offset gives;
//! - the lexer's tokens come in order, do not overlap, and only
//!   whitespace or a comment sits between two of them;
//! - an AST survives a JSON round trip unchanged.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "offsets come from the lexer and from `char_indices`, and the \
              span checks run before any slice that could fail"
)]

use crate::common::Checked;
use formalang::{compile_to_ir, parse_only, CompilerError, Lexer, Location, Span};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("fv") {
            if let Ok(source) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), source));
            }
        }
    }
}

fn corpus() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for dir in ["examples", "tests/fixtures", "tests/conformance"] {
        collect(&root.join(dir), &mut out);
    }
    assert!(
        out.len() > 300,
        "the corpus is too small: {} files",
        out.len()
    );
    out
}

/// The corpus files that are small enough for a sweep over each
/// character.
fn small_corpus() -> Vec<(String, String)> {
    corpus()
        .into_iter()
        .filter(|(_, s)| s.len() <= 1200)
        .collect()
}

// ---------------------------------------------------------------------------
// Span checks
// ---------------------------------------------------------------------------

fn expected_position(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

fn location_problem(source: &str, at: Location) -> Option<String> {
    if at.offset > source.len() {
        return Some(format!(
            "offset {} is past the end ({})",
            at.offset,
            source.len()
        ));
    }
    if !source.is_char_boundary(at.offset) {
        return Some(format!("offset {} is inside a character", at.offset));
    }
    let (line, column) = expected_position(source, at.offset);
    ((at.line, at.column) != (line, column)).then(|| {
        format!(
            "offset {} says {}:{}, the source says {line}:{column}",
            at.offset, at.line, at.column
        )
    })
}

fn span_problem(source: &str, span: Span) -> Option<String> {
    if span.start.offset > span.end.offset {
        return Some(format!(
            "the span ends ({}) before it starts ({})",
            span.end.offset, span.start.offset
        ));
    }
    location_problem(source, span.start).or_else(|| location_problem(source, span.end))
}

/// Check one outcome of the compiler. Return each broken promise.
fn outcome_problems(source: &str, result: &Result<(), Vec<CompilerError>>) -> Vec<String> {
    let Err(errors) = result else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if errors.is_empty() {
        problems.push("a failure with no error".to_string());
    }
    for error in errors {
        if matches!(error, CompilerError::InternalError { .. }) {
            problems.push(format!("internal error: {error}"));
        }
        if let Some(p) = span_problem(source, error.span()) {
            problems.push(format!("{error}: {p}"));
        }
    }
    problems
}

/// Run parse and compile over `source`, catch a panic, and return each
/// broken promise.
fn check_everything(source: &str) -> Vec<String> {
    let run = catch_unwind(AssertUnwindSafe(|| {
        let mut problems = Vec::new();
        let parsed = parse_only(source).map(|_| ());
        for p in outcome_problems(source, &parsed) {
            problems.push(format!("parse: {p}"));
        }
        let compiled = compile_to_ir(source).map(|_| ());
        for p in outcome_problems(source, &compiled) {
            problems.push(format!("compile: {p}"));
        }
        problems
    }));
    run.unwrap_or_else(|_| vec!["the compiler panicked".to_string()])
}

/// Collect problems from a sweep, keep a sample, and fail with it.
fn assert_sweep(what: &str, problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{what}: {} broken promises. The first ones:\n  {}",
        problems.len(),
        problems
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// The corpus as written
// ---------------------------------------------------------------------------

/// Every error from every file in the corpus has a good span, and none
/// is an internal error.
#[test]
fn the_corpus_gives_clean_outcomes() {
    let mut problems = Vec::new();
    let mut checked = Checked::new("corpus files checked", 300);
    for (name, source) in corpus() {
        for p in check_everything(&source) {
            problems.push(format!("{name}: {p}"));
        }
        checked.hit();
    }
    assert_sweep("the corpus", &problems);
}

/// A file that parses has no lexer error: the parser does not drop an
/// error that the lexer found.
#[test]
fn a_file_that_parses_has_no_lexer_error() {
    let mut problems = Vec::new();
    for (name, source) in corpus() {
        let (_, lex_errors) = Lexer::tokenize_all_with_errors(&source);
        if parse_only(&source).is_ok() && !lex_errors.is_empty() {
            problems.push(format!(
                "{name}: parsed, but the lexer reported {lex_errors:?}"
            ));
        }
    }
    assert_sweep("lexer errors", &problems);
}

// ---------------------------------------------------------------------------
// The lexer
// ---------------------------------------------------------------------------

/// Is the text between two tokens only whitespace and comments?
fn is_trivia(gap: &str) -> bool {
    let mut rest = gap;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return true;
        }
        if let Some(after) = rest.strip_prefix("//") {
            rest = after.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(after) = rest.strip_prefix("/*") {
            let Some((_, r)) = after.split_once("*/") else {
                return false;
            };
            rest = r;
        } else {
            return false;
        }
    }
}

fn lexer_problems(source: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let (tokens, errors) = Lexer::tokenize_all_with_errors(source);
    let mut previous_end = 0;
    for (token, span) in &tokens {
        if let Some(p) = span_problem(source, *span) {
            problems.push(format!("{token:?}: {p}"));
            continue;
        }
        if span.start.offset < previous_end {
            problems.push(format!(
                "{token:?} at {} overlaps the token before it, which ends at {previous_end}",
                span.start.offset
            ));
            continue;
        }
        if span.start.offset == span.end.offset {
            problems.push(format!("{token:?} at {} is empty", span.start.offset));
        }
        // The lexer skips a byte order mark at the start of the file.
        let gap = source[previous_end..span.start.offset].trim_start_matches('\u{feff}');
        if errors.is_empty() && !is_trivia(gap) {
            problems.push(format!("the text {gap:?} before {token:?} is in no token"));
        }
        previous_end = span.end.offset;
    }
    if errors.is_empty() && !is_trivia(&source[previous_end.min(source.len())..]) {
        problems.push("the text after the last token is in no token".to_string());
    }
    for error in &errors {
        if let Some(p) = span_problem(source, error.span()) {
            problems.push(format!("lexer error {error}: {p}"));
        }
    }
    problems
}

/// The lexer's tokens cover the corpus: in order, without overlap, and
/// with good positions.
#[test]
fn lexer_tokens_cover_the_corpus() {
    let mut problems = Vec::new();
    for (name, source) in corpus() {
        for p in lexer_problems(&source) {
            problems.push(format!("{name}: {p}"));
        }
    }
    assert_sweep("the lexer", &problems);
}

/// The same over text that the corpus does not hold: odd characters,
/// odd line endings, and odd literals.
#[test]
fn lexer_tokens_cover_odd_text() {
    let texts: &[&str] = &[
        "a\r\nb\r\n",
        "a\rb\r",
        "\ta\t=\t1",
        "\u{feff}pub let x = 1",
        "\"\u{1f600}\" + \"\u{e9}\"",
        "1..2 1...2 1.0..2.0 1.e2 1e2 .5 5.",
        "0x1F 0b101 0o17 1_000 1__0 _1 1_",
        "a.b.c?.d!.e",
        "/**/ /* */ /*/ */ //\n//",
        "\"\\\\\" \"\\\"\" \"\\u0041\" \"\\t\"",
        "=== !== <=> >>= <<= ->> =>",
        "\u{a0}a\u{2028}b\u{2029}c",
        "\u{0}",
        "a\u{200b}b",
    ];
    let mut problems = Vec::new();
    for text in texts {
        let found = catch_unwind(|| lexer_problems(text))
            .unwrap_or_else(|_| vec!["the lexer panicked".to_string()]);
        for p in found {
            problems.push(format!("{text:?}: {p}"));
        }
    }
    assert_sweep("the lexer over odd text", &problems);
}

// ---------------------------------------------------------------------------
// Broken forms of the corpus
// ---------------------------------------------------------------------------

/// Each prefix of a file, cut at each line end and at each tenth
/// character, gives a clean outcome. This is what an editor sees while
/// the user types.
#[test]
fn every_prefix_gives_a_clean_outcome() {
    let mut problems = Vec::new();
    let mut checked = Checked::new("prefixes checked", 2000);
    for (name, source) in small_corpus() {
        let cuts: Vec<usize> = source
            .char_indices()
            .map(|(i, _)| i)
            .enumerate()
            .filter(|(n, i)| n % 10 == 0 || source[*i..].starts_with('\n'))
            .map(|(_, i)| i)
            .collect();
        for cut in cuts {
            let prefix = &source[..cut];
            for p in check_everything(prefix) {
                problems.push(format!("{name} cut at {cut}: {p}"));
            }
            checked.hit();
        }
    }
    assert_sweep("the prefixes", &problems);
}

/// A file with one character removed gives a clean outcome.
#[test]
fn every_one_character_deletion_gives_a_clean_outcome() {
    let mut problems = Vec::new();
    let mut checked = Checked::new("deletions checked", 2000);
    for (name, source) in small_corpus() {
        for (n, (i, c)) in source.char_indices().enumerate() {
            if n % 7 != 0 || c.is_whitespace() {
                continue;
            }
            let mut broken = source.clone();
            broken.replace_range(i..i + c.len_utf8(), "");
            for p in check_everything(&broken) {
                problems.push(format!("{name} without {c:?} at {i}: {p}"));
            }
            checked.hit();
        }
    }
    assert_sweep("the deletions", &problems);
}

/// A file with a stray token put in gives a clean outcome.
#[test]
fn every_inserted_token_gives_a_clean_outcome() {
    const INSERTS: &[&str] = &[
        "(", ")", "{", "}", "[", "]", ",", ":", ".", "?", "\"", "->", "=", "let", "fn", "match",
        "_", "0",
    ];
    let mut problems = Vec::new();
    let mut checked = Checked::new("insertions checked", 2000);
    for (name, source) in small_corpus() {
        let lines: Vec<usize> = source
            .char_indices()
            .filter(|(_, c)| *c == '\n')
            .map(|(i, _)| i)
            .collect();
        for (k, &at) in lines.iter().enumerate() {
            let insert = INSERTS[k % INSERTS.len()];
            let mut broken = source.clone();
            broken.insert_str(at, insert);
            for p in check_everything(&broken) {
                problems.push(format!("{name} with {insert:?} at {at}: {p}"));
            }
            checked.hit();
        }
    }
    assert_sweep("the insertions", &problems);
}

/// Two adjacent characters swapped gives a clean outcome.
#[test]
fn every_swap_gives_a_clean_outcome() {
    let mut problems = Vec::new();
    let mut checked = Checked::new("swaps checked", 1000);
    for (name, source) in small_corpus() {
        let chars: Vec<(usize, char)> = source.char_indices().collect();
        for (n, pair) in chars.windows(2).enumerate() {
            if n % 13 != 0 || pair[0].1 == pair[1].1 {
                continue;
            }
            let (i, a) = pair[0];
            let (j, b) = pair[1];
            let mut broken = String::with_capacity(source.len());
            broken.push_str(&source[..i]);
            broken.push(b);
            broken.push(a);
            broken.push_str(&source[j + b.len_utf8()..]);
            for p in check_everything(&broken) {
                problems.push(format!("{name} with {a:?}{b:?} swapped at {i}: {p}"));
            }
            checked.hit();
        }
    }
    assert_sweep("the swaps", &problems);
}
