//! Type expressions and generics-related AST nodes.
//!
//! This module owns [`Type`] and the generics machinery
//! ([`GenericParam`], [`GenericConstraint`]), plus the codegen-attribute
//! types ([`FunctionAttribute`], [`AttributeAnnotation`], [`ExternAbi`])
//! that decorate function/method declarations. Re-exported from
//! [`crate::ast`].

use crate::ast::{Ident, ParamConvention, PrimitiveType};
use crate::location::Span;
use serde::{Deserialize, Serialize};

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
/// monomorphisation: `<T: Container<I32>>` instantiates Container
/// for `I32` and constrains T against that specialised trait.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GenericConstraint {
    Trait { name: Ident, args: Vec<Type> },
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
