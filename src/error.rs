use crate::location::Span;
use thiserror::Error;

/// Compiler error types
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CompilerError {
    // Lexical errors
    #[error("Invalid character: {character}")]
    InvalidCharacter { character: char, span: Span },

    #[error("Unterminated string literal")]
    UnterminatedString { span: Span },

    #[error("Invalid number format: {value}")]
    InvalidNumber { value: String, span: Span },

    #[error("Mixed tabs and spaces in indentation")]
    MixedIndentation { span: Span },

    // Syntax errors
    #[error("Expected {expected}, found {found}")]
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("Expected component or property, found {found}")]
    ExpectedComponentOrProperty { found: String, span: Span },

    #[error("Invalid indentation")]
    InvalidIndentation { span: Span },

    #[error("Unexpected end of file")]
    UnexpectedEof { span: Span },

    // Semantic errors
    #[error("Undefined reference: {name}")]
    UndefinedReference { name: String, span: Span },

    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("Unknown property '{property}' for component '{component}'")]
    UnknownProperty {
        component: String,
        property: String,
        span: Span,
    },

    #[error("Missing required property '{property}' for component '{component}'")]
    MissingRequiredProperty {
        component: String,
        property: String,
        span: Span,
    },

    #[error("Invalid value for property '{property}': {message}")]
    InvalidPropertyValue {
        property: String,
        message: String,
        span: Span,
    },

    #[error("Unknown mounting point '{mounting_point}' for component '{component}'")]
    UnknownMountingPoint {
        component: String,
        mounting_point: String,
        span: Span,
    },

    #[error(
        "Invalid child type '{child_type}' in mounting point '{mounting_point}' of '{component}'"
    )]
    InvalidMountingPointChild {
        component: String,
        mounting_point: String,
        child_type: String,
        span: Span,
    },

    #[error("Component '{component}' cannot be used in this context")]
    InvalidComponentPosition {
        component: String,
        message: String,
        span: Span,
    },

    #[error("Duplicate definition: {name}")]
    DuplicateDefinition { name: String, span: Span },

    #[error("Undefined component: {name}")]
    UndefinedComponent { name: String, span: Span },

    #[error("Mounting point '{mounting_point}' cannot be on the same line as the component")]
    MountingPointOnSameLine { mounting_point: String, span: Span },

    #[error("Property '{property}' must come before mounting points")]
    PropertyAfterMountingPoint { property: String, span: Span },

    // Module resolution errors
    #[error("Module not found: '{name}'")]
    ModuleNotFound { name: String, span: Span },

    #[error("Failed to read module '{path}': {error}")]
    ModuleReadError {
        path: String,
        error: String,
        span: Span,
    },

    #[error("Circular import detected: {cycle}")]
    CircularImport { cycle: String, span: Span },

    #[error("Cannot import private item '{name}'")]
    PrivateImport { name: String, span: Span },

    #[error("Item '{item}' not found in module '{module}'. Available items: {available}")]
    ImportItemNotFound {
        item: String,
        module: String,
        available: String,
        span: Span,
    },

    // Parser errors
    #[error("Parse error: {message}")]
    ParseError { message: String, span: Span },

    // Type resolution errors
    #[error("Undefined type: '{name}'")]
    UndefinedType { name: String, span: Span },

    #[error("Cannot redefine primitive type '{name}'")]
    PrimitiveRedefinition { name: String, span: Span },

    // Trait validation errors
    #[error("Undefined trait: '{name}'")]
    UndefinedTrait { name: String, span: Span },

    #[error("'{name}' is a {actual_kind}, not a trait (cannot be used in trait composition)")]
    NotATrait {
        name: String,
        actual_kind: String,
        span: Span,
    },

    #[error("Missing required field '{field}' from trait '{trait_name}'")]
    MissingTraitField {
        field: String,
        trait_name: String,
        span: Span,
    },

    #[error("Field '{field}' has type {actual} but trait '{trait_name}' requires {expected}")]
    TraitFieldTypeMismatch {
        field: String,
        trait_name: String,
        expected: String,
        actual: String,
        span: Span,
    },

    #[error("Model trait '{name}' cannot have mounting points")]
    ModelTraitWithMountingPoints { name: String, span: Span },

    #[error("View trait '{name}' used in model '{model}'")]
    ViewTraitInModel {
        name: String,
        model: String,
        span: Span,
    },

    #[error("Model trait '{name}' used in view '{view}'")]
    ModelTraitInView {
        name: String,
        view: String,
        span: Span,
    },

    #[error("Missing required mounting point '{mount}' from trait '{trait_name}'")]
    MissingTraitMountingPoint {
        mount: String,
        trait_name: String,
        span: Span,
    },

    #[error(
        "Mounting point '{mount}' has type {actual} but trait '{trait_name}' requires {expected}"
    )]
    TraitMountingPointTypeMismatch {
        mount: String,
        trait_name: String,
        expected: String,
        actual: String,
        span: Span,
    },

    // Circular dependency errors
    #[error("Circular dependency detected: {cycle}")]
    CircularDependency { cycle: String, span: Span },

    // Expression validation errors
    #[error("Binary operator {op} cannot be applied to {left_type} and {right_type}")]
    InvalidBinaryOp {
        op: String,
        left_type: String,
        right_type: String,
        span: Span,
    },

    #[error("For loop requires an array, found {actual}")]
    ForLoopNotArray { actual: String, span: Span },

    #[error("Array destructuring requires an array, found {actual}")]
    ArrayDestructuringNotArray { actual: String, span: Span },

    #[error("Struct destructuring requires a struct, found {actual}")]
    StructDestructuringNotStruct { actual: String, span: Span },

    #[error("If condition must be boolean or optional, found {actual}")]
    InvalidIfCondition { actual: String, span: Span },

    #[error("Match scrutinee must be an enum, found {actual}")]
    MatchNotEnum { actual: String, span: Span },

    #[error("Match is not exhaustive, missing variant(s): {missing}")]
    NonExhaustiveMatch { missing: String, span: Span },

    #[error("Duplicate match arm for variant '{variant}'")]
    DuplicateMatchArm { variant: String, span: Span },

    #[error("Unknown enum variant '{variant}' for enum '{enum_name}'")]
    UnknownEnumVariant {
        variant: String,
        enum_name: String,
        span: Span,
    },

    #[error("Variant '{variant}' has {expected} associated values, found {actual}")]
    VariantArityMismatch {
        variant: String,
        expected: usize,
        actual: usize,
        span: Span,
    },

    #[error("Missing field '{field}' for {type_name}")]
    MissingField {
        field: String,
        type_name: String,
        span: Span,
    },

    #[error("Unknown field '{field}' for {type_name}")]
    UnknownField {
        field: String,
        type_name: String,
        span: Span,
    },

    #[error("Cannot assign to immutable binding")]
    AssignmentToImmutable { span: Span },

    #[error(
        "Struct '{struct_name}' requires named arguments (field: value), but argument {position} is positional"
    )]
    PositionalArgInStruct {
        struct_name: String,
        position: usize,
        span: Span,
    },

    #[error("Enum variant '{variant}' has no data, cannot instantiate with parentheses")]
    EnumVariantWithoutData {
        variant: String,
        enum_name: String,
        span: Span,
    },

    #[error(
        "Enum variant '{variant}' requires data, use {enum_name}.{variant}(field: value, ...)"
    )]
    EnumVariantRequiresData {
        variant: String,
        enum_name: String,
        span: Span,
    },

    // Mutability errors
    #[error("Parameter '{param}' requires a mutable value, but received an immutable value")]
    MutabilityMismatch { param: String, span: Span },

    // Generic type errors
    #[error("Type '{name}' expected {expected} generic argument(s), found {actual}")]
    GenericArityMismatch {
        name: String,
        expected: usize,
        actual: usize,
        span: Span,
    },

    #[error("Type argument '{arg}' does not satisfy constraint '{constraint}'")]
    GenericConstraintViolation {
        arg: String,
        constraint: String,
        span: Span,
    },

    #[error("Type parameter '{param}' is out of scope")]
    OutOfScopeTypeParameter { param: String, span: Span },

    #[error("Generic type '{name}' requires type arguments")]
    MissingGenericArguments { name: String, span: Span },

    #[error("Duplicate generic parameter '{param}'")]
    DuplicateGenericParam { param: String, span: Span },

    // Mount field errors
    #[error("Unknown mount field '{mount}' in struct '{struct_name}'")]
    UnknownMount {
        mount: String,
        struct_name: String,
        span: Span,
    },

    // Enum type inference errors
    #[error("Cannot infer enum type for variant '.{variant}' from context")]
    CannotInferEnumType { variant: String, span: Span },

    // Function validation errors
    #[error("Function '{function}' has return type {expected} but body has type {actual}")]
    FunctionReturnTypeMismatch {
        function: String,
        expected: String,
        actual: String,
        span: Span,
    },
}

