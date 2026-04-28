// Error reporting with ariadne for beautiful, user-friendly error messages

use crate::error::CompilerError;
use ariadne::{Config, Source};

mod errors;
mod errors_advanced;
mod internal_codes;

/// Audit2 B35: honour the `NO_COLOR` environment variable.
///
/// When `NO_COLOR` is set to any non-empty value, error rendering omits
/// ANSI escape codes — both ariadne's frame chrome (governed by
/// `Config::with_color`) AND the inline `Fmt::fg(...)` highlights inside
/// label messages (governed at the `yansi` global level, since ariadne
/// re-exports `yansi::Color`).
///
/// `yansi::disable()` is process-global; we call it once per render
/// after detecting the env var, then restore the prior state on the way
/// out. We don't auto-detect TTY status — users wanting
/// colour-on-tty-only can set `NO_COLOR` from a shell wrapper. See
/// <https://no-color.org>.
fn colour_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// Report a single compiler error with beautiful formatting
#[must_use]
pub fn report_error(error: &CompilerError, source: &str, filename: &str) -> String {
    let with_colour = colour_enabled();
    if !with_colour {
        // Disable yansi globally so inline `Fmt::fg(...)` highlights
        // produce no ANSI codes. Restoring is best-effort: ariadne does
        // not run multi-threaded internally, so toggling around the
        // render call is safe; users who render concurrently should
        // export `NO_COLOR` once at startup.
        yansi::disable();
    }
    let mut output = Vec::new();
    let report =
        build_error_report(error, filename).with_config(Config::default().with_color(with_colour));
    let render = report
        .finish()
        .write((filename, Source::from(source)), &mut output);
    if !with_colour {
        yansi::enable();
    }
    if let Err(write_error) = render {
        // Writing to a Vec<u8> buffer cannot fail for I/O reasons, so reaching this
        // branch indicates a formatter bug. Fall back to the error's Display impl so
        // the caller still sees a useful message instead of an empty string.
        return format!("failed to render error: {write_error}\n{error}");
    }
    String::from_utf8_lossy(&output).into_owned()
}

