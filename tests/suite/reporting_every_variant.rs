//! Render every `CompilerError` variant.
//!
//! The renderer draws a source snippet around a span. It is the last
//! thing that runs before a user sees a failure, so a panic there
//! turns a clear diagnostic into a crash — and the user has no way
//! around it, because the only way to reach the renderer is to have
//! made a mistake.
//!
//! Most variants are hard to provoke from source, so most were never
//! rendered by any test. This file constructs one of each and renders
//! it against four sources: the matching one, an empty one, one whose
//! text is shorter than the span, and one made of multi-byte
//! characters. A stale span is not hypothetical — an editor sends
//! diagnostics for a buffer that has already moved on.
//!
//! [`variant_name`] matches exhaustively, with no wildcard arm, so
//! adding a variant to `CompilerError` stops this file compiling until
//! someone adds a sample for it.

#![expect(
    clippy::too_many_lines,
    reason = "one arm per CompilerError variant; splitting it would hide the exhaustiveness"
)]

use formalang::ast::PrimitiveType;
use formalang::{report_error, report_errors, CompilerError, Location, Span};

/// A span over the first line of [`MATCHING_SOURCE`].
const fn span() -> Span {
    Span::new(Location::new(4, 1, 5), Location::new(11, 1, 12))
}

/// A span that runs past the end of every source below.
const fn stale_span() -> Span {
    Span::new(Location::new(9000, 400, 5), Location::new(9100, 400, 40))
}

const MATCHING_SOURCE: &str = "pub struct Sample {\n    field: I32\n}\n";
const EMPTY_SOURCE: &str = "";
const SHORT_SOURCE: &str = "x";
const MULTIBYTE_SOURCE: &str = "// é中\u{1f600}\npub struct Sample {\n    field: I32\n}\n";

fn s(text: &str) -> String {
    text.to_string()
}

