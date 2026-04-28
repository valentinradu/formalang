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
//!
//! # Module layout
//!
//! - [`types`]   — [`Type`], generics, function attributes, extern ABI
//! - [`definitions`] — [`File`], [`Statement`], [`Definition`] and friends
//! - [`expressions`] — [`Expr`], [`Literal`], patterns, block statements
//!
//! Everything is re-exported here so callers continue to use
//! `crate::ast::Foo` paths.

mod definitions;
mod expressions;
mod types;

#[cfg(test)]
mod tests;

use crate::location::Span;
use serde::{Deserialize, Serialize};

pub use definitions::{
    Definition, EnumDef, EnumVariant, FieldDef, File, FnDef, FnParam, FnSig, FunctionDef,
    ImplDef, LetBinding, ModuleDef, Statement, StructDef, StructField, TraitDef, UseItems,
    UseStmt, FORMAT_VERSION,
};
pub use expressions::{
    ArrayPatternElement, BindingPattern, BlockStatement, ClosureParam, Expr, Literal, MatchArm,
    NumberLiteral, NumberSourceKind, NumericSuffix, Pattern, StructPatternField,
};
pub use types::{
    AttributeAnnotation, ExternAbi, FunctionAttribute, GenericConstraint, GenericParam,
    TupleField, Type,
};

/// Visibility modifier
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
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

/// Primitive types
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    String,
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

/// Unary operators
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Neg,
    Not,
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
