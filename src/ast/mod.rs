//! Abstract Syntax Tree (AST) for `FormaLang`
//!
//! # Invocation Disambiguation
//!
//! `FormaLang` uses a two-phase approach for handling struct instantiation and function calls:
//!
//! 1. **Parsing Phase**: Both struct instantiation (`Point(x: 1, y: 2)`) and function calls
//!    (`max(a, b)`) are parsed as a unified [`Expr::Invocation`] node. The parser cannot
//!    distinguish between them syntactically since both use `Name(args)` syntax.
//!
//! 2. **Semantic Analysis Phase**: The semantic analyzer looks up the name in the symbol table
//!    to determine whether it's a struct or function:
//!    - **Struct instantiation**: Requires named arguments (`field: value`), supports generic
//!      type arguments.
//!    - **Function call**: Uses positional or named arguments; type arguments are rejected.
//!
//! This approach follows Rust's model where the same syntax can represent different constructs
//! depending on what the name resolves to.

use crate::location::Span;
use serde::{Deserialize, Serialize};

/// The current AST serialization format version.
///
/// Embedders use this to detect incompatible AST changes. Increment when making
/// breaking changes to any public AST type.
pub const FORMAT_VERSION: u32 = 1;

/// Generic type parameter (e.g., T in `Box<T>`)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: Ident,
    pub constraints: Vec<GenericConstraint>,
    pub span: Span,
}

/// Constraint on a generic parameter.
///
/// `Trait { name, args }` represents a trait bound — `T: Foo` (with
/// `args: []`) or `T: Foo<X, Y>` (with concrete or generic-param type
/// arguments). The args slot lets generic-trait constraints survive
/// monomorphisation: `<T: Container<Number>>` instantiates Container
/// for `Number` and constrains T against that specialised trait.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GenericConstraint {
    Trait { name: Ident, args: Vec<Type> },
}

/// Root node representing a complete `.fv` file.
///
/// The `format_version` field allows embedders to detect AST format changes when
/// using the AST as a wire format. Currently [`FORMAT_VERSION`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    /// AST serialization format version. See [`FORMAT_VERSION`].
    pub format_version: u32,
    pub statements: Vec<Statement>,
    pub span: Span,
}

impl File {
    /// Create a new `File` with the current [`FORMAT_VERSION`].
    #[must_use]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "Vec<Statement> is not const-compatible"
    )]
    pub fn new(statements: Vec<Statement>, span: Span) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            statements,
            span,
        }
    }
}

/// Top-level statement (use, let, or definition)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Use(UseStmt),
    Let(Box<LetBinding>),
    Definition(Box<Definition>),
}

/// Definition (trait, struct, impl, enum, module, or function)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Definition {
    Trait(TraitDef),
    Struct(StructDef),
    Impl(ImplDef),
    Enum(EnumDef),
    Module(ModuleDef),
    /// Standalone function definition (not inside impl block)
    Function(Box<FunctionDef>),
}

/// Standalone function definition with visibility.
///
/// `body` is `None` for `extern fn` declarations.
/// `body` is `Some(_)` for regular functions.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    /// `None` for `extern fn`; `Some(_)` for regular functions.
    pub body: Option<Expr>,
    /// Calling convention for `extern fn` declarations. `Some(_)` is
    /// produced by `extern_fn_parser` (`extern fn`, `extern "C" fn`,
    /// `extern "system" fn`); `None` for regular functions. Tracked
    /// alongside `body` so the semantic layer can detect mismatches
    /// (extern with body, regular without) consistently — including
    /// under parser error recovery. Audit findings #28, Tier-1 item E.
    pub extern_abi: Option<ExternAbi>,
    /// Codegen attributes parsed as keyword prefixes (`inline`,
    /// `no_inline`, `cold`). Order is the source order; duplicates are
    /// preserved so semantic / backends can diagnose them. Each entry
    /// carries the span of the introducing keyword.
    pub attributes: Vec<AttributeAnnotation>,
    /// Joined `///` doc comments preceding this definition. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

impl FunctionDef {
    /// Whether this function was declared `extern`. Convenience wrapper
    /// over [`Self::extern_abi`] for the common boolean check.
    #[must_use]
    pub const fn is_extern(&self) -> bool {
        self.extern_abi.is_some()
    }
}