/// Type alias used by sub-module builder functions to return a chained
/// `ariadne::ReportBuilder` with the spans tied to the source filename.
pub(crate) type ReportBuilder<'a> = ariadne::ReportBuilder<'a, (&'a str, std::ops::Range<usize>)>;

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over ~60 CompilerError variants — each arm dispatches to a per-variant builder"
)]
fn build_error_report<'a>(error: &'a CompilerError, filename: &'a str) -> ReportBuilder<'a> {
    let span = error.span();

    match error {
        CompilerError::ParseError { message, .. } => errors::parse_error(filename, span, message),
        CompilerError::UndefinedType { name, .. } => errors::undefined_type(filename, span, name),
        CompilerError::TraitUsedAsValueType { trait_name, .. } => {
            errors::trait_used_as_value_type(filename, span, trait_name)
        }
        CompilerError::DuplicateDefinition { name, .. } => {
            errors::duplicate_definition(filename, span, name)
        }
        CompilerError::TypeMismatch {
            expected, found, ..
        } => errors::type_mismatch(filename, span, expected, found),
        CompilerError::ModuleNotFound { name, .. } => {
            errors::module_not_found(filename, span, name)
        }
        CompilerError::CircularImport { cycle, .. } => {
            errors::circular_import(filename, span, cycle)
        }
        CompilerError::CircularDependency { cycle, .. } => {
            errors::circular_dependency(filename, span, cycle)
        }
        CompilerError::MissingTraitField {
            field, trait_name, ..
        } => errors_advanced::missing_trait_field(filename, span, field, trait_name),
        CompilerError::TraitFieldTypeMismatch {
            field,
            trait_name,
            expected,
            actual,
            ..
        } => errors_advanced::trait_field_type_mismatch(
            filename, span, field, trait_name, expected, actual,
        ),
        CompilerError::InvalidBinaryOp {
            op,
            left_type,
            right_type,
            ..
        } => errors::invalid_binary_op(filename, span, op, left_type, right_type),
        CompilerError::ForLoopNotArray { actual, .. } => {
            errors::for_loop_not_array(filename, span, actual)
        }
        CompilerError::ArrayDestructuringNotArray { actual, .. } => {
            errors::array_destructuring_not_array(filename, span, actual)
        }
        CompilerError::StructDestructuringNotStruct { actual, .. } => {
            errors::struct_destructuring_not_struct(filename, span, actual)
        }
        CompilerError::InvalidIfCondition { actual, .. } => {
            errors::invalid_if_condition(filename, span, actual)
        }
        CompilerError::MatchNotEnum { actual, .. } => {
            errors::match_not_enum(filename, span, actual)
        }
        CompilerError::NonExhaustiveMatch { missing, .. } => {
            errors::non_exhaustive_match(filename, span, missing)
        }
        CompilerError::DuplicateMatchArm { variant, .. } => {
            errors::duplicate_match_arm(filename, span, variant)
        }
        CompilerError::PrivateImport { name, .. } => errors::private_import(filename, span, name),
        CompilerError::ImportItemNotFound {
            item,
            module,
            available,
            ..
        } => errors::import_item_not_found(filename, span, item, module, available),
        CompilerError::NotATrait {
            name, actual_kind, ..
        } => errors::not_a_trait(filename, span, name, actual_kind),
        CompilerError::UnknownEnumVariant {
            variant, enum_name, ..
        } => errors::unknown_enum_variant(filename, span, variant, enum_name),
        CompilerError::VariantArityMismatch {
            variant,
            expected,
            actual,
            ..
        } => errors::variant_arity_mismatch(filename, span, variant, *expected, *actual),
        CompilerError::InvalidCharacter { character, .. } => {
            errors::invalid_character(filename, span, *character)
        }
        CompilerError::UnterminatedString { .. } => errors::unterminated_string(filename, span),
        CompilerError::UnterminatedBlockComment { .. } => {
            errors::unterminated_block_comment(filename, span)
        }
        CompilerError::InvalidUnicodeEscape { value, .. } => {
            errors::invalid_unicode_escape(filename, span, value)
        }
        CompilerError::InvalidNumber { value, .. } => errors::invalid_number(filename, span, value),
        CompilerError::UnexpectedToken {
            expected, found, ..
        } => errors::unexpected_token(filename, span, expected, found),
        CompilerError::UnexpectedEof { .. } => errors::unexpected_eof(filename, span),
        CompilerError::UndefinedReference { name, .. } => {
            errors::undefined_reference(filename, span, name)
        }
        CompilerError::PrimitiveRedefinition { name, .. } => {
            errors::primitive_redefinition(filename, span, name)
        }
        CompilerError::UndefinedTrait { name, .. } => errors::undefined_trait(filename, span, name),
        CompilerError::ModuleReadError { path, error, .. } => {
            errors::module_read_error(filename, span, path, error)
        }
        CompilerError::MissingField {
            field, type_name, ..
        } => errors_advanced::missing_field(filename, span, field, type_name),
        CompilerError::UnknownField {
            field, type_name, ..
        } => errors_advanced::unknown_field(filename, span, field, type_name),
        CompilerError::AssignmentToImmutable { .. } => {
            errors_advanced::assignment_to_immutable(filename, span)
        }
        CompilerError::PositionalArgInStruct {
            struct_name,
            position,
            ..
        } => errors_advanced::positional_arg_in_struct(filename, span, struct_name, *position),
        CompilerError::EnumVariantWithoutData {
            variant, enum_name, ..
        } => errors_advanced::enum_variant_without_data(filename, span, variant, enum_name),
        CompilerError::EnumVariantRequiresData {
            variant, enum_name, ..
        } => errors_advanced::enum_variant_requires_data(filename, span, variant, enum_name),
        CompilerError::MutabilityMismatch { param, .. } => {
            errors_advanced::mutability_mismatch(filename, span, param)
        }
        CompilerError::UseAfterSink { name, .. } => {
            errors_advanced::use_after_sink(filename, span, name)
        }
        CompilerError::GenericArityMismatch {
            name,
            expected,
            actual,
            ..
        } => errors_advanced::generic_arity_mismatch(filename, span, name, *expected, *actual),
        CompilerError::GenericConstraintViolation {
            arg, constraint, ..
        } => errors_advanced::generic_constraint_violation(filename, span, arg, constraint),
        CompilerError::OutOfScopeTypeParameter { param, .. } => {
            errors_advanced::out_of_scope_type_parameter(filename, span, param)
        }
        CompilerError::MissingGenericArguments { name, .. } => {
            errors_advanced::missing_generic_arguments(filename, span, name)
        }
        CompilerError::DuplicateGenericParam { param, .. } => {
            errors_advanced::duplicate_generic_param(filename, span, param)
        }
        CompilerError::ExternFnWithBody { function, .. } => {
            errors_advanced::extern_fn_with_body(filename, span, function)
        }
        CompilerError::RegularFnWithoutBody { function, .. } => {
            errors_advanced::regular_fn_without_body(filename, span, function)
        }
        CompilerError::ExternImplWithBody { name, .. } => {
            errors_advanced::extern_impl_with_body(filename, span, name)
        }
        CompilerError::NilAssignedToNonOptional { expected, .. } => {
            errors_advanced::nil_assigned_to_non_optional(filename, span, expected)
        }
        CompilerError::OptionalUsedAsNonOptional {
            actual, expected, ..
        } => errors_advanced::optional_used_as_non_optional(filename, span, actual, expected),
        CompilerError::MissingTraitMethod {
            method, trait_name, ..
        } => errors_advanced::missing_trait_method(filename, span, method, trait_name),
        CompilerError::TraitMethodSignatureMismatch {
            method,
            trait_name,
            expected,
            actual,
            ..
        } => errors_advanced::trait_method_signature_mismatch(
            filename, span, method, trait_name, expected, actual,
        ),
        CompilerError::FunctionReturnTypeMismatch {
            function,
            expected,
            actual,
            ..
        } => errors_advanced::function_return_type_mismatch(
            filename, span, function, expected, actual,
        ),
        CompilerError::AmbiguousCall { function, .. } => {
            errors_advanced::ambiguous_call(filename, span, function)
        }
        CompilerError::NoMatchingOverload { function, .. } => {
            errors_advanced::no_matching_overload(filename, span, function)
        }
        CompilerError::CannotInferEnumType { variant, .. } => {
            errors_advanced::cannot_infer_enum_type(filename, span, variant)
        }
        CompilerError::ExpressionDepthExceeded { .. } => {
            errors_advanced::expression_depth_exceeded(filename, span)
        }
        CompilerError::TooManyDefinitions { kind, .. } => {
            errors_advanced::too_many_definitions(filename, span, kind)
        }
        CompilerError::VisibilityViolation { name, .. } => {
            errors_advanced::visibility_violation(filename, span, name)
        }
        CompilerError::ClosureCaptureEscapesLocalBinding { binding, .. } => {
            errors_advanced::closure_capture_escapes_local_binding(filename, span, binding)
        }
        CompilerError::InternalError { detail, .. } => {
            errors_advanced::internal_error(filename, span, detail)
        }
        CompilerError::NumericOverflow {
            written, target, ..
        } => errors_advanced::numeric_overflow(filename, span, written, *target),
    }
}

/// Report multiple compiler errors
#[must_use]
pub fn report_errors(errors: &[CompilerError], source: &str, filename: &str) -> String {
    errors
        .iter()
        .map(|error| report_error(error, source, filename))
        .collect::<Vec<_>>()
        .join("\n")
}
