// Error reporting with ariadne for beautiful, user-friendly error messages

use crate::error::CompilerError;
use ariadne::{Color, Fmt, Label, Report, ReportKind, Source};

/// Report a single compiler error with beautiful formatting
#[must_use]
pub fn report_error(error: &CompilerError, source: &str, filename: &str) -> String {
    let mut output = Vec::new();
    let report = build_error_report(error, filename);
    if let Err(write_error) = report
        .finish()
        .write((filename, Source::from(source)), &mut output)
    {
        // Writing to a Vec<u8> buffer cannot fail for I/O reasons, so reaching this
        // branch indicates a formatter bug. Fall back to the error's Display impl so
        // the caller still sees a useful message instead of an empty string.
        return format!("failed to render error: {write_error}\n{error}");
    }
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

        // Lexer errors
        CompilerError::InvalidCharacter { character, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E030")
                .with_message(format!("Invalid character '{character}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is not a valid character here",
                            character.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::UnterminatedString { .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E031")
                .with_message("Unterminated string literal")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message("string literal is never closed")
                        .with_color(Color::Red),
                )
                .with_help("Add a closing '\"' to end the string")
        }

        CompilerError::InvalidNumber { value, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E032")
                .with_message(format!("Invalid number '{value}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is not a valid number literal",
                            value.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        // Parser errors
        CompilerError::UnexpectedToken {
            expected, found, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E040")
            .with_message(format!("Unexpected token '{found}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "expected {}, found {}",
                        expected.fg(Color::Green),
                        found.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::UnexpectedEof { .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E043")
                .with_message("Unexpected end of file")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message("file ended unexpectedly — is there an unclosed delimiter?")
                        .with_color(Color::Red),
                )
        }

        // Semantic — references and definitions
        CompilerError::UndefinedReference { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E050")
                .with_message(format!("Undefined reference '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is not defined in this scope",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help(format!("Check that '{name}' is defined before it is used"))
        }

        CompilerError::PrimitiveRedefinition { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E052")
                .with_message(format!("Cannot redefine primitive type '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is a built-in primitive and cannot be redefined",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::UndefinedTrait { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E053")
                .with_message(format!("Undefined trait '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!("'{}' is not a known trait", name.fg(Color::Red)))
                        .with_color(Color::Red),
                )
                .with_help(format!("Check that '{name}' is defined or imported"))
        }

        CompilerError::ModuleReadError { path, error, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E054")
                .with_message(format!("Failed to read module '{path}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "could not read '{}': {}",
                            path.fg(Color::Red),
                            error
                        ))
                        .with_color(Color::Red),
                )
        }

        // Struct/enum field errors
        CompilerError::MissingField {
            field, type_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E064")
            .with_message(format!("Missing field '{field}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' requires field '{}'",
                        type_name,
                        field.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Add '{field}: ...' to the expression")),

        CompilerError::UnknownField {
            field, type_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E065")
            .with_message(format!("Unknown field '{field}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' has no field '{}'",
                        type_name,
                        field.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::AssignmentToImmutable { .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E066")
                .with_message("Cannot assign to immutable binding")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message("this binding is not mutable")
                        .with_color(Color::Red),
                )
                .with_help("Declare the binding with 'let mut' to allow assignment")
        }

        CompilerError::PositionalArgInStruct {
            struct_name,
            position,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E067")
            .with_message("Positional argument in struct instantiation")
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "argument {} is positional, but '{}' requires named arguments",
                        position,
                        struct_name.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help("Use 'field: value' syntax for all arguments"),

        CompilerError::EnumVariantWithoutData {
            variant, enum_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E068")
            .with_message(format!("Enum variant '{variant}' has no data"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}.{}' has no associated data — use '.{}' without parentheses",
                        enum_name,
                        variant.fg(Color::Red),
                        variant
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::EnumVariantRequiresData {
            variant, enum_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E069")
            .with_message(format!("Enum variant '{variant}' requires data"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}.{}' must be instantiated with its associated fields",
                        enum_name,
                        variant.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Use {enum_name}.{variant}(field: value, ...)")),

        // Mutability / Mutable Value Semantics
        CompilerError::MutabilityMismatch { param, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E070")
                .with_message(format!("Mutability mismatch for parameter '{param}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "parameter '{}' requires a mutable value",
                            param.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help(
                    "Declare the binding with 'let mut' so it can be passed to a 'mut' parameter",
                )
        }

        CompilerError::UseAfterSink { name, .. } => Report::build(
            ReportKind::Error,
            filename,
            span.start.offset,
        )
        .with_code("E071")
        .with_message(format!("Use of moved value '{name}'"))
        .with_label(
            Label::new((filename, span.start.offset..span.end.offset))
                .with_message(format!(
                    "'{}' was moved into a 'sink' parameter and cannot be used again",
                    name.fg(Color::Red)
                ))
                .with_color(Color::Red),
        )
        .with_help(
            "Each 'sink' parameter consumes its argument — do not use the binding after the call",
        ),

        // Generic type errors
        CompilerError::GenericArityMismatch {
            name,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E080")
            .with_message(format!("Wrong number of type arguments for '{name}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' expects {} type argument(s), found {}",
                        name,
                        expected.to_string().fg(Color::Green),
                        actual.to_string().fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::GenericConstraintViolation {
            arg, constraint, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E081")
            .with_message(format!("Type argument '{arg}' does not satisfy constraint"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "'{}' does not implement required trait '{}'",
                        arg.fg(Color::Red),
                        constraint.fg(Color::Green)
                    ))
                    .with_color(Color::Red),
            ),

        CompilerError::OutOfScopeTypeParameter { param, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E082")
                .with_message(format!("Type parameter '{param}' is out of scope"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is not a type parameter in this context",
                            param.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::MissingGenericArguments { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E083")
                .with_message(format!("Missing type arguments for '{name}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is a generic type and requires type arguments",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help(format!("Provide type arguments: {name}<Type>"))
        }

        CompilerError::DuplicateGenericParam { param, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E084")
                .with_message(format!("Duplicate generic parameter '{param}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "type parameter '{}' is already declared",
                            param.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        // Extern errors
        CompilerError::ExternFnWithBody { function, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E090")
                .with_message(format!("Extern function '{function}' must not have a body"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is declared extern — remove the body",
                            function.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::RegularFnWithoutBody { function, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E091")
                .with_message(format!("Non-extern function '{function}' must have a body"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is not extern — add a body expression",
                            function.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Add a body: fn name(params) -> ReturnType { expression }")
        }

        CompilerError::ExternImplWithBody { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E092")
                .with_message(format!(
                    "Extern impl block for '{name}' must not contain function bodies"
                ))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "extern impl '{}' — remove all function bodies",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::NilAssignedToNonOptional { expected, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E095")
                .with_message(format!(
                    "Cannot assign nil to non-optional type '{expected}'"
                ))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "nil cannot be assigned to '{}' — use '{}?' to allow nil",
                            expected.fg(Color::Red),
                            expected
                        ))
                        .with_color(Color::Red),
                )
                .with_help(format!("Change the type annotation to '{expected}?'"))
        }

        CompilerError::OptionalUsedAsNonOptional {
            actual, expected, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E096")
            .with_message("Optional type used where non-optional is required")
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "found optional '{}', expected non-optional '{}'",
                        actual.fg(Color::Red),
                        expected.fg(Color::Green)
                    ))
                    .with_color(Color::Red),
            )
            .with_help("Unwrap the optional value before accessing its fields"),

        // Trait implementation errors
        CompilerError::MissingTraitMethod {
            method, trait_name, ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E100")
            .with_message(format!(
                "Missing method '{method}' required by trait '{trait_name}'"
            ))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' requires method '{}'",
                        trait_name.fg(Color::Blue),
                        method.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            )
            .with_help(format!("Add 'fn {method}(...)' to the impl block")),

        CompilerError::TraitMethodSignatureMismatch {
            method,
            trait_name,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E101")
            .with_message(format!("Method '{method}' signature mismatch"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "trait '{}' expects {}, found {}",
                        trait_name.fg(Color::Blue),
                        expected.fg(Color::Green),
                        actual.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        // Function errors
        CompilerError::FunctionReturnTypeMismatch {
            function,
            expected,
            actual,
            ..
        } => Report::build(ReportKind::Error, filename, span.start.offset)
            .with_code("E110")
            .with_message(format!("Return type mismatch in '{function}'"))
            .with_label(
                Label::new((filename, span.start.offset..span.end.offset))
                    .with_message(format!(
                        "function '{}' returns {} but body has type {}",
                        function,
                        expected.fg(Color::Green),
                        actual.fg(Color::Red)
                    ))
                    .with_color(Color::Red),
            ),

        // Overload resolution errors
        CompilerError::AmbiguousCall { function, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E120")
                .with_message(format!("Ambiguous call to '{function}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "multiple overloads of '{}' match this call",
                            function.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Add argument labels to disambiguate the overload")
        }

        CompilerError::NoMatchingOverload { function, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E121")
                .with_message(format!("No matching overload for '{function}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "no overload of '{}' matches the given arguments",
                            function.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Check the argument labels and types against the available overloads")
        }

        CompilerError::CannotInferEnumType { variant, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E122")
                .with_message(format!("Cannot infer enum type for variant '.{variant}'"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'.{}' — which enum does this variant belong to?",
                            variant.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help("Add a type annotation: let x: MyEnum = .variant")
        }

        // Limit errors
        CompilerError::ExpressionDepthExceeded { .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E130")
                .with_message("Expression nesting too deep")
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message("expression exceeds the compiler recursion limit")
                        .with_color(Color::Red),
                )
                .with_help(
                    "Simplify the expression by extracting sub-expressions into let bindings",
                )
        }

        CompilerError::TooManyDefinitions { kind, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E131")
                .with_message(format!("Too many {kind} definitions"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "module contains too many {kind} definitions (limit: u32::MAX)"
                        ))
                        .with_color(Color::Red),
                )
        }

        CompilerError::VisibilityViolation { name, .. } => {
            Report::build(ReportKind::Error, filename, span.start.offset)
                .with_code("E097")
                .with_message(format!("'{name}' is private"))
                .with_label(
                    Label::new((filename, span.start.offset..span.end.offset))
                        .with_message(format!(
                            "'{}' is private and cannot be accessed from outside its module",
                            name.fg(Color::Red)
                        ))
                        .with_color(Color::Red),
                )
                .with_help(format!("Make '{name}' public with the 'pub' keyword"))
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
