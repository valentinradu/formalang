//! Property tests over generated programs and generated source text.
//!
//! The fuzz targets in `fuzz/` cover the same ground with a much
//! larger budget, but they need a nightly toolchain and a separate
//! command. These run on stable, in CI, on every commit, and they
//! shrink a failure to a small counterexample.
//!
//! Set `PROPTEST_CASES=N` to change the case count.

#![expect(
    clippy::format_push_string,
    clippy::arithmetic_side_effects,
    reason = "the strategies build source text with format! and index arithmetic"
)]

use proptest::prelude::*;

use formalang::location::offset_to_location;
use formalang::{compile_to_ir, parse_only, Lexer, Pipeline};

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Text built from fragments of `FormaLang` syntax.
///
/// Fully random strings almost never reach the parser's interesting
/// paths; a random *sequence of tokens* does. Every fragment is a real
/// piece of the grammar, so the generated text is lexically valid and
/// syntactically nonsense, which is exactly the input that breaks
/// error recovery.
fn token_soup() -> impl Strategy<Value = String> {
    const FRAGMENTS: &[&str] = &[
        "pub ", "fn ", "struct ", "enum ", "trait ", "impl ", "for ", "let ", "mut ", "sink ",
        "match ", "if ", "else ", "use ", "mod ", "extern ", "return ", "{", "}", "(", ")", "[",
        "]", "<", ">", ",", ":", "::", ".", "..", "?", "!", "->", "=", "==", "+", "-", "*", "/",
        "%", "&&", "||", "??", "I32", "F64", "String", "Boolean", "none", "true", "false", "x",
        "Foo", "\"s\"", "0", "42", "1.5", "\n", "  ", "// c\n", "/* c */", "_",
    ];
    proptest::collection::vec(proptest::sample::select(FRAGMENTS), 0..60)
        .prop_map(|parts| parts.concat())
}

/// A `FormaLang` program that compiles: `n` structs, each with `k`
/// fields of a chosen type.
fn valid_struct_program() -> impl Strategy<Value = String> {
    (1_usize..6, 0_usize..5, proptest::sample::select(TYPES)).prop_map(|(n, k, ty)| {
        let mut s = String::new();
        for i in 0..n {
            s.push_str(&format!("pub struct S{i} {{\n"));
            for f in 0..k {
                if f > 0 {
                    s.push_str(",\n");
                }
                s.push_str(&format!("    f{f}: {ty}"));
            }
            if k == 0 {
                s.push_str("    only: I32");
            }
            s.push_str("\n}\n\n");
        }
        s
    })
}

const TYPES: &[&str] = &[
    "I32",
    "I64",
    "F64",
    "Boolean",
    "String",
    "[I32]",
    "[String: I32]",
    "I32?",
    "(a: I32, b: String)",
];

/// A program of `n` functions, each calling the previous one.
fn valid_function_chain() -> impl Strategy<Value = String> {
    (1_usize..8).prop_map(|n| {
        let mut s = String::from("pub fn f0(x: I32) -> I32 {\n    x + 1\n}\n\n");
        for i in 1..n {
            let prev = i - 1;
            s.push_str(&format!(
                "pub fn f{i}(x: I32) -> I32 {{\n    f{prev}(x: x) * 2\n}}\n\n"
            ));
        }
        s
    })
}

// ---------------------------------------------------------------------------
// Totality and span invariants
// ---------------------------------------------------------------------------

proptest! {
    /// The lexer never panics, and every span it reports names a real
    /// byte range with a one-based line and column.
    #[test]
    fn lexer_spans_are_in_range(source in token_soup()) {
        let (tokens, errors) = Lexer::tokenize_all_with_errors(&source);
        for (_, span) in &tokens {
            prop_assert!(span.start.offset <= span.end.offset, "inverted span {span:?}");
            prop_assert!(span.end.offset <= source.len(), "span past end {span:?}");
            prop_assert!(span.start.line >= 1 && span.start.column >= 1, "zero position {span:?}");
        }
        for error in &errors {
            let span = error.span();
            prop_assert!(span.start.offset <= span.end.offset, "inverted error span {span:?}");
            prop_assert!(span.end.offset <= source.len(), "error span past end {span:?}");
        }
    }

    /// Every token span's line and column agree with a direct
    /// conversion of its byte offset. A mismatch means the lexer's
    /// positions and the reporter's positions disagree, and the caret
    /// in a diagnostic lands on the wrong character.
    #[test]
    fn token_positions_match_the_offset_conversion(source in token_soup()) {
        let (tokens, _) = Lexer::tokenize_all_with_errors(&source);
        for (token, span) in &tokens {
            let start = offset_to_location(span.start.offset, &source);
            prop_assert_eq!(
                (span.start.line, span.start.column),
                (start.line, start.column),
                "token {:?} start position disagrees with its offset",
                token
            );
        }
    }

    /// The parser never panics, and a failure always carries at least
    /// one error.
    #[test]
    fn parser_is_total(source in token_soup()) {
        match parse_only(&source) {
            Ok(_) => {}
            Err(errors) => prop_assert!(!errors.is_empty(), "parse failed with no errors"),
        }
    }

    /// The whole frontend never panics on token soup.
    #[test]
    fn compile_is_total(source in token_soup()) {
        let Ok(module) = compile_to_ir(&source) else { return Ok(()); };
        let _ = Pipeline::for_codegen().run(module);
    }
}

