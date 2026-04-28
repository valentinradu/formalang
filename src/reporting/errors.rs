//! Per-variant `ReportBuilder` constructors for the more common
//! `CompilerError` variants: parse / lex / token / module / type
//! mismatch / cycle / destructuring / match / import / lexer / parser /
//! semantic-reference families.
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

pub(super) fn parse_error<'a>(
    filename: &'a str,
    span: Span,
    message: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E001")
        .with_message(format!("Parse error: {message}"))
        .with_label(label(filename, span).with_message(message))
}

pub(super) fn undefined_type<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E002")
        .with_message(format!("Undefined type '{name}'"))
        .with_label(
            label(filename, span)
                .with_message(format!("type '{}' is not defined", name.fg(Color::Red))),
        )
        .with_help("Check that the type is defined or imported")
}

pub(super) fn trait_used_as_value_type<'a>(
    filename: &'a str,
    span: Span,
    trait_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E063")
        .with_message(format!("Trait '{trait_name}' used as a value type"))
        .with_label(label(filename, span).with_message(format!(
            "trait '{}' cannot be the type of a value",
            trait_name.fg(Color::Red)
        )))
        .with_help(format!(
            "FormaLang has no dynamic dispatch. Take the trait as a generic bound \
             instead: `<T: {trait_name}>` and pass values of any concrete type that \
             implements `{trait_name}`."
        ))
}

pub(super) fn duplicate_definition<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E003")
        .with_message(format!("Duplicate definition of '{name}'"))
        .with_label(
            label(filename, span)
                .with_message(format!("'{}' is already defined", name.fg(Color::Red))),
        )
        .with_help("Consider renaming one of the definitions")
}

pub(super) fn type_mismatch<'a>(
    filename: &'a str,
    span: Span,
    expected: &'a str,
    found: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E004")
        .with_message("Type mismatch")
        .with_label(label(filename, span).with_message(format!(
            "expected {}, found {}",
            expected.fg(Color::Green),
            found.fg(Color::Red)
        )))
}

pub(super) fn module_not_found<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E005")
        .with_message(format!("Module '{name}' not found"))
        .with_label(label(filename, span).with_message(format!(
            "module '{}' could not be found",
            name.fg(Color::Red)
        )))
        .with_help("Check that the module file exists in the expected location")
}

pub(super) fn circular_import<'a>(
    filename: &'a str,
    span: Span,
    cycle: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E006")
        .with_message("Circular import detected")
        .with_label(
            label(filename, span)
                .with_message(format!("circular import: {}", cycle.fg(Color::Red))),
        )
        .with_help("Remove the circular dependency by restructuring your imports")
}

pub(super) fn circular_dependency<'a>(
    filename: &'a str,
    span: Span,
    cycle: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E007")
        .with_message("Circular dependency detected")
        .with_label(
            label(filename, span)
                .with_message(format!("circular dependency: {}", cycle.fg(Color::Red))),
        )
        .with_help("Remove the circular dependency by breaking the cycle")
}

pub(super) fn invalid_binary_op<'a>(
    filename: &'a str,
    span: Span,
    op: &'a str,
    left_type: &'a str,
    right_type: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E010")
        .with_message(format!("Invalid binary operation '{op}'"))
        .with_label(label(filename, span).with_message(format!(
            "cannot apply '{}' to {} and {}",
            op.fg(Color::Yellow),
            left_type.fg(Color::Cyan),
            right_type.fg(Color::Cyan)
        )))
}

pub(super) fn for_loop_not_array<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E011")
        .with_message("For loop requires an array")
        .with_label(label(filename, span).with_message(format!(
            "found {}, expected an array",
            actual.fg(Color::Red)
        )))
}

pub(super) fn array_destructuring_not_array<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E011b")
        .with_message("Array destructuring requires an array")
        .with_label(label(filename, span).with_message(format!(
            "found {}, expected an array",
            actual.fg(Color::Red)
        )))
}

pub(super) fn struct_destructuring_not_struct<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E011c")
        .with_message("Struct destructuring requires a struct")
        .with_label(label(filename, span).with_message(format!(
            "found {}, expected a struct",
            actual.fg(Color::Red)
        )))
}

pub(super) fn invalid_if_condition<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E012")
        .with_message("Invalid if condition")
        .with_label(label(filename, span).with_message(format!(
            "condition must be Boolean or optional, found {}",
            actual.fg(Color::Red)
        )))
}

pub(super) fn match_not_enum<'a>(
    filename: &'a str,
    span: Span,
    actual: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E013")
        .with_message("Match scrutinee must be an enum")
        .with_label(label(filename, span).with_message(format!(
            "found {}, expected an enum type",
            actual.fg(Color::Red)
        )))
}

pub(super) fn non_exhaustive_match<'a>(
    filename: &'a str,
    span: Span,
    missing: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E014")
        .with_message("Match is not exhaustive")
        .with_label(
            label(filename, span)
                .with_message(format!("missing variant(s): {}", missing.fg(Color::Red))),
        )
        .with_help(format!("Add arms for: {missing}"))
}