/// Visibility modifier
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
}

/// Codegen-hint attribute on a function or method declaration.
///
/// Surfaces source-level annotations like `inline fn foo()` /
/// `cold fn rare_path()` to backends so they can apply target-specific
/// inlining or branch-likelihood heuristics. The frontend does *not*
/// act on these — they are pass-through metadata.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionAttribute {
    /// Hint: inline this function at every call site when possible.
    Inline,
    /// Hint: do not inline this function.
    NoInline,
    /// Hint: this function is unlikely to be called (rarely-taken
    /// branch, error path). Backends typically place its body in a
    /// cold section and bias surrounding branches.
    Cold,
}

/// A `FunctionAttribute` together with the source span of the keyword
/// that introduced it.
///
/// AST-only wrapper — the IR drops the span and stores plain
/// `FunctionAttribute`s, since backends don't need parser locations.
/// Spans are preserved on the AST so a future diagnostic can point at
/// the offending `inline` / `cold` keyword (e.g. duplicate or
/// contradictory annotations).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttributeAnnotation {
    pub kind: FunctionAttribute,
    pub span: Span,
}

/// Calling convention for an extern function.
///
/// Carries enough information for backends targeting languages with
/// distinguished calling conventions (C, Win32 stdcall, etc.) to emit
/// the right call sequence and symbol mangling. The default — produced
/// by a bare `extern fn foo()` — is `C`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExternAbi {
    /// Plain C ABI. Default for `extern fn foo()` and `extern "C" fn foo()`.
    C,
    /// Platform "system" ABI (`stdcall` on Win32 x86, `C` elsewhere).
    /// Spelled `extern "system"` in source.
    System,
}

/// Use statement (import items from modules)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UseStmt {
    pub visibility: Visibility,
    pub path: Vec<Ident>,
    pub items: UseItems,
    pub span: Span,
}

/// Items to import (single, multiple, or glob)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UseItems {
    Single(Ident),
    Multiple(Vec<Ident>),
    /// Glob import (`use module::*`) - imports all public symbols
    Glob,
}

/// Let binding (file-level constant)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetBinding {
    pub visibility: Visibility,
    pub mutable: bool,
    pub pattern: BindingPattern,
    pub type_annotation: Option<Type>,
    pub value: Expr,
    /// Joined `///` doc comments preceding this binding. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Trait definition.
///
/// Traits declare field requirements and method signatures. Trait inheritance
/// (`trait A: B + C`) is supported.
///
/// # Example
///
/// ```formalang
/// trait Shape {
///     color: String
///     fn area(self) -> Number
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    /// Trait inheritance (`trait A: B + C`)
    pub traits: Vec<Ident>,
    /// Required field declarations
    pub fields: Vec<FieldDef>,
    /// Required method signatures (no default implementations)
    pub methods: Vec<FnSig>,
    /// Joined `///` doc comments preceding this trait. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Struct definition
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<StructField>,
    /// Joined `///` doc comments preceding this struct. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Struct field (with optional and default support)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub optional: bool,
    pub default: Option<Expr>,
    /// Joined `///` doc comments preceding this field. Audit2 B2.
    pub doc: Option<String>,
    pub span: Span,
}

/// Impl block definition.
///
/// - `impl Type { ... }` — inherent implementation
/// - `impl Trait for Type { ... }` — trait implementation
/// - `impl Trait<X> for Type { ... }` — generic-trait instantiation
/// - `extern impl Type { ... }` — extern method declarations (bodies must all be `None`)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: Option<Ident>,
    /// Type arguments applied to `trait_name` for generic-trait
    /// instantiations (`impl Foo<X> for Y`). Empty when the trait
    /// is non-generic, or when the impl is inherent (`trait_name`
    /// is `None`).
    pub trait_args: Vec<Type>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub functions: Vec<FnDef>,
    /// When `true`, this is `extern impl`; all contained `FnDef` bodies must be `None`.
    pub is_extern: bool,
    /// Joined `///` doc comments preceding this impl block. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Function definition (inside impl blocks).
