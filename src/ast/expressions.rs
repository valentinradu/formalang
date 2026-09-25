//! Expressions, literals, and patterns.
//!
//! This module owns the runtime-shaped AST: [`Expr`] and its building
//! blocks ([`Literal`], [`MatchArm`], [`Pattern`], [`BindingPattern`],
//! [`BlockStatement`], [`ClosureParam`]). Numeric-literal types live in
//! [`super::numbers`]. Re-exported from [`crate::ast`].

use crate::ast::{BinaryOperator, Ident, NumberLiteral, ParamConvention, Type, UnaryOperator};
use crate::location::Span;

/// Expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
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

    /// A call of the value of an expression: `make()(4)`. The callee
    /// is not a plain name; `f(x)` is an [`Self::Invocation`].
    Call {
        callee: Box<Self>,
        args: Vec<(Option<Ident>, Self)>,
        span: Span,
    },

    /// Enum instantiation: `Status.active` or
    /// `Shape.circle(radius: 1.0)`. The semantic pass changes it into a
    /// field path or a method call when `enum_name` names a value.
    EnumInstantiation {
        /// The path of the enum as one name: `shapes::Status`.
        enum_name: Ident,
        /// The type arguments written on the path: `Maybe<I32>.none`.
        /// Empty when the path writes none.
        type_args: Vec<Type>,
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
        /// A declared return type. The closure syntax `(params) -> body`
        /// has no place for one, so the parser always sets `None` and the
        /// type comes from the body. The semantic pass checks the body
        /// against a type that a tool sets on a hand-built AST.
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

    /// Block expression. A line break separates two statements:
    /// `{ let x = 1` on one line, `x + 1 }` on the next.
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
            | Self::Call { span, .. }
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureParam {
    pub convention: ParamConvention,
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}

/// Literal values
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Literal {
    String(String),
    /// Numeric literal: see [`NumberLiteral`] for the carried payload.
    Number(NumberLiteral),
    Boolean(bool),
    Nil,
}

/// Match arm
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

/// Pattern (for match expressions).
///
/// The `bindings` of a variant are positional: the first name binds
/// the first field of the variant, whatever the names of the fields.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Variant { name: Ident, bindings: Vec<Ident> },
    Wildcard,
}

/// Binding pattern (for let bindings with destructuring)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayPatternElement {
    Binding(BindingPattern),
    Rest(Option<Ident>),
    Wildcard,
}

/// Field in a struct destructuring pattern
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructPatternField {
    pub name: Ident,
    pub alias: Option<Ident>,
}
