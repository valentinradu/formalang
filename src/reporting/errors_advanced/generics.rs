//! Generic-argument error renderers: arity / constraint violations,
//! out-of-scope type parameters, missing type arguments, duplicate
//! parameter names.

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn generic_arity_mismatch<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
    expected: usize,
    actual: usize,
) -> ReportBuilder<'a> {
    report(filename, span, "E080")
        .with_message(format!("Wrong number of type arguments for '{name}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' expects {} type argument(s), found {}",
            name,
            expected.to_string().fg(Color::Green),
            actual.to_string().fg(Color::Red)
        )))
}

pub(in crate::reporting) fn generic_constraint_violation<'a>(
    filename: &'a str,
    span: Span,
    arg: &'a str,
    constraint: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E081")
        .with_message(format!("Type argument '{arg}' does not satisfy constraint"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' does not implement required trait '{}'",
            arg.fg(Color::Red),
            constraint.fg(Color::Green)
        )))
}

pub(in crate::reporting) fn out_of_scope_type_parameter<'a>(
    filename: &'a str,
    span: Span,
    param: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E082")
        .with_message(format!("Type parameter '{param}' is out of scope"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not a type parameter in this context",
            param.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn missing_generic_arguments<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E083")
        .with_message(format!("Missing type arguments for '{name}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is a generic type and requires type arguments",
            name.fg(Color::Red)
        )))
        .with_help(format!("Provide type arguments: {name}<Type>"))
}

pub(in crate::reporting) fn duplicate_generic_param<'a>(
    filename: &'a str,
    span: Span,
    param: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E084")
        .with_message(format!("Duplicate generic parameter '{param}'"))
        .with_label(label(filename, span).with_message(format!(
            "type parameter '{}' is already declared",
            param.fg(Color::Red)
        )))
}

pub(in crate::reporting) fn generic_trait_method<'a>(
    filename: &'a str,
    span: Span,
    method: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E146")
        .with_message(format!(
            "Trait method '{method}' cannot declare its own type parameters"
        ))
        .with_label(label(filename, span).with_message(format!(
            "'{}' declares type parameters",
            method.fg(Color::Red)
        )))
        .with_help("Declare the type parameters on the trait, or move the method to an impl block")
}

pub(in crate::reporting) fn uninferable_method_type_parameter<'a>(
    filename: &'a str,
    span: Span,
    param: &'a str,
    method: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E147")
        .with_message(format!(
            "Type parameter '{param}' of '{method}' gets no type from the arguments"
        ))
        .with_label(label(filename, span).with_message(format!(
            "no argument can give '{}' a type",
            param.fg(Color::Red)
        )))
        .with_help("Use the type parameter in the type of a parameter with no default, or remove it. A function call can also give the type in <...>")
}