///
/// `body` is `None` inside `extern impl` blocks; `Some(_)` in regular impl blocks.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    /// `None` in `extern impl`; `Some(_)` in regular impl.
    pub body: Option<Expr>,
    /// Codegen attributes (`inline`, `no_inline`, `cold`) preceding the
    /// `fn` keyword. See [`FunctionAttribute`]. Each entry carries the
    /// span of the introducing keyword.
    pub attributes: Vec<AttributeAnnotation>,
    /// Joined `///` doc comments preceding this method. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Function signature (used in trait method declarations — no body).
///
/// # Example
///
/// ```formalang
/// trait Drawable {
///     fn draw(self) -> Boolean
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnSig {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    /// Codegen attributes on the trait method declaration. Each entry
    /// carries the span of the introducing keyword.
    pub attributes: Vec<AttributeAnnotation>,
    pub span: Span,
}

/// Parameter passing convention (Mutable Value Semantics).
///
/// - `Let` — immutable borrow (default). The callee reads but cannot mutate.
/// - `Mut` — exclusive mutable. The callee may mutate; the updated value is returned
///   to the caller at the end of the call.
/// - `Sink` — ownership transfer. The caller gives up the value; the callee owns it.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ParamConvention {
    #[default]
    Let,
    Mut,
    Sink,
}

/// Function parameter
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnParam {
    /// Parameter passing convention (default: `Let`).
    pub convention: ParamConvention,
    /// External call-site label (if specified separately from the internal name).
    /// For `fn foo(label name: Type)`, `external_label` is `Some("label")` and `name` is `"name"`.
    pub external_label: Option<Ident>,
    pub name: Ident,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
    pub span: Span,
}

/// Enum definition (sum type)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    /// Joined `///` doc comments preceding this enum. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Enum variant (with optional named associated data)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

/// Module definition (namespace for grouping types)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub definitions: Vec<Definition>,
    /// Joined `///` doc comments preceding this module. Audit #51.
    pub doc: Option<String>,
    pub span: Span,
}

/// Field definition (used in traits and enum variants)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    /// Joined `///` doc comments preceding this field. Audit2 B2.
    pub doc: Option<String>,
    pub span: Span,
}

/// Type expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Primitive(PrimitiveType),
    Ident(Ident),

    Generic {
        name: Ident,
        args: Vec<Self>,
        span: Span,
    },

    Array(Box<Self>),
    Optional(Box<Self>),
    Tuple(Vec<TupleField>),

    Dictionary {
        key: Box<Self>,
        value: Box<Self>,
    },

    Closure {
        params: Vec<(ParamConvention, Self)>,
        ret: Box<Self>,
    },
}

/// Named tuple field
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TupleField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// Primitive types
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    String,
    Number,
    I32,
    I64,
    F32,
    F64,
    Boolean,
    Path,
    Regex,
    /// Uninhabited type — has no values
    Never,
}

/// Whether a numeric literal was written with integer or float syntax.
///
/// The lexer sets this based on the presence of `.` or `e` in the digit
/// slice — `42` is integer, `42.0` and `1e5` are float. Used to pick the
/// inference default for unsuffixed literals (`I32` for integer, `F64`
/// for float).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumberSourceKind {
    Integer,
    Float,
}

/// Width-tagged suffix attached to a numeric literal at parse time.
///
/// Source spelling is uppercase and adjacent to the digits (e.g. `42I32`,
/// `3.14F64`). The suffix is preserved through the AST so later passes can
/// type the literal without re-running inference defaults.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumericSuffix {
    I32,
    I64,
    F32,
    F64,
}

impl NumericSuffix {
    /// The [`PrimitiveType`] this suffix designates.
    #[must_use]
    pub const fn primitive(self) -> PrimitiveType {
        match self {
            Self::I32 => PrimitiveType::I32,
            Self::I64 => PrimitiveType::I64,
            Self::F32 => PrimitiveType::F32,
            Self::F64 => PrimitiveType::F64,
        }
    }
}

/// Parsed payload of a numeric literal: the `f64` value, an optional
/// source-level type suffix, and the integer-vs-float source-syntax kind.
///
/// A single field type that both `lexer::Token::Number` and `Literal::Number`
/// wrap. Single field because logos (used by the lexer) only supports
/// single-field token variants. Storage is `f64` for both integer and float
/// literals — values above 2^53 with `I64` suffix lose precision; specialising
/// the storage is tracked as a follow-up.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NumberLiteral {
    pub value: f64,
    pub suffix: Option<NumericSuffix>,
    pub kind: NumberSourceKind,
}

