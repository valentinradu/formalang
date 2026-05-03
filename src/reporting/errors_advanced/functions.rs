//! Function-shape error renderers: extern with body, regular without
//! body, default-param ordering, return-type mismatch, overload
//! resolution, enum-context inference for variant calls.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn extern_fn_with_body<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E090")
        .with_message(format!("Extern function '{function}' must not have a body"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is declared extern; remove the body",
            function.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn regular_fn_without_body<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E091")
        .with_message(format!("Non-extern function '{function}' must have a body"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not extern; add a body expression",
            function.fg(Color::Red)
        )))
        .with_help("Add a body: fn name(params) -> ReturnType { expression }")
}

pub(in crate::reporting) fn extern_impl_with_body<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E092")
        .with_message(format!(
            "Extern impl block for '{name}' must not contain function bodies"
        ))
        .with_label(label(filename, span).with_message(format!(
            "extern impl '{}'; remove all function bodies",
            name.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn required_param_after_default<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
    param: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E121")
        .with_message(format!(
            "Parameter '{param}' on '{function}' has no default value but follows one that does"
        ))
        .with_label(label(filename, span).with_message(format!(
            "'{}' must have a default value or appear before any defaulted parameter",
            param.fg(Color::Red)
        )))
        .with_help("Defaults must be positional from the right: every parameter after a defaulted one needs a default too.")
}

pub(in crate::reporting) fn function_return_type_mismatch<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
    expected: &'a str,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E110")
        .with_message(format!("Return type mismatch in '{function}'"))
        .with_label(label(filename, span).with_message(format!(
            "function '{}' returns {} but body has type {}",
            function,
            expected.fg(Color::Green),
            actual.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn ambiguous_call<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E120")
        .with_message(format!("Ambiguous call to '{function}'"))
        .with_label(label(filename, span).with_message(format!(
            "multiple overloads of '{}' match this call",
            function.fg(Color::Red)
        )))
        .with_help("Add argument labels to disambiguate the overload")
}

pub(in crate::reporting) fn no_matching_overload<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E121")
        .with_message(format!("No matching overload for '{function}'"))
        .with_label(label(filename, span).with_message(format!(
            "no overload of '{}' matches the given arguments",
            function.fg(Color::Red)
        )))
        .with_help("Check the argument labels and types against the available overloads")
}

pub(in crate::reporting) fn cannot_infer_enum_type<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E122")
        .with_message(format!("Cannot infer enum type for variant '.{variant}'"))
        .with_label(label(filename, span).with_message(format!(
            "'.{}'; which enum does this variant belong to?",
            variant.fg(Color::Red)
        )))
        .with_help("Add a type annotation: let x: MyEnum = .variant")
}