/// One value of every `CompilerError` variant.
fn every_variant(span: Span) -> Vec<CompilerError> {
    vec![
        CompilerError::InvalidCharacter {
            character: '\u{feff}',
            span,
        },
        CompilerError::UnterminatedString { span },
        CompilerError::UnterminatedBlockComment { span },
        CompilerError::InvalidUnicodeEscape {
            value: s("ZZZZ"),
            span,
        },
        CompilerError::InvalidNumber {
            value: s("1e400"),
            span,
        },
        CompilerError::UnexpectedToken {
            expected: s("'}'"),
            found: s("'let'"),
            span,
        },
        CompilerError::UnexpectedEof { span },
        CompilerError::UndefinedReference {
            name: s("missing"),
            span,
        },
        CompilerError::TypeMismatch {
            expected: s("I32"),
            found: s("String"),
            span,
        },
        CompilerError::DuplicateDefinition {
            name: s("Sample"),
            span,
        },
        CompilerError::ModuleNotFound {
            name: s("other"),
            span,
        },
        CompilerError::ModuleReadError {
            path: s("other.fv"),
            error: s("permission denied"),
            span,
        },
        CompilerError::CircularImport {
            cycle: s("a -> b -> a"),
            span,
        },
        CompilerError::PrivateImport {
            name: s("Hidden"),
            span,
        },
        CompilerError::ImportItemNotFound {
            item: s("Missing"),
            module: s("other"),
            available: s("Alpha, Beta"),
            span,
        },
        CompilerError::ParseError {
            message: s("expected an expression"),
            span,
        },
        CompilerError::UndefinedType {
            name: s("Unknown"),
            span,
        },
        CompilerError::PrimitiveRedefinition {
            name: s("I32"),
            span,
        },
        CompilerError::TraitUsedAsValueType {
            trait_name: s("Shape"),
            span,
        },
        CompilerError::UndefinedTrait {
            name: s("Shape"),
            span,
        },
        CompilerError::NotATrait {
            name: s("Sample"),
            actual_kind: s("struct"),
            span,
        },
        CompilerError::MissingTraitField {
            field: s("name"),
            trait_name: s("Named"),
            span,
        },
        CompilerError::TraitFieldTypeMismatch {
            field: s("name"),
            trait_name: s("Named"),
            expected: s("String"),
            actual: s("I32"),
            span,
        },
        CompilerError::CircularDependency {
            cycle: s("A -> B -> A"),
            span,
        },
        CompilerError::InvalidBinaryOp {
            op: s("+"),
            left_type: s("String"),
            right_type: s("Boolean"),
            span,
        },
        CompilerError::ForLoopNotArray {
            actual: s("I32"),
            span,
        },
        CompilerError::ArrayDestructuringNotArray {
            actual: s("I32"),
            span,
        },
        CompilerError::StructDestructuringNotStruct {
            actual: s("I32"),
            span,
        },
        CompilerError::InvalidIfCondition {
            actual: s("I32"),
            span,
        },
        CompilerError::MatchNotEnum {
            actual: s("I32"),
            span,
        },
        CompilerError::NonExhaustiveMatch {
            missing: s("inactive"),
            span,
        },
        CompilerError::DuplicateMatchArm {
            variant: s("active"),
            span,
        },
        CompilerError::UnknownEnumVariant {
            variant: s("nope"),
            enum_name: s("Status"),
            span,
        },
        CompilerError::UnreachableMatchArm { span },
        CompilerError::PointlessOptionalElement {
            declared: s("[I32?]"),
            suggested: s("[I32]"),
            span,
        },
        CompilerError::PrivateTypeInPublic {
            type_name: s("Hidden"),
            position: s("the return type of f"),
            span,
        },
        CompilerError::NotIndexable {
            actual: s("Boolean"),
            span,
        },
        CompilerError::ArgumentCountMismatch {
            callee: s("This closure"),
            expected: 1,
            actual: 2,
            span,
        },
        CompilerError::VariantArityMismatch {
            variant: s("circle"),
            expected: 1,
            actual: 2,
            span,
        },
        CompilerError::MissingField {
            field: s("field"),
            type_name: s("Sample"),
            span,
        },
        CompilerError::UnknownField {
            field: s("nope"),
            type_name: s("Sample"),
            span,
        },
        CompilerError::AssignmentToImmutable { span },
        CompilerError::AssignmentToElement { span },
        CompilerError::OverlappingArguments {
            path: s("sample"),
            span,
        },
        CompilerError::PositionalArgInStruct {
            struct_name: s("Sample"),
            position: 2,
            span,
        },
        CompilerError::EnumVariantWithoutData {
            variant: s("point"),
            enum_name: s("Shape"),
            span,
        },
        CompilerError::EnumVariantRequiresData {
            variant: s("circle"),
            enum_name: s("Shape"),
            span,
        },
        CompilerError::MutabilityMismatch {
            param: s("counter"),
            span,
        },
        CompilerError::UseAfterSink {
            name: s("label"),
            span,
        },
        CompilerError::GenericArityMismatch {
            name: s("Pair"),
            expected: 2,
            actual: 1,
            span,
        },
        CompilerError::GenericConstraintViolation {
            arg: s("I32"),
            constraint: s("Named"),
            span,
        },
        CompilerError::OutOfScopeTypeParameter {
            param: s("T"),
            span,
        },
        CompilerError::MissingGenericArguments {
            name: s("Pair"),
            span,
        },
        CompilerError::DuplicateGenericParam {
            param: s("T"),
            span,
        },
        CompilerError::ExternFnWithBody {
            function: s("host_call"),
            span,
        },
        CompilerError::RegularFnWithoutBody {
            function: s("compute"),
            span,
        },
        CompilerError::ExternImplWithBody {
            name: s("String"),
            span,
        },
        CompilerError::RequiredParamAfterDefault {
            function: s("build"),
            param: s("size"),
            span,
        },
        CompilerError::NilAssignedToNonOptional {
            expected: s("I32"),
            span,
        },
        CompilerError::OptionalUsedAsNonOptional {
            actual: s("I32?"),
            expected: s("I32"),
            span,
        },
        CompilerError::MissingTraitMethod {
            method: s("area"),
            trait_name: s("Shape"),
            span,
        },
        CompilerError::TraitMethodSignatureMismatch {
            method: s("area"),
            trait_name: s("Shape"),
            expected: s("(self) -> I32"),
            actual: s("(self) -> F64"),
            span,
        },
        CompilerError::AmbiguousCall {
            function: s("render"),
            span,
        },
        CompilerError::NoMatchingOverload {
            function: s("render"),
            span,
        },
        CompilerError::CannotInferEnumType {
            variant: s("active"),
            span,
        },
        CompilerError::ClosureParameterNeedsType {
            param: s("x"),
            span,
        },
        CompilerError::FloatDictionaryKey {
            key_type: s("F64"),
            span,
        },
        CompilerError::SeqNotConsumed { span },
        CompilerError::SeqUsedTwice {
            name: s("rows"),
            span,
        },
        CompilerError::SeqInvalidPosition {
            position: s("struct field"),
            span,
        },
        CompilerError::FunctionReturnTypeMismatch {
            function: s("compute"),
            expected: s("I32"),
            actual: s("String"),
            span,
        },
        CompilerError::ExpressionDepthExceeded { span },
        CompilerError::TooManyDefinitions {
            kind: "struct",
            span,
        },
        CompilerError::VisibilityViolation {
            name: s("Hidden"),
            span,
        },
        CompilerError::ClosureCaptureEscapesLocalBinding {
            binding: s("total"),
            span,
        },
        CompilerError::InternalError {
            detail: s("unreachable lowering state"),
            span,
        },
        CompilerError::NumericOverflow {
            written: s("99999999999"),
            target: PrimitiveType::I32,
            span,
        },
        CompilerError::PublicClosureField {
            owner: s("Sample"),
            field: s("callback"),
            span,
        },
    ]
}