impl NumberLiteral {
    /// Construct an integer-syntax literal with no suffix.
    #[must_use]
    pub const fn unsuffixed(value: f64) -> Self {
        Self {
            value,
            suffix: None,
            kind: NumberSourceKind::Integer,
        }
    }

    /// Construct a float-syntax literal with no suffix.
    #[must_use]
    pub const fn unsuffixed_float(value: f64) -> Self {
        Self {
            value,
            suffix: None,
            kind: NumberSourceKind::Float,
        }
    }

    /// Construct with an explicit suffix. The source kind is inferred from
    /// the suffix's family (`I32`/`I64` → Integer, `F32`/`F64` → Float).
    #[must_use]
    pub const fn suffixed(value: f64, suffix: NumericSuffix) -> Self {
        let kind = match suffix {
            NumericSuffix::I32 | NumericSuffix::I64 => NumberSourceKind::Integer,
            NumericSuffix::F32 | NumericSuffix::F64 => NumberSourceKind::Float,
        };
        Self {
            value,
            suffix: Some(suffix),
            kind,
        }
    }

    /// Construct with the suffix already wrapped in an `Option` and an
    /// explicit source kind — convenience for the lexer, which determines
    /// both fields independently from the literal slice.
    #[must_use]
    pub const fn from_lex(
        value: f64,
        suffix: Option<NumericSuffix>,
        kind: NumberSourceKind,
    ) -> Self {
        Self {
            value,
            suffix,
            kind,
        }
    }

    /// The [`PrimitiveType`] this literal carries.
    ///
    /// When a suffix is present, that wins. Otherwise the source kind picks
    /// the default: `Integer` → [`PrimitiveType::I32`],
    /// `Float` → [`PrimitiveType::F64`].
    #[must_use]
    pub fn primitive_type(&self) -> PrimitiveType {
        self.suffix
            .map_or_else(|| self.kind.default_primitive(), NumericSuffix::primitive)
    }
}

impl NumberSourceKind {
    /// Default [`PrimitiveType`] for an unsuffixed literal of this kind.
    #[must_use]
    pub const fn default_primitive(self) -> PrimitiveType {
        match self {
            Self::Integer => PrimitiveType::I32,
            Self::Float => PrimitiveType::F64,
        }
    }
}

impl From<f64> for NumberLiteral {
    /// Default conversion infers the source kind from whether `value` has a
    /// fractional part — finite whole-number values get `Integer`, anything
    /// else (including `NaN` / infinities) gets `Float`. Convenient for tests
    /// and IR-internal construction; suffix is always `None`.
    fn from(value: f64) -> Self {
        let kind = if value.is_finite() && value.fract() == 0.0 {
            NumberSourceKind::Integer
        } else {
            NumberSourceKind::Float
        };
        Self {
            value,
            suffix: None,
            kind,
        }
    }
}

/// Expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal {
        value: Literal,
        span: Span,
    },

    /// Struct instantiation or function call (disambiguated by semantic analysis)
    Invocation {
        path: Vec<Ident>,
        type_args: Vec<Type>,
        args: Vec<(Option<Ident>, Self)>,
        span: Span,
    },

    EnumInstantiation {
        enum_name: Ident,
        variant: Ident,
        data: Vec<(Ident, Self)>,
        span: Span,
    },

    InferredEnumInstantiation {
        variant: Ident,
        data: Vec<(Ident, Self)>,
        span: Span,
    },

    Array {
        elements: Vec<Self>,
        span: Span,
    },

    Tuple {
        fields: Vec<(Ident, Self)>,
        span: Span,
    },

    Reference {
        path: Vec<Ident>,
        span: Span,
    },

    BinaryOp {
        left: Box<Self>,
        op: BinaryOperator,
        right: Box<Self>,
        span: Span,
    },

    UnaryOp {
        op: UnaryOperator,
        operand: Box<Self>,
        span: Span,
    },

    ForExpr {
        var: Ident,
        collection: Box<Self>,
        body: Box<Self>,
        span: Span,
    },

    IfExpr {
        condition: Box<Self>,
        then_branch: Box<Self>,
        else_branch: Option<Box<Self>>,
        span: Span,
    },

    MatchExpr {
        scrutinee: Box<Self>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    Group {
        expr: Box<Self>,
        span: Span,
    },

    DictLiteral {
        entries: Vec<(Self, Self)>,
        span: Span,
    },

    DictAccess {
        dict: Box<Self>,
        key: Box<Self>,
        span: Span,
    },

    FieldAccess {
        object: Box<Self>,
        field: Ident,
        span: Span,
    },

    ClosureExpr {
        params: Vec<ClosureParam>,
        /// Optional declared return type (`|x: T| -> R { body }`). `None`
        /// when the closure does not specify one and the type is inferred
        /// from the body.
        return_type: Option<Type>,
        body: Box<Self>,
        span: Span,
    },

    LetExpr {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>,
        value: Box<Self>,
        body: Box<Self>,
        span: Span,
    },

    /// Method call: `expr.method(arg1, label: arg2, ...)`
    MethodCall {
        receiver: Box<Self>,
        method: Ident,
        /// Arguments with optional call-site labels.
        args: Vec<(Option<Ident>, Self)>,
        span: Span,
    },

    /// Block expression: `{ let x = 1; x + 1 }`
    Block {
        statements: Vec<BlockStatement>,
        result: Box<Self>,
        span: Span,
    },
}

