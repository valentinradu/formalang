//! Mutability and ownership error renderers (`mut` parameter mismatch
//! and `sink` use-after-move).

use super::super::ReportBuilder;
use super::{label, report};
use crate::location::Span;
use ariadne::{Color, Fmt};

pub(in crate::reporting) fn mutability_mismatch<'a>(
    filename: &'a str,
    span: Span,
    param: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E070")
        .with_message(format!("Mutability mismatch for parameter '{param}'"))
        .with_label(label(filename, span).with_message(format!(
            "parameter '{}' requires a mutable value",
            param.fg(Color::Red)
        )))
        .with_help("Declare the binding with 'let mut' so it can be passed to a 'mut' parameter")
}

pub(in crate::reporting) fn use_after_sink<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E071")
        .with_message(format!("Use of moved value '{name}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' was moved into a 'sink' parameter and cannot be used again",
            name.fg(Color::Red)
        )))
        .with_help(
            "Each 'sink' parameter consumes its argument; do not use the binding after the call",
        )
}

pub(in crate::reporting) fn overlapping_arguments<'a>(
    filename: &'a str,
    span: Span,
    path: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E144")
        .with_message(format!("Two arguments of this call reach '{path}'"))
        .with_label(label(filename, span).with_message(format!(
            "one argument may change or take '{}' while another points at it",
            path.fg(Color::Red)
        )))
        .with_help(
            "A 'mut' or 'sink' argument needs sole access to its value. \
             Copy one argument into its own binding with 'let', and pass that",
        )
}
