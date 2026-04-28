//! Expressions, literals, and patterns.
//!
//! This module owns the runtime-shaped AST: [`Expr`] and its building
//! blocks ([`Literal`], [`NumberLiteral`], [`MatchArm`], [`Pattern`],
//! [`BindingPattern`], [`BlockStatement`], [`ClosureParam`]).
//! Re-exported from [`crate::ast`].

use crate::ast::{BinaryOperator, Ident, ParamConvention, PrimitiveType, Type, UnaryOperator};
use crate::location::Span;
use serde::{Deserialize, Serialize};

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
    Regex {
        pattern: String,
        flags: String,
    },
    Path(String),
    Nil,
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
