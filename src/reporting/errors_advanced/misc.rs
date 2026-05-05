//! Catch-all renderers for limits, visibility, closure-capture
//! escape, and the dispatcher's internal-error fallback.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt, Report, ReportKind};

pub(in crate::reporting) fn expression_depth_exceeded(
    filename: &str,
    span: Span,
) -> ReportBuilder<'_> {
    report(filename, span, "E130")
        .with_message("Expression nesting too deep")
        .with_label(
            label(filename, span).with_message("expression exceeds the compiler recursion limit"),
        )
        .with_help("Simplify the expression by extracting sub-expressions into let bindings")
}

pub(in crate::reporting) fn too_many_definitions<'a>(
    filename: &'a str,
    span: Span,
    kind: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E131")
        .with_message(format!("Too many {kind} definitions"))
        .with_label(label(filename, span).with_message(format!(
            "module contains too many {kind} definitions (limit: u32::MAX)"
        )))
}

pub(in crate::reporting) fn visibility_violation<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E097")
        .with_message(format!("'{name}' is private"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is private and cannot be accessed from outside its module",
            name.fg(Color::Red)
        )))
        .with_help(format!("Make '{name}' public with the 'pub' keyword"))
}

pub(in crate::reporting) fn public_closure_field<'a>(
    filename: &'a str,
    span: Span,
    owner: &'a str,
    field: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E133")
        .with_message(format!(
            "'{owner}' is public and cannot have closure-typed field '{field}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "field '{}' has a closure type",
            field.fg(Color::Red)
        )))
        .with_help(
            "Closures are an internal abstraction; replace the field with a non-closure type, or make the enclosing item private",
        )
}

pub(in crate::reporting) fn closure_capture_escapes_local_binding<'a>(
    filename: &'a str,
    span: Span,
    binding: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E132")
        .with_message(format!(
            "Returned closure captures '{binding}' which does not outlive the function"
        ))
        .with_label(label(filename, span).with_message(format!(
            "'{}' dies when the function returns, leaving a dangling capture",
            binding.fg(Color::Red)
        )))
        .with_help(
            "Only `sink` parameters and outer-scope bindings may be captured by a closure that escapes the function; consider taking ownership via a `sink` parameter",
        )
}

/// Internal compiler error fallback. The `detail` push sites use
/// subsystem prefixes (e.g. `"IR lowering:"`, `"monomorphise:"`) so the
/// code dispatch is reusable without changing every call site. `E999`
/// falls through.
pub(in crate::reporting) fn internal_error<'a>(
    filename: &'a str,
    span: Span,
    detail: &'a str,
) -> ReportBuilder<'a> {
    let code = crate::reporting::internal_codes::internal_error_code(detail);
    Report::build(ReportKind::Error, filename, span.start.offset)
        .with_code(code)
        .with_message(format!("Internal compiler error: {detail}"))
        .with_label(
            label(filename, span).with_message("compiler invariant violated; please file a bug"),
        )
}
