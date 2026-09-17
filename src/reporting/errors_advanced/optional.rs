//! Optional / `nil` mismatch error renderers.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn nil_assigned_to_non_optional<'a>(
    filename: &'a str,
    span: Span,
    expected: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E095")
        .with_message(format!(
            "Cannot assign nil to non-optional type '{expected}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "nil cannot be assigned to '{}'; use '{}?' to allow nil",
            expected.fg(Color::Red),
            expected
        )))
        .with_help(format!("Change the type annotation to '{expected}?'"))
}

pub(in crate::reporting) fn optional_used_as_non_optional<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
    expected: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E096")
        .with_message("Optional type used where non-optional is required")
        .with_label(label(filename, span).with_message(format!(
            "found optional '{}', expected non-optional '{}'",
            actual.fg(Color::Red),
            expected.fg(Color::Green)
        )))
        .with_help("Unwrap the optional value before accessing its fields")
}

pub(in crate::reporting) fn pointless_optional_element<'a>(
    filename: &'a str,
    span: Span,
    declared: &'a str,
    suggested: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E142")
        .with_message("Nothing in this array is optional")
        .with_label(label(filename, span).with_message(format!(
            "every element is present, so '{}' says more than it means",
            declared.fg(Color::Red)
        )))
        .with_help(format!(
            "declare it '{}'. An optional element is worth declaring when one \
             of them may be 'nil' — either because the literal holds one, or \
             because a 'let mut' will put one there later",
            suggested.fg(Color::Green)
        ))
}