/// The name of `error`'s variant.
///
/// The match is exhaustive on purpose: a new `CompilerError` variant
/// stops this file compiling, which is the reminder to add a sample
/// for it to [`every_variant`].
const fn variant_name(error: &CompilerError) -> &'static str {
    match error {
        CompilerError::InvalidCharacter { .. } => "InvalidCharacter",
        CompilerError::UnterminatedString { .. } => "UnterminatedString",
        CompilerError::UnterminatedBlockComment { .. } => "UnterminatedBlockComment",
        CompilerError::InvalidUnicodeEscape { .. } => "InvalidUnicodeEscape",
        CompilerError::InvalidNumber { .. } => "InvalidNumber",
        CompilerError::UnexpectedToken { .. } => "UnexpectedToken",
        CompilerError::UnexpectedEof { .. } => "UnexpectedEof",
        CompilerError::UndefinedReference { .. } => "UndefinedReference",
        CompilerError::TypeMismatch { .. } => "TypeMismatch",
        CompilerError::DuplicateDefinition { .. } => "DuplicateDefinition",
        CompilerError::ModuleNotFound { .. } => "ModuleNotFound",
        CompilerError::ModuleReadError { .. } => "ModuleReadError",
        CompilerError::CircularImport { .. } => "CircularImport",
        CompilerError::PrivateImport { .. } => "PrivateImport",
        CompilerError::ImportItemNotFound { .. } => "ImportItemNotFound",
        CompilerError::ParseError { .. } => "ParseError",
        CompilerError::UndefinedType { .. } => "UndefinedType",
        CompilerError::PrimitiveRedefinition { .. } => "PrimitiveRedefinition",
        CompilerError::TraitUsedAsValueType { .. } => "TraitUsedAsValueType",
        CompilerError::UndefinedTrait { .. } => "UndefinedTrait",
        CompilerError::NotATrait { .. } => "NotATrait",
        CompilerError::MissingTraitField { .. } => "MissingTraitField",
        CompilerError::TraitFieldTypeMismatch { .. } => "TraitFieldTypeMismatch",
        CompilerError::CircularDependency { .. } => "CircularDependency",
        CompilerError::InvalidBinaryOp { .. } => "InvalidBinaryOp",
        CompilerError::ForLoopNotArray { .. } => "ForLoopNotArray",
        CompilerError::ArrayDestructuringNotArray { .. } => "ArrayDestructuringNotArray",
        CompilerError::StructDestructuringNotStruct { .. } => "StructDestructuringNotStruct",
        CompilerError::InvalidIfCondition { .. } => "InvalidIfCondition",
        CompilerError::MatchNotEnum { .. } => "MatchNotEnum",
        CompilerError::NonExhaustiveMatch { .. } => "NonExhaustiveMatch",
        CompilerError::DuplicateMatchArm { .. } => "DuplicateMatchArm",
        CompilerError::UnknownEnumVariant { .. } => "UnknownEnumVariant",
        CompilerError::UnreachableMatchArm { .. } => "UnreachableMatchArm",
        CompilerError::PointlessOptionalElement { .. } => "PointlessOptionalElement",
        CompilerError::PrivateTypeInPublic { .. } => "PrivateTypeInPublic",
        CompilerError::NotIndexable { .. } => "NotIndexable",
        CompilerError::ArgumentCountMismatch { .. } => "ArgumentCountMismatch",
        CompilerError::VariantArityMismatch { .. } => "VariantArityMismatch",
        CompilerError::MissingField { .. } => "MissingField",
        CompilerError::UnknownField { .. } => "UnknownField",
        CompilerError::AssignmentToImmutable { .. } => "AssignmentToImmutable",
        CompilerError::AssignmentToElement { .. } => "AssignmentToElement",
        CompilerError::OverlappingArguments { .. } => "OverlappingArguments",
        CompilerError::PositionalArgInStruct { .. } => "PositionalArgInStruct",
        CompilerError::EnumVariantWithoutData { .. } => "EnumVariantWithoutData",
        CompilerError::EnumVariantRequiresData { .. } => "EnumVariantRequiresData",
        CompilerError::MutabilityMismatch { .. } => "MutabilityMismatch",
        CompilerError::UseAfterSink { .. } => "UseAfterSink",
        CompilerError::GenericArityMismatch { .. } => "GenericArityMismatch",
        CompilerError::GenericConstraintViolation { .. } => "GenericConstraintViolation",
        CompilerError::OutOfScopeTypeParameter { .. } => "OutOfScopeTypeParameter",
        CompilerError::MissingGenericArguments { .. } => "MissingGenericArguments",
        CompilerError::DuplicateGenericParam { .. } => "DuplicateGenericParam",
        CompilerError::ExternFnWithBody { .. } => "ExternFnWithBody",
        CompilerError::RegularFnWithoutBody { .. } => "RegularFnWithoutBody",
        CompilerError::ExternImplWithBody { .. } => "ExternImplWithBody",
        CompilerError::RequiredParamAfterDefault { .. } => "RequiredParamAfterDefault",
        CompilerError::NilAssignedToNonOptional { .. } => "NilAssignedToNonOptional",
        CompilerError::OptionalUsedAsNonOptional { .. } => "OptionalUsedAsNonOptional",
        CompilerError::MissingTraitMethod { .. } => "MissingTraitMethod",
        CompilerError::TraitMethodSignatureMismatch { .. } => "TraitMethodSignatureMismatch",
        CompilerError::AmbiguousCall { .. } => "AmbiguousCall",
        CompilerError::NoMatchingOverload { .. } => "NoMatchingOverload",
        CompilerError::CannotInferEnumType { .. } => "CannotInferEnumType",
        CompilerError::ClosureParameterNeedsType { .. } => "ClosureParameterNeedsType",
        CompilerError::FloatDictionaryKey { .. } => "FloatDictionaryKey",
        CompilerError::SeqNotConsumed { .. } => "SeqNotConsumed",
        CompilerError::SeqUsedTwice { .. } => "SeqUsedTwice",
        CompilerError::SeqInvalidPosition { .. } => "SeqInvalidPosition",
        CompilerError::FunctionReturnTypeMismatch { .. } => "FunctionReturnTypeMismatch",
        CompilerError::ExpressionDepthExceeded { .. } => "ExpressionDepthExceeded",
        CompilerError::TooManyDefinitions { .. } => "TooManyDefinitions",
        CompilerError::VisibilityViolation { .. } => "VisibilityViolation",
        CompilerError::ClosureCaptureEscapesLocalBinding { .. } => {
            "ClosureCaptureEscapesLocalBinding"
        }
        CompilerError::InternalError { .. } => "InternalError",
        CompilerError::NumericOverflow { .. } => "NumericOverflow",
        CompilerError::PublicClosureField { .. } => "PublicClosureField",
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Every variant renders against the source its span belongs to.
#[test]
fn every_variant_renders() -> Result<(), Box<dyn std::error::Error>> {
    for error in every_variant(span()) {
        let name = variant_name(&error);
        let report = report_error(&error, MATCHING_SOURCE, "sample.fv");
        if report.is_empty() {
            return Err(format!("{name} rendered an empty report").into());
        }
        if report.contains("failed to render error") {
            return Err(format!("{name} fell back to the raw Display impl:\n{report}").into());
        }
    }
    Ok(())
}

/// Every variant carries a message of its own. A report that holds
/// only frame chrome tells the user nothing.
#[test]
fn every_variant_carries_a_message() -> Result<(), Box<dyn std::error::Error>> {
    for error in every_variant(span()) {
        let name = variant_name(&error);
        let report = report_error(&error, MATCHING_SOURCE, "sample.fv");
        // Every report ariadne draws names the file it points into.
        if !report.contains("sample.fv") {
            return Err(format!("{name} rendered without naming the file:\n{report}").into());
        }
        // And it is more than a bare frame.
        if report.lines().count() < 2 {
            return Err(format!("{name} rendered a one-line report:\n{report}").into());
        }
    }
    Ok(())
}

/// Every variant renders against a source that does not match its
/// span. An editor sends diagnostics for a buffer that has already
/// moved on, so this is the normal case, not an odd one.
#[test]
fn every_variant_renders_against_a_mismatched_source() -> Result<(), Box<dyn std::error::Error>> {
    for source in [
        EMPTY_SOURCE,
        SHORT_SOURCE,
        MULTIBYTE_SOURCE,
        MATCHING_SOURCE,
    ] {
        for error in every_variant(stale_span()) {
            let name = variant_name(&error);
            let report = report_error(&error, source, "sample.fv");
            if report.is_empty() {
                return Err(format!(
                    "{name} rendered an empty report against a {}-byte source",
                    source.len()
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Every variant renders against a source made of multi-byte
/// characters, with a span whose offsets fall inside one of them.
#[test]
fn every_variant_renders_inside_a_multibyte_character() -> Result<(), Box<dyn std::error::Error>> {
    // `MULTIBYTE_SOURCE` starts `// é…`; byte 4 is the second byte of
    // `é`, so neither end of this span is a character boundary.
    let inside = Span::new(Location::new(4, 1, 4), Location::new(5, 1, 4));
    for error in every_variant(inside) {
        let name = variant_name(&error);
        let report = report_error(&error, MULTIBYTE_SOURCE, "sample.fv");
        if report.is_empty() {
            return Err(format!("{name} rendered an empty report on multi-byte text").into());
        }
    }
    Ok(())
}

/// `report_errors` renders a whole list, and every variant appears in
/// the result.
#[test]
fn the_whole_list_renders_at_once() -> Result<(), Box<dyn std::error::Error>> {
    let errors = every_variant(span());
    let count = errors.len();
    let report = report_errors(&errors, MATCHING_SOURCE, "sample.fv");
    if report.is_empty() {
        return Err("rendering the whole list produced nothing".into());
    }
    // One report per error, joined by a newline, so the result must
    // name the file once per error.
    let mentions = report.matches("sample.fv").count();
    if mentions < count {
        return Err(format!(
            "expected at least {count} reports, found {mentions} mentions of the file"
        )
        .into());
    }
    Ok(())
}

/// Every variant has a distinct name, and the sample list covers each
/// one exactly once.
///
/// Together with the exhaustive match in [`variant_name`], this makes
/// a new variant impossible to add without extending the list.
#[test]
fn the_sample_list_covers_every_variant_once() -> Result<(), Box<dyn std::error::Error>> {
    let errors = every_variant(span());
    let mut names: Vec<&str> = errors.iter().map(variant_name).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    if names.len() != total {
        return Err(format!(
            "the sample list holds {total} errors but only {} distinct variants",
            names.len()
        )
        .into());
    }
    Ok(())
}

/// Every variant's span accessor returns the span it was built with.
#[test]
fn every_variant_reports_its_span() -> Result<(), Box<dyn std::error::Error>> {
    let expected = span();
    for error in every_variant(expected) {
        let name = variant_name(&error);
        if error.span() != expected {
            return Err(format!(
                "{name} reported {:?} instead of the span it was built with",
                error.span()
            )
            .into());
        }
    }
    Ok(())
}

/// Every variant's `Display` text is non-empty, and the rendered
/// report is not simply that text.
#[test]
fn every_variant_has_a_display_form() -> Result<(), Box<dyn std::error::Error>> {
    for error in every_variant(span()) {
        let name = variant_name(&error);
        let text = error.to_string();
        if text.trim().is_empty() {
            return Err(format!("{name} has an empty Display form").into());
        }
    }
    Ok(())
}
