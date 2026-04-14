// Error reporting with ariadne for beautiful, user-friendly error messages

use crate::error::CompilerError;
use ariadne::{Color, Fmt, Label, Report, ReportKind, Source};

/// Report a single compiler error with beautiful formatting
#[must_use]
pub fn report_error(error: &CompilerError, source: &str, filename: &str) -> String {
    let mut output = Vec::new();
    let report = build_error_report(error, filename);
    let _ = report
        .finish()
        .write((filename, Source::from(source)), &mut output);
    String::from_utf8_lossy(&output).into_owned()
}

type ReportBuilder<'a> = ariadne::ReportBuilder<'a, (&'a str, std::ops::Range<usize>)>;

#[expect(
    clippy::too_many_lines,
    reason = "match expression over 30 variants — arms cannot be further extracted without losing context"
)]
fn build_error_report<'a>(error: &'a CompilerError, filename: &'a str) -> ReportBuilder<'a> {
    let span = error.span();

    match error {
        CompilerError::ParseError { message, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E001")
                .with_message(format!("Parse error: {message}"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(message)
                        .with_color(Color::Red),
                )
        }

        CompilerError::UndefinedType { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E002")
                .with_message(format!("Undefined type '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("type '{}' is not defined", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Check that the type is defined or imported")
        }

        CompilerError::DuplicateDefinition { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E003")
                .with_message(format!("Duplicate definition of '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("'{}' is already defined", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Consider renaming one of the definitions")
        }

        CompilerError::TypeMismatch {
            expected, found, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E004")
            .with_message("Type mismatch")
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "expected {}, found {}",
                        expected.fg(Color::Green),
                        found.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::ModuleNotFound { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E005")
                .with_message(format!("Module '{name}' not found"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "module '{}' could not be found",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Check that the module file exists in the expected location")
        }

        CompilerError::CircularImport { cycle, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E006")
                .with_message("Circular import detected")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("circular import: {}", cycle.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Remove the circular dependency by restructuring your imports")
        }

        CompilerError::CircularDependency { cycle, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E007")
                .with_message("Circular dependency detected")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("circular dependency: {}", cycle.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Remove the circular dependency by breaking the cycle")
        }

        CompilerError::MissingTraitField {
            field, trait_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E008")
            .with_message(format!(
                "Missing required field '{field}' from trait '{trait_name}'"
            ))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' requires field '{}'",
                        trait_name.fg(Color::Blue),
                        field.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!(
                "Add the '{field}' field to satisfy the trait requirement"
            )),

        CompilerError::TraitFieldTypeMismatch {
            field,
            trait_name,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E009")
            .with_message(format!("Field '{field}' type mismatch"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' requires type {}, found {}",
                        trait_name.fg(Color::Blue),
                        expected.fg(Color::Green),
                        actual.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Change the field type to {expected}")),

        CompilerError::InvalidBinaryOp {
            op,
            left_type,
            right_type,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E010")
            .with_message(format!("Invalid binary operation '{op}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "cannot apply '{}' to {} and {}",
                        op.fg(Color::Yellow),
                        left_type.fg(Color::Cyan),
                        right_type.fg(Color::Cyan)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::ForLoopNotArray { actual, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E011")
                .with_message("For loop requires an array")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "found {}, expected an array",
                            actual.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::ArrayDestructuringNotArray { actual, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E011b")
                .with_message("Array destructuring requires an array")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "found {}, expected an array",
                            actual.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::StructDestructuringNotStruct { actual, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E011c")
                .with_message("Struct destructuring requires a struct")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "found {}, expected a struct",
                            actual.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::InvalidIfCondition { actual, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E012")
                .with_message("Invalid if condition")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "condition must be Boolean or optional, found {}",
                            actual.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::MatchNotEnum { actual, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E013")
                .with_message("Match scrutinee must be an enum")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "found {}, expected an enum type",
                            actual.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::NonExhaustiveMatch { missing, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E014")
                .with_message("Match is not exhaustive")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("missing variant(s): {}", missing.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help(format!("Add arms for: {missing}"))
        }

        CompilerError::DuplicateMatchArm { variant, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E015")
                .with_message(format!("Duplicate match arm for variant '{variant}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "variant '{}' is already handled",
                            variant.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Remove the duplicate arm")
        }

        CompilerError::PrivateImport { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E016")
                .with_message(format!("Cannot import private item '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("'{}' is not public", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help(format!("Make '{name}' public with the 'pub' keyword"))
        }

        CompilerError::ImportItemNotFound {
            item,
            module,
            available,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E017")
            .with_message(format!("Item '{item}' not found in module '{module}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' is not exported by '{}'",
                        item.fg(Color::Red),
                        module
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Available items: {available}")),

        CompilerError::ViewTraitInModel { name, model, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E018")
                .with_message(format!(
                    "View trait '{name}' cannot be used in model '{model}'"
                ))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("'{}' is a view trait", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Models can only implement model traits")
        }

        CompilerError::ModelTraitInView { name, view, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E019")
                .with_message(format!(
                    "Model trait '{name}' cannot be used in view '{view}'"
                ))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("'{}' is a model trait", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help("Views can only implement view traits")
        }

        CompilerError::NotATrait {
            name, actual_kind, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E020")
            .with_message(format!("'{name}' is not a trait"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' is a {}, not a trait",
                        name.fg(Color::Red),
                        actual_kind
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::UnknownEnumVariant {
            variant, enum_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E021")
            .with_message(format!(
                "Unknown variant '{variant}' for enum '{enum_name}'"
            ))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' does not have a variant '{}'",
                        enum_name,
                        variant.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::VariantArityMismatch {
            variant,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E022")
            .with_message(format!("Variant '{variant}' arity mismatch"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "expected {} associated value(s), found {}",
                        expected.to_string().fg(Color::Green),
                        actual.to_string().fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::MissingTraitMountingPoint {
            mount, trait_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E023")
            .with_message(format!(
                "Missing required mounting point '{mount}' from trait '{trait_name}'"
            ))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' requires mounting point '{}'",
                        trait_name.fg(Color::Blue),
                        mount.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!(
                "Add the '{mount}' mounting point to satisfy the trait requirement"
            )),

        CompilerError::TraitMountingPointTypeMismatch {
            mount,
            trait_name,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E024")
            .with_message(format!("Mounting point '{mount}' type mismatch"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' requires type {}, found {}",
                        trait_name.fg(Color::Blue),
                        expected.fg(Color::Green),
                        actual.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Change the mounting point type to {expected}")),

        // Add more specific error formatting as needed
        // For errors without specific formatting, use a generic format
        CompilerError::InvalidCharacter { .. }
        | CompilerError::UnterminatedString { .. }
        | CompilerError::InvalidNumber { .. }
        | CompilerError::MixedIndentation { .. }
        | CompilerError::UnexpectedToken { .. }
        | CompilerError::ExpectedComponentOrProperty { .. }
        | CompilerError::InvalidIndentation { .. }
        | CompilerError::UnexpectedEof { .. }
        | CompilerError::UndefinedReference { .. }
        | CompilerError::UnknownProperty { .. }
        | CompilerError::MissingRequiredProperty { .. }
        | CompilerError::InvalidPropertyValue { .. }
        | CompilerError::UnknownMountingPoint { .. }
        | CompilerError::InvalidMountingPointChild { .. }
        | CompilerError::InvalidComponentPosition { .. }
        | CompilerError::UndefinedComponent { .. }
        | CompilerError::MountingPointOnSameLine { .. }
        | CompilerError::PropertyAfterMountingPoint { .. }
        | CompilerError::ModuleReadError { .. }
        | CompilerError::PrimitiveRedefinition { .. }
        | CompilerError::UndefinedTrait { .. }
        | CompilerError::ModelTraitWithMountingPoints { .. }
        | CompilerError::MissingField { .. }
        | CompilerError::UnknownField { .. }
        | CompilerError::AssignmentToImmutable { .. }
        | CompilerError::PositionalArgInStruct { .. }
        | CompilerError::EnumVariantWithoutData { .. }
        | CompilerError::EnumVariantRequiresData { .. }
        | CompilerError::MutabilityMismatch { .. }
        | CompilerError::GenericArityMismatch { .. }
        | CompilerError::GenericConstraintViolation { .. }
        | CompilerError::OutOfScopeTypeParameter { .. }
        | CompilerError::MissingGenericArguments { .. }
        | CompilerError::DuplicateGenericParam { .. }
        | CompilerError::UnknownMount { .. }
        | CompilerError::CannotInferEnumType { .. }
        | CompilerError::FunctionReturnTypeMismatch { .. }
        | CompilerError::ExpressionDepthExceeded { .. }
        | CompilerError::TooManyDefinitions { .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E999")
                .with_message(error.to_string())
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(error.to_string())
                        .with_color(Color::Red),
                )
        }
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
