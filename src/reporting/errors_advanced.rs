//! Per-variant `ReportBuilder` constructors for the more advanced /
//! less common `CompilerError` variants: trait-impl checks, generic
//! arity / constraint violations, function / overload resolution,
//! mutability and ownership, extern declarations, struct/enum field
//! errors, optional/`nil` handling, visibility, limits, and
//! closure-capture lifetime errors.
//!
//! Each function builds the `ariadne::ReportBuilder` for exactly one
//! enum variant and is invoked from the dispatcher in
//! [`super::build_error_report`].

use super::ReportBuilder;
use crate::location::Span;
use ariadne::{Color, Fmt, Label, Report, ReportKind};

fn label(filename: &str, span: Span) -> Label<(&str, std::ops::Range<usize>)> {
    Label::new((filename, span.start.offset..span.end.offset)).with_color(Color::Red)
}

fn report<'a>(filename: &'a str, span: Span, code: &'static str) -> ReportBuilder<'a> {
    Report::build(ReportKind::Error, filename, span.start.offset).with_code(code)
}

pub(super) fn missing_trait_field<'a>(
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

pub(super) fn trait_field_type_mismatch<'a>(
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

pub(super) fn missing_field<'a>(
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

pub(super) fn unknown_field<'a>(
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

pub(super) fn assignment_to_immutable(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E066")
        .with_message("Cannot assign to immutable binding")
        .with_label(label(filename, span).with_message("this binding is not mutable"))
        .with_help("Declare the binding with 'let mut' to allow assignment")
}

pub(super) fn positional_arg_in_struct<'a>(
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

pub(super) fn enum_variant_without_data<'a>(
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

pub(super) fn enum_variant_requires_data<'a>(
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

pub(super) fn mutability_mismatch<'a>(
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

pub(super) fn use_after_sink<'a>(
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
            "Each 'sink' parameter consumes its argument — do not use the binding after the call",
        )
}

pub(super) fn generic_arity_mismatch<'a>(
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

pub(super) fn generic_constraint_violation<'a>(
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

pub(super) fn out_of_scope_type_parameter<'a>(
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

pub(super) fn missing_generic_arguments<'a>(
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

pub(super) fn duplicate_generic_param<'a>(
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

pub(super) fn extern_fn_with_body<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E090")
        .with_message(format!("Extern function '{function}' must not have a body"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is declared extern — remove the body",
            function.fg(Color::Red)
        )))
}

pub(super) fn regular_fn_without_body<'a>(
    filename: &'a str,
    span: Span,
    function: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E091")
        .with_message(format!("Non-extern function '{function}' must have a body"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not extern — add a body expression",
            function.fg(Color::Red)
        )))
        .with_help("Add a body: fn name(params) -> ReturnType { expression }")
}

pub(super) fn extern_impl_with_body<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E092")
        .with_message(format!(
            "Extern impl block for '{name}' must not contain function bodies"
        ))
        .with_label(label(filename, span).with_message(format!(
            "extern impl '{}' — remove all function bodies",
            name.fg(Color::Red)
        )))
}

pub(super) fn nil_assigned_to_non_optional<'a>(
    filename: &'a str,
    span: Span,
    expected: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E095")
        .with_message(format!(
            "Cannot assign nil to non-optional type '{expected}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "nil cannot be assigned to '{}' — use '{}?' to allow nil",
            expected.fg(Color::Red),
            expected
        )))
        .with_help(format!("Change the type annotation to '{expected}?'"))
}

pub(super) fn optional_used_as_non_optional<'a>(
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

pub(super) fn missing_trait_method<'a>(
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

pub(super) fn trait_method_signature_mismatch<'a>(
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

pub(super) fn function_return_type_mismatch<'a>(
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

pub(super) fn ambiguous_call<'a>(
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

pub(super) fn no_matching_overload<'a>(
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

pub(super) fn cannot_infer_enum_type<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E122")
        .with_message(format!("Cannot infer enum type for variant '.{variant}'"))
        .with_label(label(filename, span).with_message(format!(
            "'.{}' — which enum does this variant belong to?",
            variant.fg(Color::Red)
        )))
        .with_help("Add a type annotation: let x: MyEnum = .variant")
}

pub(super) fn expression_depth_exceeded(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E130")
        .with_message("Expression nesting too deep")
        .with_label(
            label(filename, span).with_message("expression exceeds the compiler recursion limit"),
        )
        .with_help("Simplify the expression by extracting sub-expressions into let bindings")
}

pub(super) fn too_many_definitions<'a>(
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

pub(super) fn visibility_violation<'a>(
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

pub(super) fn closure_capture_escapes_local_binding<'a>(
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

pub(super) fn numeric_overflow<'a>(
    filename: &'a str,
    span: Span,
    written: &'a str,
    target: crate::ast::PrimitiveType,
) -> ReportBuilder<'a> {
    Report::build(ReportKind::Error, filename, span.start.offset)
        .with_code("E0801")
        .with_message(format!(
            "integer literal {written} does not fit in {target:?}",
        ))
        .with_label(label(filename, span).with_message("value out of range for target type"))
        .with_help(format!(
            "use the {target:?} suffix only on values within its range",
        ))
}

pub(super) fn internal_error<'a>(
    filename: &'a str,
    span: Span,
    detail: &'a str,
) -> ReportBuilder<'a> {
    // Audit2 B32: subdivide the catch-all internal-error code by
    // looking at the leading subsystem prefix on `detail`.
    // Push sites already include a prefix (e.g. "IR lowering:",
    // "monomorphise:", "registration lookup ...") so we reuse
    // those without changing 21 call sites. E999 stays as the
    // generic fall-through for anything that doesn't carry a
    // recognised prefix.
    let code = super::internal_codes::internal_error_code(detail);
    Report::build(ReportKind::Error, filename, span.start.offset)
        .with_code(code)
        .with_message(format!("Internal compiler error: {detail}"))
        .with_label(
            label(filename, span).with_message("compiler invariant violated — please file a bug"),
        )
}