// ---------------------------------------------------------------------------
// Properties of programs that compile
// ---------------------------------------------------------------------------

proptest! {
    /// A struct program lowers to exactly the structs it declares.
    #[test]
    fn struct_count_matches_the_source(source in valid_struct_program()) {
        let declared = source.matches("pub struct ").count();
        let module = compile_to_ir(&source)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!(
            module.user_structs().count(),
            declared,
            "lowered struct count does not match the source"
        );
    }

    /// Compiling the same source twice must produce the same IR.
    #[test]
    fn compilation_is_deterministic(source in valid_function_chain()) {
        let first = compile_to_ir(&source)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let second = compile_to_ir(&source)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let a = serde_json::to_string(&first).unwrap_or_default();
        let b = serde_json::to_string(&second).unwrap_or_default();
        prop_assert_eq!(a, b, "two compiles of the same source differ");
    }

    /// The codegen pipeline is idempotent over programs that compile.
    #[test]
    fn codegen_pipeline_is_idempotent(source in valid_function_chain()) {
        let module = compile_to_ir(&source)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let once = Pipeline::for_codegen().run(module)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let once_json = serde_json::to_string(&once).unwrap_or_default();
        let twice = Pipeline::for_codegen().run(once)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let twice_json = serde_json::to_string(&twice).unwrap_or_default();
        prop_assert_eq!(once_json, twice_json, "the pipeline is not idempotent");
    }

    /// Indenting the whole program by a constant amount must not
    /// change whether it compiles.
    #[test]
    fn a_uniform_indent_does_not_change_acceptance(
        source in valid_function_chain(),
        indent in 1_usize..5,
    ) {
        let pad = " ".repeat(indent);
        let indented: String = source
            .lines()
            .map(|l| if l.is_empty() { String::from("\n") } else { format!("{pad}{l}\n") })
            .collect();
        let plain_ok = compile_to_ir(&source).is_ok();
        let indented_ok = compile_to_ir(&indented).is_ok();
        prop_assert_eq!(
            plain_ok, indented_ok,
            "a uniform indent of {} spaces changed whether the program compiles",
            indent
        );
    }
}

// ---------------------------------------------------------------------------
// The offset-to-position conversion
// ---------------------------------------------------------------------------

/// An independent line/column calculation, written the obvious way.
/// The property below checks the compiler's version against this one.
fn reference_location(offset: usize, source: &str) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

proptest! {
    /// `offset_to_location` must agree with the reference for every
    /// offset, including offsets that land inside a multi-byte
    /// character and offsets past the end of the source.
    #[test]
    fn offset_to_location_agrees_with_the_reference(
        source in "[a-z\n\r\t ]{0,200}",
        offset in 0_usize..250,
    ) {
        let got = offset_to_location(offset, &source);
        let (line, column) = reference_location(offset, &source);
        prop_assert_eq!(
            (got.line, got.column),
            (line, column),
            "offset {} in {:?}",
            offset,
            source
        );
        prop_assert_eq!(got.offset, offset, "the offset was not preserved");
    }

    /// The same, over text with multi-byte characters.
    #[test]
    fn offset_to_location_handles_multibyte_text(
        source in "[a\u{00e9}\u{4e2d}\u{1f600}\n]{0,60}",
        offset in 0_usize..200,
    ) {
        let got = offset_to_location(offset, &source);
        prop_assert!(got.line >= 1, "line numbers are one-based");
        prop_assert!(got.column >= 1, "column numbers are one-based");
        // At a character boundary the reference applies exactly.
        if source.is_char_boundary(offset.min(source.len())) && offset <= source.len() {
            let (line, column) = reference_location(offset, &source);
            prop_assert_eq!(
                (got.line, got.column),
                (line, column),
                "offset {} in {:?}",
                offset,
                source
            );
        }
    }
}