pub(super) fn duplicate_match_arm<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E015")
        .with_message(format!("Duplicate match arm for variant '{variant}'"))
        .with_label(label(filename, span).with_message(format!(
            "variant '{}' is already handled",
            variant.fg(Color::Red)
        )))
        .with_help("Remove the duplicate arm")
}

pub(super) fn private_import<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E016")
        .with_message(format!("Cannot import private item '{name}'"))
        .with_label(
            label(filename, span).with_message(format!("'{}' is not public", name.fg(Color::Red))),
        )
        .with_help(format!("Make '{name}' public with the 'pub' keyword"))
}

pub(super) fn import_item_not_found<'a>(
    filename: &'a str,
    span: Span,
    item: &'a str,
    module: &'a str,
    available: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E017")
        .with_message(format!("Item '{item}' not found in module '{module}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not exported by '{}'",
            item.fg(Color::Red),
            module
        )))
        .with_help(format!("Available items: {available}"))
}

pub(super) fn not_a_trait<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
    actual_kind: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E020")
        .with_message(format!("'{name}' is not a trait"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is a {}, not a trait",
            name.fg(Color::Red),
            actual_kind
        )))
}

pub(super) fn unknown_enum_variant<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
    enum_name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E021")
        .with_message(format!(
            "Unknown variant '{variant}' for enum '{enum_name}'"
        ))
        .with_label(label(filename, span).with_message(format!(
            "'{}' does not have a variant '{}'",
            enum_name,
            variant.fg(Color::Red)
        )))
}

pub(super) fn variant_arity_mismatch<'a>(
    filename: &'a str,
    span: Span,
    variant: &'a str,
    expected: usize,
    actual: usize,
) -> ReportBuilder<'a> {
    report(filename, span, "E022")
        .with_message(format!("Variant '{variant}' arity mismatch"))
        .with_label(label(filename, span).with_message(format!(
            "expected {} associated value(s), found {}",
            expected.to_string().fg(Color::Green),
            actual.to_string().fg(Color::Red)
        )))
}

pub(super) fn invalid_character(filename: &str, span: Span, character: char) -> ReportBuilder<'_> {
    report(filename, span, "E030")
        .with_message(format!("Invalid character '{character}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not a valid character here",
            character.fg(Color::Red)
        )))
}

pub(super) fn unterminated_string(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E031")
        .with_message("Unterminated string literal")
        .with_label(label(filename, span).with_message("string literal is never closed"))
        .with_help("Add a closing '\"' to end the string")
}

pub(super) fn unterminated_block_comment(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E033")
        .with_message("Unterminated block comment")
        .with_label(label(filename, span).with_message("block comment is never closed"))
        .with_help("Add `*/` to close the block comment")
}

pub(super) fn invalid_unicode_escape<'a>(
    filename: &'a str,
    span: Span,
    value: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E034")
        .with_message(format!("Invalid unicode escape '\\u{value}'"))
        .with_label(label(filename, span).with_message(format!(
            "'\\u{}' is not a valid unicode scalar value",
            value.fg(Color::Red)
        )))
        .with_help("Surrogate code points (U+D800..=U+DFFF) are not allowed")
}

pub(super) fn invalid_number<'a>(
    filename: &'a str,
    span: Span,
    value: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E032")
        .with_message(format!("Invalid number '{value}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not a valid number literal",
            value.fg(Color::Red)
        )))
}

pub(super) fn unexpected_token<'a>(
    filename: &'a str,
    span: Span,
    expected: &'a str,
    found: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E040")
        .with_message(format!("Unexpected token '{found}'"))
        .with_label(label(filename, span).with_message(format!(
            "expected {}, found {}",
            expected.fg(Color::Green),
            found.fg(Color::Red)
        )))
}

pub(super) fn unexpected_eof(filename: &str, span: Span) -> ReportBuilder<'_> {
    report(filename, span, "E043")
        .with_message("Unexpected end of file")
        .with_label(
            label(filename, span)
                .with_message("file ended unexpectedly — is there an unclosed delimiter?"),
        )
}

pub(super) fn undefined_reference<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E050")
        .with_message(format!("Undefined reference '{name}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is not defined in this scope",
            name.fg(Color::Red)
        )))
        .with_help(format!("Check that '{name}' is defined before it is used"))
}

pub(super) fn primitive_redefinition<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E052")
        .with_message(format!("Cannot redefine primitive type '{name}'"))
        .with_label(label(filename, span).with_message(format!(
            "'{}' is a built-in primitive and cannot be redefined",
            name.fg(Color::Red)
        )))
}

pub(super) fn undefined_trait<'a>(
    filename: &'a str,
    span: Span,
    name: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E053")
        .with_message(format!("Undefined trait '{name}'"))
        .with_label(
            label(filename, span)
                .with_message(format!("'{}' is not a known trait", name.fg(Color::Red))),
        )
        .with_help(format!("Check that '{name}' is defined or imported"))
}

pub(super) fn module_read_error<'a>(
    filename: &'a str,
    span: Span,
    path: &'a str,
    error: &'a str,
) -> ReportBuilder<'a> {
    report(filename, span, "E054")
        .with_message(format!("Failed to read module '{path}'"))
        .with_label(label(filename, span).with_message(format!(
            "could not read '{}': {}",
            path.fg(Color::Red),
            error
        )))
}
