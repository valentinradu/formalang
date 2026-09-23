//! The errors the compiler reports, and where each one points.

mod span;

use crate::ast::PrimitiveType;
use crate::location::Span;
use thiserror::Error;

/// Compiler error types
#[expect(
    clippy::exhaustive_enums,
    reason = "matched exhaustively by consumer code"
)]
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CompilerError {
    // Lexical errors
    #[error("Invalid character: {character}")]
    InvalidCharacter { character: char, span: Span },

    #[error("Unterminated string literal")]
    UnterminatedString { span: Span },

    #[error("Unterminated block comment")]
    UnterminatedBlockComment { span: Span },

    #[error("Invalid unicode escape '\\u{value}'")]
    InvalidUnicodeEscape { value: String, span: Span },

    #[error("Invalid number format: {value}")]
    InvalidNumber { value: String, span: Span },

    // Syntax errors
    #[error("Expected {expected}, found {found}")]
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },

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

    #[error("Duplicate definition: {name}")]
    DuplicateDefinition { name: String, span: Span },

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

    /// A trait name appeared in a type position that produces a value
    /// (parameter, return, let annotation, struct/enum field, closure
    /// param/return). `FormaLang` has no dynamic dispatch — trait values
    /// must be passed via a generic-bounded parameter
    /// (`fn foo<T: SomeTrait>(x: T)`) so the concrete type is known
    /// after monomorphisation.
    #[error(
        "trait '{trait_name}' cannot be used as a value type — use a generic bound \
         like `<T: {trait_name}>` instead"
    )]
    TraitUsedAsValueType { trait_name: String, span: Span },

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

    #[error("This match arm can never run: '_' above it takes every remaining value")]
    UnreachableMatchArm { span: Span },

    #[error("Unknown enum variant '{variant}' for enum '{enum_name}'")]
    UnknownEnumVariant {
        variant: String,
        enum_name: String,
        span: Span,
    },

    #[error(
        "Every element of this `{declared}` is present, so nothing is optional. \
         Declare it `{suggested}`"
    )]
    PointlessOptionalElement {
        /// The declared type, as written.
        declared: String,
        /// The same type without the optional, which is what was meant.
        suggested: String,
        span: Span,
    },

    #[error("Private type '{type_name}' is named in {position}, which is public")]
    PrivateTypeInPublic {
        type_name: String,
        /// Where the type appears, ready to read in a sentence:
        /// "the return type of f", "field 'h' of struct Shown".
        position: String,
        span: Span,
    },

    #[error("Type '{actual}' cannot be indexed")]
    NotIndexable { actual: String, span: Span },

    #[error("{callee} takes {expected} argument(s), but the call gives {actual}")]
    ArgumentCountMismatch {
        /// What was called, named so the message reads naturally:
        /// "This closure", "Method 'add'".
        callee: String,
        expected: usize,
        actual: usize,
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

    /// An assignment names an element of an array, an entry of a
    /// dictionary, or a byte of a string. All three are immutable in
    /// their elements, so no binding mutability makes the write legal.
    #[error("Cannot assign to an element")]
    AssignmentToElement { span: Span },

    /// Two arguments of one call reach the same place, and one of them
    /// is `mut` or `sink`. The callee would change or take a value that
    /// another argument still points at.
    #[error("Two arguments of one call reach '{path}', and one of them is mut or sink")]
    OverlappingArguments { path: String, span: Span },

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

    #[error("Cannot use '{name}' after it was moved into a sink parameter")]
    UseAfterSink { name: String, span: Span },

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

    // Extern validation errors
    /// An `extern fn` declaration includes a body, which is not allowed.
    #[error("Extern function '{function}' must not have a body")]
    ExternFnWithBody { function: String, span: Span },

    /// A non-extern function is missing its body expression.
    #[error("Non-extern function '{function}' must have a body")]
    RegularFnWithoutBody { function: String, span: Span },

    /// An `extern impl` block contains at least one function with a body.
    #[error("Extern impl block for '{name}' must not contain function bodies")]
    ExternImplWithBody { name: String, span: Span },

    /// A parameter without a default value appears after one with a
    /// default value. Default values must be positional from the
    /// right (no required parameter may follow a defaulted one,
    /// excluding `self`).
    #[error(
        "Parameter '{param}' on '{function}' has no default value but follows a parameter that does — defaults must be positional from the right"
    )]
    RequiredParamAfterDefault {
        function: String,
        param: String,
        span: Span,
    },

    /// nil literal assigned to a non-optional type.
    #[error("Cannot assign nil to non-optional type '{expected}'")]
    NilAssignedToNonOptional { expected: String, span: Span },

    /// Optional type used where a non-optional is required.
    #[error("Cannot use optional type '{actual}' where non-optional '{expected}' is required")]
    OptionalUsedAsNonOptional {
        actual: String,
        expected: String,
        span: Span,
    },

    /// A trait implementation is missing a method required by the trait.
    #[error("Missing method '{method}' required by trait '{trait_name}'")]
    MissingTraitMethod {
        method: String,
        trait_name: String,
        span: Span,
    },

    /// A method's signature in an impl block does not match the trait's declaration.
    #[error(
        "Method '{method}' signature does not match trait '{trait_name}': expected {expected}, found {actual}"
    )]
    TraitMethodSignatureMismatch {
        method: String,
        trait_name: String,
        expected: String,
        actual: String,
        span: Span,
    },

    // Function overload errors
    /// More than one overload of a function matches the call arguments.
    #[error("Ambiguous call to '{function}': multiple overloads match")]
    AmbiguousCall { function: String, span: Span },

    /// No overload of a function matches the call arguments.
    #[error("No matching overload for '{function}' with the given arguments")]
    NoMatchingOverload { function: String, span: Span },

    // Enum type inference errors
    #[error("Cannot infer enum type for variant '.{variant}' from context")]
    CannotInferEnumType { variant: String, span: Span },

    /// A closure parameter with no type, in a position that gives the
    /// closure no type. A `let` annotation, a declared parameter or
    /// field type, or a declared return type gives one.
    #[error("Closure parameter '{param}' needs a type")]
    ClosureParameterNeedsType { param: String, span: Span },

    /// A dictionary key typed `F32` or `F64`.
    #[error("'{key_type}' cannot be a dictionary key")]
    FloatDictionaryKey { key_type: String, span: Span },

    /// A sequence that nothing consumes. It never runs.
    #[error("This sequence is never consumed")]
    SeqNotConsumed { span: Span },

    /// A sequence read after something already consumed it.
    #[error("Sequence '{name}' was already consumed")]
    SeqUsedTwice { name: String, span: Span },

    /// A sequence type somewhere it cannot be stored or returned.
    #[error("A sequence cannot be {position}")]
    SeqInvalidPosition { position: String, span: Span },

    // Function validation errors
    #[error("Function '{function}' has return type {expected} but body has type {actual}")]
    FunctionReturnTypeMismatch {
        function: String,
        expected: String,
        actual: String,
        span: Span,
    },

    /// Expression nesting exceeded the compiler recursion limit.
    #[error("Expression nesting exceeded the compiler recursion limit")]
    ExpressionDepthExceeded { span: Span },

    /// Module contains more definitions than the ID space allows (> `u32::MAX`).
    #[error("Module contains too many {kind} definitions")]
    TooManyDefinitions { kind: &'static str, span: Span },

    /// Attempted to access a private item from outside its defining module.
    #[error("'{name}' is private and cannot be accessed from outside its module")]
    VisibilityViolation { name: String, span: Span },

    /// A closure returned from a function captures a binding that does not
    /// outlive the function. Only `sink` parameters and outer-scope bindings
    /// may be captured by an escaping closure.
    #[error("Returned closure captures '{binding}' which does not outlive the function")]
    ClosureCaptureEscapesLocalBinding { binding: String, span: Span },

    /// A compiler invariant was violated during lowering or analysis. This is
    /// always a bug in the compiler itself — the `detail` field documents
    /// which invariant failed so it can be reported and fixed.
    #[error("Internal compiler error: {detail}")]
    InternalError { detail: String, span: Span },

    /// An integer literal does not fit in its declared (or default) target
    /// primitive — e.g. `2147483648I32` exceeds `i32::MAX`, or an unsuffixed
    /// `9_999_999_999` exceeds the `I32` default.
    #[error("Integer literal {written} does not fit in {target:?}")]
    NumericOverflow {
        written: String,
        target: PrimitiveType,
        span: Span,
    },

    /// A `pub` struct or `pub` enum variant declares a field whose type is
    /// a closure. Closures are an internal abstraction; they cannot be part
    /// of a publicly exposed type because they have no stable representation
    /// across the module / backend boundary.
    #[error("'{owner}' is public and cannot have closure-typed field '{field}'")]
    PublicClosureField {
        /// Human-readable identity of the offending item, e.g.
        /// `"struct Form"` or `"enum Event variant submitted"`.
        owner: String,
        /// Name of the closure-typed field.
        field: String,
        span: Span,
    },
}

/// Result type for compiler operations
pub type CompilerResult<T> = Result<T, Vec<CompilerError>>;
