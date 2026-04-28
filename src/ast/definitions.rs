//! Top-level definitions: files, statements, items.
//!
//! This module owns the AST nodes that describe a parsed `.fv` file —
//! the [`File`] root, [`Statement`] variants, the [`Definition`] enum,
//! and each definition kind ([`TraitDef`], [`StructDef`], [`ImplDef`],
//! [`EnumDef`], [`ModuleDef`], [`FunctionDef`], plus their helpers).
//! Re-exported from [`crate::ast`].

use crate::ast::{
    AttributeAnnotation, BindingPattern, Expr, ExternAbi, GenericParam, Ident, ParamConvention,
    Type, Visibility,
};
use crate::location::Span;
use serde::{Deserialize, Serialize};

/// The current AST serialization format version.
///
/// Embedders use this to detect incompatible AST changes. Increment when making
/// breaking changes to any public AST type.
pub const FORMAT_VERSION: u32 = 1;

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
///     fn area(self) -> F64
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
    /// `fn` keyword. See [`crate::ast::FunctionAttribute`]. Each entry
    /// carries the span of the introducing keyword.
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