/// A statement within a block expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockStatement {
    Let {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}

/// Closure parameter
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosureParam {
    pub convention: ParamConvention,
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}

/// Literal values
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    String(String),
    /// Numeric literal: see [`NumberLiteral`] for the carried payload.
    Number(NumberLiteral),
    Boolean(bool),
    Regex { pattern: String, flags: String },
    Path(String),
    Nil,
}

/// Binary operators
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Range,
}

/// Unary operators
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Neg,
    Not,
}

/// Match arm
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

/// Pattern (for match expressions)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    Variant { name: Ident, bindings: Vec<Ident> },
    Wildcard,
}

/// Binding pattern (for let bindings with destructuring)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingPattern {
    Simple(Ident),
    Array {
        elements: Vec<ArrayPatternElement>,
        span: Span,
    },
    Struct {
        fields: Vec<StructPatternField>,
        span: Span,
    },
    Tuple {
        elements: Vec<Self>,
        span: Span,
    },
}

/// Element in an array destructuring pattern
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrayPatternElement {
    Binding(BindingPattern),
    Rest(Option<Ident>),
    Wildcard,
}

/// Field in a struct destructuring pattern
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructPatternField {
    pub name: Ident,
    pub alias: Option<Ident>,
}

/// Identifier with source location
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

impl Expr {
    /// Get the span of an expression
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Literal { span, .. }
            | Self::Invocation { span, .. }
            | Self::EnumInstantiation { span, .. }
            | Self::InferredEnumInstantiation { span, .. }
            | Self::Array { span, .. }
            | Self::Tuple { span, .. }
            | Self::Reference { span, .. }
            | Self::BinaryOp { span, .. }
            | Self::UnaryOp { span, .. }
            | Self::ForExpr { span, .. }
            | Self::IfExpr { span, .. }
            | Self::MatchExpr { span, .. }
            | Self::Group { span, .. }
            | Self::DictLiteral { span, .. }
            | Self::DictAccess { span, .. }
            | Self::FieldAccess { span, .. }
            | Self::ClosureExpr { span, .. }
            | Self::LetExpr { span, .. }
            | Self::MethodCall { span, .. }
            | Self::Block { span, .. } => *span,
        }
    }
}

impl BinaryOperator {
    /// Get operator precedence (higher = tighter binding)
    #[must_use]
    pub const fn precedence(&self) -> u8 {
        match self {
            Self::Range => 0,
            Self::Or => 1,
            Self::And => 2,
            Self::Eq | Self::Ne => 3,
            Self::Lt | Self::Gt | Self::Le | Self::Ge => 4,
            Self::Add | Self::Sub => 5,
            Self::Mul | Self::Div | Self::Mod => 6,
        }
    }

