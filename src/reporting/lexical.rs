//! Report builders for the lexical errors that the string and comment
//! checks find. The older lexical errors are in `errors.rs`.

use super::ReportBuilder;
use crate::location::Span;
use ariadne::{Color, Fmt, Label, Report, ReportKind};

fn label(filename: &str, span: Span) -> Label<(&str, std::ops::Range<usize>)> {
    Label::new((filename, span.start.offset..span.end.offset)).with_color(Color::Red)
}

fn report<'a>(filename: &'a str, span: Span, code: &'static str) -> ReportBuilder<'a> {
    Report::build(ReportKind::Error, filename, span.start.offset).with_code(code)
}

pub(super) fn invalid_escape<'a>(
    filename: &'a str,
    span: Span,
    sequence: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E035")
        .with_message(format!("Invalid escape '{sequence}'"))
        .with_label(
            label(filename, span)
                .with_message(format!("'{}' starts no escape", sequence.fg(Color::Red))),
        )
        .with_help("The escapes are \\\", \\\\, \\n, \\t, \\r and \\uXXXX with four hex digits")
}

pub(super) fn bidirectional_control(
    filename: &str,
    span: Span,
    character: char,
) -> ReportBuilder<'_> {
    let code = u32::from(character);
    report(filename, span, "E036")
        .with_message(format!(
            "Bidirectional control character U+{code:04X} in a comment or a string"
        ))
        .with_label(label(filename, span).with_message(
            "this character can make the source show a different order than the compiler reads",
        ))
        .with_help(format!(
            "Remove the character. In a string, write the escape \\u{code:04X} if you need it"
        ))
}
