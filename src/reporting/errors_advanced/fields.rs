//! Struct/enum field error renderers: missing/unknown field,
//! immutable-binding assignment, positional arg in struct, enum-variant
//! data shape.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn missing_field<'a>(
    filename: &'a str,
    span: Span,
    field: &'a str,
    type_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E064")
        .with_message(format!("Missing field '{field}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' requires field '{}'",
            type_name,
            field.fg(Color::Red)
        )))
        .with_help(format!("Add '{field}: ...' to the expression"))
}

pub(in crate::reporting) fn unknown_field<'a>(
    filename: &'a str,
    span: Span,
    field: &'a str,
    type_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E065")
        .with_message(format!("Unknown field '{field}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' has no field '{}'",
            type_name,
            field.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn assignment_to_immutable(
    filename: &str,
    span: Span,
) -> ReportBuilder<'_> {
    report(filename, span, "E066")
        .with_message("Cannot assign to immutable binding")
        .with_label(label(filename, span).with_message("this binding is not mutable"))
        .with_help("Declare the binding with 'let mut' to allow assignment")
}

pub(in crate::reporting) fn assignment_to_element(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E143")
        .with_message("Cannot assign to an element")
        .with_label(label(filename, span).with_message("this element never changes"))
        .with_help(
            "An array, a dictionary and a string are immutable in their elements. \
             'let mut' does not change this. Make a new value, and assign the binding",
        )
}

pub(in crate::reporting) fn positional_arg_in_struct<'a>(
    filename: &'a str,
    span: Span,
    struct_name: &'a str,
    position: usize,
) -> ReportBuilder<'a> {
    report(filename, span, "E067")
        .with_message("Positional argument in struct instantiation")
        .with_label(label(filename, span).with_message(format!(
            "argument {} is positional, but '{}' requires named arguments",
            position,
            struct_name.fg(Color::Red)
        )))
        .with_help("Use 'field: value' syntax for all arguments")
}

pub(in crate::reporting) fn enum_variant_without_data<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
    enum_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E068")
        .with_message(format!("Enum variant '{variant}' has no data"))
        .with_label(label(filename, span).with_message(format!(
            "'{}.{}' has no associated data — use '.{}' without parentheses",
            enum_name,
            variant.fg(Color::Red),
            variant
        )))
}

pub(in crate::reporting) fn enum_variant_requires_data<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
    enum_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E069")
        .with_message(format!("Enum variant '{variant}' requires data"))
        .with_label(label(filename, span).with_message(format!(
            "'{}.{}' must be instantiated with its associated fields",
            enum_name,
            variant.fg(Color::Red)
        )))
        .with_help(format!("Use {enum_name}.{variant}(field: value, ...)"))
}