    /// Check if operator is left-associative
    #[must_use]
    pub const fn is_left_associative(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location::Span;

    #[test]
    fn test_binary_operator_precedence_all() -> Result<(), Box<dyn std::error::Error>> {
        if BinaryOperator::Or.precedence() != 1 {
            return Err(format!("expected 1, got {:?}", BinaryOperator::Or.precedence()).into());
        }
        if BinaryOperator::And.precedence() != 2 {
            return Err(format!("expected 2, got {:?}", BinaryOperator::And.precedence()).into());
        }
        if BinaryOperator::Eq.precedence() != 3 {
            return Err(format!("expected 3, got {:?}", BinaryOperator::Eq.precedence()).into());
        }
        if BinaryOperator::Ne.precedence() != 3 {
            return Err(format!("expected 3, got {:?}", BinaryOperator::Ne.precedence()).into());
        }
        if BinaryOperator::Lt.precedence() != 4 {
            return Err(format!("expected 4, got {:?}", BinaryOperator::Lt.precedence()).into());
        }
        if BinaryOperator::Gt.precedence() != 4 {
            return Err(format!("expected 4, got {:?}", BinaryOperator::Gt.precedence()).into());
        }
        if BinaryOperator::Le.precedence() != 4 {
            return Err(format!("expected 4, got {:?}", BinaryOperator::Le.precedence()).into());
        }
        if BinaryOperator::Ge.precedence() != 4 {
            return Err(format!("expected 4, got {:?}", BinaryOperator::Ge.precedence()).into());
        }
        if BinaryOperator::Add.precedence() != 5 {
            return Err(format!("expected 5, got {:?}", BinaryOperator::Add.precedence()).into());
        }
        if BinaryOperator::Sub.precedence() != 5 {
            return Err(format!("expected 5, got {:?}", BinaryOperator::Sub.precedence()).into());
        }
        if BinaryOperator::Mul.precedence() != 6 {
            return Err(format!("expected 6, got {:?}", BinaryOperator::Mul.precedence()).into());
        }
        if BinaryOperator::Div.precedence() != 6 {
            return Err(format!("expected 6, got {:?}", BinaryOperator::Div.precedence()).into());
        }
        if BinaryOperator::Mod.precedence() != 6 {
            return Err(format!("expected 6, got {:?}", BinaryOperator::Mod.precedence()).into());
        }
        Ok(())
    }

    #[test]
    fn test_binary_operator_precedence_order() -> Result<(), Box<dyn std::error::Error>> {
        if BinaryOperator::Mul.precedence() <= BinaryOperator::Add.precedence() {
            return Err("mul > add".into());
        }
        if BinaryOperator::Add.precedence() <= BinaryOperator::Lt.precedence() {
            return Err("add > lt".into());
        }
        if BinaryOperator::Lt.precedence() <= BinaryOperator::Eq.precedence() {
            return Err("lt > eq".into());
        }
        if BinaryOperator::Eq.precedence() <= BinaryOperator::And.precedence() {
            return Err("eq > and".into());
        }
        if BinaryOperator::And.precedence() <= BinaryOperator::Or.precedence() {
            return Err("and > or".into());
        }
        Ok(())
    }

    #[test]
    fn test_binary_operator_is_left_associative() -> Result<(), Box<dyn std::error::Error>> {
        if !BinaryOperator::Add.is_left_associative() {
            return Err("Add".into());
        }
        if !BinaryOperator::Mul.is_left_associative() {
            return Err("Mul".into());
        }
        if !BinaryOperator::Or.is_left_associative() {
            return Err("Or".into());
        }
        Ok(())
    }

    #[test]
    fn test_expr_span_literal() -> Result<(), Box<dyn std::error::Error>> {
        let expr = Expr::Literal {
            value: Literal::Nil,
            span: Span::default(),
        };
        if expr.span() != Span::default() {
            return Err("Literal should return default span".into());
        }
        Ok(())
    }

    #[test]
    fn test_expr_span_invocation() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(10, 20);
        let expr = Expr::Invocation {
            path: vec![Ident::new("Test", Span::default())],
            type_args: vec![],
            args: vec![],
            span: test_span,
        };
        if expr.span() != test_span {
            return Err(format!("expected {test_span:?}, got {:?}", expr.span()).into());
        }
        Ok(())
    }

    #[test]
    fn test_file_new_sets_format_version() -> Result<(), Box<dyn std::error::Error>> {
        let file = File::new(vec![], Span::default());
        if file.format_version != FORMAT_VERSION {
            return Err(format!("expected {FORMAT_VERSION}, got {}", file.format_version).into());
        }
        Ok(())
    }
}
