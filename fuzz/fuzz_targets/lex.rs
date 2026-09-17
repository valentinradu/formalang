//! Lexer totality.
//!
//! `Lexer::tokenize_all_with_errors` reports every lexical problem as a
//! `CompilerError`. It must never panic, and every span it reports must
//! be a valid byte range inside the source.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::{Lexer, Span};

fn check_span(span: Span, len: usize, source: &str, what: &str) {
    assert!(
        span.start.offset <= span.end.offset,
        "{what} span is inverted: {span:?} in {source:?}"
    );
    assert!(
        span.end.offset <= len,
        "{what} span runs past the source: {span:?} > {len} in {source:?}"
    );
    assert!(
        span.start.line >= 1 && span.end.line >= 1,
        "{what} span has a zero line number: {span:?} in {source:?}"
    );
    assert!(
        span.start.column >= 1 && span.end.column >= 1,
        "{what} span has a zero column number: {span:?} in {source:?}"
    );
}

fuzz_target!(|source: &str| {
    let (tokens, errors) = Lexer::tokenize_all_with_errors(source);

    let len = source.len();
    for (_, span) in &tokens {
        check_span(*span, len, source, "token");
    }
    for error in &errors {
        check_span(error.span(), len, source, "error");
    }
});
