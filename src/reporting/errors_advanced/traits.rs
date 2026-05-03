//! Trait-conformance error renderers: missing/required field, missing
//! method, signature mismatch on a method.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn missing_trait_field<'a>(
    filename: &'a str,
    span: Span,
    field: &'a str,
    trait_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E008")
        .with_message(format!(
            "Missing required field '{field}' from trait '{trait_name}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "trait '{}' requires field '{}'",
            trait_name.fg(Color::Blue),
            field.fg(Color::Red)
        )))
        .with_help(format!(
            "Add the '{field}' field to satisfy the trait requirement"
        ))
}

pub(in crate::reporting) fn trait_field_type_mismatch<'a>(
    filename: &'a str,
    span: Span,
    field: &'a str,
    trait_name: &'a str,
    expected: &'a str,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E009")
        .with_message(format!("Field '{field}' type mismatch"))
        .with_label(label(filename, span).with_message(format!(
            "trait '{}' requires type {}, found {}",
            trait_name.fg(Color::Blue),
            expected.fg(Color::Green),
            actual.fg(Color::Red)
        )))
        .with_help(format!("Change the field type to {expected}"))
}

pub(in crate::reporting) fn missing_trait_method<'a>(
    filename: &'a str,
    span: Span,
    method: &'a str,
    trait_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E100")
        .with_message(format!(
            "Missing method '{method}' required by trait '{trait_name}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "trait '{}' requires method '{}'",
            trait_name.fg(Color::Blue),
            method.fg(Color::Red)
        )))
        .with_help(format!("Add 'fn {method}(...)' to the impl block"))
}

pub(in crate::reporting) fn trait_method_signature_mismatch<'a>(
    filename: &'a str,
    span: Span,
    method: &'a str,
    trait_name: &'a str,
    expected: &'a str,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E101")
        .with_message(format!("Method '{method}' signature mismatch"))
        .with_label(label(filename, span).with_message(format!(
            "trait '{}' expects {}, found {}",
            trait_name.fg(Color::Blue),
            expected.fg(Color::Green),
            actual.fg(Color::Red)
        )))
}
