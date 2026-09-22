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
use ariadne::{Color, Label, Report, ReportKind};

mod fields;
mod functions;
mod generics;
mod misc;
mod mutability;
mod optional;
mod traits;

pub(in crate::reporting) use fields::{
    assignment_to_element, assignment_to_immutable, enum_variant_requires_data,
    enum_variant_without_data, missing_field, positional_arg_in_struct, unknown_field,
};
pub(in crate::reporting) use functions::{
    ambiguous_call, argument_count_mismatch, cannot_infer_enum_type, extern_fn_with_body,
    extern_impl_with_body, function_return_type_mismatch, no_matching_overload,
    regular_fn_without_body, required_param_after_default,
};
pub(in crate::reporting) use generics::{
    duplicate_generic_param, generic_arity_mismatch, generic_constraint_violation,
    missing_generic_arguments, out_of_scope_type_parameter,
};
pub(in crate::reporting) use misc::{
    closure_capture_escapes_local_binding, expression_depth_exceeded, float_dictionary_key,
    internal_error, not_indexable, private_type_in_public, public_closure_field,
    seq_invalid_position, seq_not_consumed, seq_used_twice, too_many_definitions,
    unreachable_match_arm, visibility_violation,
};
pub(in crate::reporting) use mutability::{mutability_mismatch, use_after_sink};
pub(in crate::reporting) use optional::{
    nil_assigned_to_non_optional, optional_used_as_non_optional, pointless_optional_element,
};
pub(in crate::reporting) use traits::{
    missing_trait_field, missing_trait_method, trait_field_type_mismatch,
    trait_method_signature_mismatch,
};

/// Red label spanning `span` in `filename`. Shared across every renderer
/// in this module.
pub(in crate::reporting) fn label(
    filename: &str,
    span: Span,
) -> Label<(&str, std::ops::Range<usize>)> {
    Label::new((filename, span.start.offset..span.end.offset)).with_color(Color::Red)
}

/// New `ariadne` report builder pinned to `span.start.offset` and stamped
/// with the project-wide error `code`.
pub(in crate::reporting) fn report<'a>(
    filename: &'a str,
    span: Span,
    code: &'static str,
) -> ReportBuilder<'a> {
    Report::build(ReportKind::Error, filename, span.start.offset).with_code(code)
}