impl CompilerError {
    pub fn span(&self) -> Span {
        match self {
            Self::InvalidCharacter { span, .. }
            | Self::UnterminatedString { span }
            | Self::InvalidNumber { span, .. }
            | Self::MixedIndentation { span }
            | Self::UnexpectedToken { span, .. }
            | Self::ExpectedComponentOrProperty { span, .. }
            | Self::InvalidIndentation { span }
            | Self::UnexpectedEof { span }
            | Self::UndefinedReference { span, .. }
            | Self::TypeMismatch { span, .. }
            | Self::UnknownProperty { span, .. }
            | Self::MissingRequiredProperty { span, .. }
            | Self::InvalidPropertyValue { span, .. }
            | Self::UnknownMountingPoint { span, .. }
            | Self::InvalidMountingPointChild { span, .. }
            | Self::InvalidComponentPosition { span, .. }
            | Self::DuplicateDefinition { span, .. }
            | Self::UndefinedComponent { span, .. }
            | Self::MountingPointOnSameLine { span, .. }
            | Self::PropertyAfterMountingPoint { span, .. }
            | Self::ModuleNotFound { span, .. }
            | Self::ModuleReadError { span, .. }
            | Self::CircularImport { span, .. }
            | Self::PrivateImport { span, .. }
            | Self::ImportItemNotFound { span, .. }
            | Self::ParseError { span, .. }
            | Self::UndefinedType { span, .. }
            | Self::PrimitiveRedefinition { span, .. }
            | Self::UndefinedTrait { span, .. }
            | Self::NotATrait { span, .. }
            | Self::MissingTraitField { span, .. }
            | Self::TraitFieldTypeMismatch { span, .. }
            | Self::ModelTraitWithMountingPoints { span, .. }
            | Self::ViewTraitInModel { span, .. }
            | Self::ModelTraitInView { span, .. }
            | Self::MissingTraitMountingPoint { span, .. }
            | Self::TraitMountingPointTypeMismatch { span, .. }
            | Self::CircularDependency { span, .. }
            | Self::InvalidBinaryOp { span, .. }
            | Self::ForLoopNotArray { span, .. }
            | Self::ArrayDestructuringNotArray { span, .. }
            | Self::StructDestructuringNotStruct { span, .. }
            | Self::InvalidIfCondition { span, .. }
            | Self::MatchNotEnum { span, .. }
            | Self::NonExhaustiveMatch { span, .. }
            | Self::DuplicateMatchArm { span, .. }
            | Self::UnknownEnumVariant { span, .. }
            | Self::VariantArityMismatch { span, .. }
            | Self::MissingField { span, .. }
            | Self::UnknownField { span, .. }
            | Self::PositionalArgInStruct { span, .. }
            | Self::EnumVariantWithoutData { span, .. }
            | Self::EnumVariantRequiresData { span, .. }
            | Self::MutabilityMismatch { span, .. }
            | Self::GenericArityMismatch { span, .. }
            | Self::GenericConstraintViolation { span, .. }
            | Self::OutOfScopeTypeParameter { span, .. }
            | Self::MissingGenericArguments { span, .. }
            | Self::DuplicateGenericParam { span, .. }
            | Self::UnknownMount { span, .. }
            | Self::CannotInferEnumType { span, .. }
            | Self::FunctionReturnTypeMismatch { span, .. }
            | Self::AssignmentToImmutable { span, .. } => *span,
        }
    }
}

/// Result type for compiler operations
pub type CompilerResult<T> = Result<T, Vec<CompilerError>>;
