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
//!      type arguments and mount fields.
//!    - **Function call**: Uses positional arguments, type arguments and mounts are rejected.
//!
//! This approach follows Rust's model where the same syntax can represent different constructs
//! depending on what the name resolves to.
//!
//! # Argument Representation
//!
//! There are two argument representations in the AST:
//!
//! - [`Expr::Invocation`] uses `Vec<(Option<Ident>, Expr)>` where `Some(name)` indicates a
//!   named argument and `None` indicates a positional argument. This allows the parser to
//!   accept both styles, with semantic analysis enforcing that structs require named args.
//!
//! - [`Expr::MethodCall`] uses `Vec<Expr>` (positional only) because method calls are for
//!   builtin methods which don't have parameter names in their signatures.

use crate::location::Span;
use serde::{Deserialize, Serialize};

/// Generic type parameter (e.g., T in model Box<T>)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: Ident,
    pub constraints: Vec<GenericConstraint>, // e.g., [Container] for T: Container
    pub span: Span,
}

/// Constraint on a generic parameter (e.g., Container in T: Container)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenericConstraint {
    Trait(Ident), // Trait bound: T: TraitName
}

/// Root node representing a complete .fv file
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// Top-level statement (use, let, or definition)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Use(UseStmt),
    Let(Box<LetBinding>), // Boxed to reduce enum size (LetBinding is 576+ bytes)
    Definition(Box<Definition>), // Boxed to reduce enum size (Definition is 592+ bytes)
}

/// Definition (trait, struct, impl, enum, or module)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Definition {
    Trait(TraitDef),
    Struct(StructDef),
    Impl(ImplDef),
    Enum(EnumDef),
    Module(ModuleDef),
    /// Standalone function definition (not inside impl block)
    Function(Box<FunctionDef>), // Boxed to reduce enum size (FunctionDef is 592+ bytes)
}

/// Standalone function definition with visibility
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Expr,
    pub span: Span,
}

/// Visibility modifier
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,  // pub
    Private, // default
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
    /// Optional explicit type annotation (e.g., `let x: String = "hello"`)
    pub type_annotation: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

/// Trait definition (unified - no model/view distinction)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>, // Generic parameters
    pub traits: Vec<Ident>,          // Trait composition (A + B + C)
    pub fields: Vec<FieldDef>,       // Regular field requirements
    pub mount_fields: Vec<FieldDef>, // Mount field requirements
    pub span: Span,
}

/// Struct definition (unified data and UI component type)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,    // Generic parameters
    pub traits: Vec<Ident>,             // Implemented traits (A + B + C)
    pub fields: Vec<StructField>,       // Regular fields
    pub mount_fields: Vec<StructField>, // Mount fields (with mount keyword)
    pub span: Span,
}

/// Struct field (with optional and default support)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub optional: bool, // true if Type?
    pub default: Option<Expr>,
    pub span: Span,
}

/// Impl block definition (implementation body for structs)
///
/// Supports two forms:
/// - `impl Type { ... }` - inherent implementation
/// - `impl Trait for Type { ... }` - trait implementation
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: Option<Ident>, // Trait being implemented (None for inherent impl)
    pub name: Ident,               // Struct/enum name being implemented
    pub generics: Vec<GenericParam>, // Type parameters
    pub functions: Vec<FnDef>,     // Function definitions
    pub span: Span,
}

/// Function definition (inside impl blocks)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<FnParam>,      // Parameters (first is typically `self`)
    pub return_type: Option<Type>, // Return type (None = unit/void)
    pub body: Expr,                // Function body expression
    pub span: Span,
}

/// Function parameter
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnParam {
    pub name: Ident,
    pub ty: Option<Type>,      // None for `self` parameter
    pub default: Option<Expr>, // Default value expression
    pub span: Span,
}

/// Enum definition (sum type)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>, // Generic parameters
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// Enum variant (with optional named associated data)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<FieldDef>, // Named fields (empty for simple variants)
    pub span: Span,
}

/// Module definition (namespace for grouping types)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub definitions: Vec<Definition>, // Nested definitions (trait, model, view, enum, module)
    pub span: Span,
}

/// Field definition (used in traits)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// Type expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Primitive(PrimitiveType),
    Ident(Ident), // Type reference (trait, model, or enum)

    // Generic type application: Box<String> or Container<T>
    Generic {
        name: Ident,     // The generic type name (e.g., "Box")
        args: Vec<Self>, // Type arguments (e.g., [String])
        span: Span,
    },

    Array(Box<Self>),       // Array type: [T]
    Optional(Box<Self>),    // Optional type: T?
    Tuple(Vec<TupleField>), // Named tuple type: (name1: T1, name2: T2)

    // Dictionary type: [K: V]
    Dictionary {
        key: Box<Self>,
        value: Box<Self>,
    },

    // Closure type: () -> T, T -> U, or T, U -> V
    Closure {
        params: Vec<Self>, // Parameter types (empty for () -> T)
        ret: Box<Self>,    // Return type
    },

    // Reference to a type parameter: T in Box<T>(value: T)
    TypeParameter(Ident),
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
    Boolean,
    Path,
    Regex,
    /// Uninhabited type - has no values, used for terminal structs
    Never,

    // GPU scalar types
    F32,
    I32,
    U32,
    Bool,

    // GPU vector types (float)
    Vec2,
    Vec3,
    Vec4,

    // GPU vector types (signed int)
    IVec2,
    IVec3,
    IVec4,

    // GPU vector types (unsigned int)
    UVec2,
    UVec3,
    UVec4,

    // GPU matrix types
    Mat2,
    Mat3,
    Mat4,
}

/// Expression (compile-time evaluated)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    // Literals (remain in final AST)
    Literal(Literal),

    /// Invocation expression: struct instantiation or function call
    ///
    /// The parser cannot distinguish between `Point(x: 1)` (struct) and `max(a, b)` (function)
    /// syntactically. Semantic analysis resolves this by looking up the name in the symbol table:
    /// - If it's a struct → struct instantiation (requires named args, `type_args` and mounts valid)
    /// - If it's a function → function call (uses positional args, `type_args` and mounts must be empty)
    Invocation {
        /// The name/path being invoked (struct name or function name)
        /// Can be module-qualified: `module::Name`
        path: Vec<Ident>,
        /// Generic type arguments (only valid for struct instantiation)
        type_args: Vec<Type>,
        /// Arguments: Some(name) for named args, None for positional args
        /// Structs require named args, functions use positional args
        args: Vec<(Option<Ident>, Self)>,
        /// Mount field arguments (only valid for struct instantiation)
        mounts: Vec<(Ident, Self)>,
        span: Span,
    },

    EnumInstantiation {
        enum_name: Ident,
        variant: Ident,
        data: Vec<(Ident, Self)>, // Named parameters: (field_name, value)
        span: Span,
    },

    // Inferred enum instantiation: .variant(...) where enum type is inferred from context
    InferredEnumInstantiation {
        variant: Ident,           // Variant name (without enum name)
        data: Vec<(Ident, Self)>, // Named parameters: (field_name, value)
        span: Span,
    },

    // Array literal (remains in final AST)
    Array {
        elements: Vec<Self>,
        span: Span,
    },

    // Tuple literal (remains in final AST)
    Tuple {
        fields: Vec<(Ident, Self)>, // Named fields: (name1: expr1, name2: expr2)
        span: Span,
    },

    // Reference (remains in final AST)
    Reference {
        path: Vec<Ident>, // e.g., user.name or UserType::admin
        span: Span,
    },

    // Binary operation (evaluated by evaluator crate)
    BinaryOp {
        left: Box<Self>,
        op: BinaryOperator,
        right: Box<Self>,
        span: Span,
    },

    // Unary operation (evaluated by evaluator crate)
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Self>,
        span: Span,
    },

    // For expression (validated by semantic analyzer, expanded by codegen)
    ForExpr {
        var: Ident,
        collection: Box<Self>,
        body: Box<Self>,
        span: Span,
    },

    // If expression (validated by semantic analyzer, expanded by codegen)
    IfExpr {
        condition: Box<Self>,
        then_branch: Box<Self>,
        else_branch: Option<Box<Self>>,
        span: Span,
    },

    // Match expression (validated by semantic analyzer, expanded by codegen)
    MatchExpr {
        scrutinee: Box<Self>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    // Grouped expression (parentheses)
    Group {
        expr: Box<Self>,
        span: Span,
    },

    // Dictionary literal: ["key": value, "key2": value2] or [:]
    DictLiteral {
        entries: Vec<(Self, Self)>, // Key-value pairs
        span: Span,
    },

    // Dictionary access: dict["key"] or dict[index]
    DictAccess {
        dict: Box<Self>,
        key: Box<Self>,
        span: Span,
    },

    // Field access on arbitrary expressions: expr.field
    // Used when the base is not a simple reference (e.g., (-chord).y, (a + b).len)
    FieldAccess {
        object: Box<Self>,
        field: Ident,
        span: Span,
    },

    // Closure expression: x -> expr, x, y -> expr, () -> expr, x: T -> expr
    ClosureExpr {
        params: Vec<ClosureParam>, // Parameters (empty for () -> expr)
        body: Box<Self>,
        span: Span,
    },

    // Let expression: let pattern = value, let pattern: Type = value, let mut pattern = value
    // Local binding inside blocks (for, if, match, mount children)
    LetExpr {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>, // Optional type annotation
        value: Box<Self>,
        body: Box<Self>, // Continuation expression after the let
        span: Span,
    },

    /// Method call: expr.method(arg1, arg2, ...)
    ///
    /// Methods are always called on a receiver expression, so there's no ambiguity
    /// with struct instantiation. Uses positional arguments since builtins don't
    /// have parameter names.
    MethodCall {
        receiver: Box<Self>, // The object/value to call method on
        method: Ident,       // Method name
        args: Vec<Self>,     // Positional arguments
        span: Span,
    },

    /// Block expression: { let x = 1; let y = 2; x + y }
    ///
    /// A sequence of let bindings followed by a final expression.
    /// The final expression's value becomes the block's value.
    Block {
        statements: Vec<BlockStatement>,
        result: Box<Self>, // Final expression (the block's value)
        span: Span,
    },
}

/// A statement within a block expression
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockStatement {
    /// Let binding: let x = expr or let mut x = expr
    Let {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    /// Assignment: target = value
    /// Target must be a mutable binding or field
    Assign {
        target: Expr, // Reference path like `x` or `self.field`
        value: Expr,
        span: Span,
    },
    /// Expression statement (expression evaluated for side effects)
    Expr(Expr),
}

/// Closure parameter (name with optional type annotation)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>, // Optional type annotation
    pub span: Span,
}

/// Literal values
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    String(String),
    Number(f64),      // Also used for Factor values (validated in semantic analysis)
    UnsignedInt(u32), // GPU u32 literal with 'u' suffix
    SignedInt(i32),   // GPU i32 literal with 'i' suffix
    Boolean(bool),
    Regex { pattern: String, flags: String },
    Path(String),
    Nil,
}

/// Binary operators
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    // Logical
    And,
    Or,
    // Range
    Range, // start..end
}

/// Unary operators
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    /// Negation: -x
    Neg,
    /// Logical not: !x
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
    Variant {
        name: Ident,
        bindings: Vec<Ident>, // For associated data
    },
    /// Wildcard pattern: `_` matches anything
    Wildcard,
}

/// Binding pattern (for let bindings with destructuring)
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingPattern {
    /// Simple name binding: `let x = ...`
    Simple(Ident),
    /// Array destructuring: `let [a, b, ...rest] = ...`
    Array {
        elements: Vec<ArrayPatternElement>,
        span: Span,
    },
    /// Struct destructuring: `let {name, age as userAge} = ...`
    Struct {
        fields: Vec<StructPatternField>,
        span: Span,
    },
    /// Tuple destructuring (for enum associated data): `let (a, b) = ...`
    Tuple {
        elements: Vec<Self>,
        span: Span,
    },
}

/// Element in an array destructuring pattern
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrayPatternElement {
    /// Named binding: `a` in `[a, b]`
    Binding(BindingPattern),
    /// Rest pattern: `...rest` in `[a, ...rest]`
    Rest(Option<Ident>),
    /// Wildcard (ignore): `_` in `[_, b]`
    Wildcard,
}

/// Field in a struct destructuring pattern
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructPatternField {
    /// Field name to destructure
    pub name: Ident,
    /// Optional rename: `name as alias`
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
    pub fn span(&self) -> Span {
        match self {
            Self::Literal(lit) => match lit {
                Literal::Nil
                | Literal::String(_)
                | Literal::Number(_)
                | Literal::UnsignedInt(_)
                | Literal::SignedInt(_)
                | Literal::Boolean(_)
                | Literal::Regex { .. }
                | Literal::Path(_) => Span::default(), // Will be set during parsing
            },
            Self::Invocation { span, .. }
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
            Self::Range => 0, // Lowest precedence
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
        true // All operators are left-associative in FormaLang
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location::Span;

    // =========================================================================
    // BinaryOperator Tests
    // =========================================================================

    #[test]
    fn test_binary_operator_precedence_all() -> Result<(), Box<dyn std::error::Error>> {
        if BinaryOperator::Or.precedence() != 1 { return Err(format!("expected {:?}, got {:?}", 1, BinaryOperator::Or.precedence()).into()); }
        if BinaryOperator::And.precedence() != 2 { return Err(format!("expected {:?}, got {:?}", 2, BinaryOperator::And.precedence()).into()); }
        if BinaryOperator::Eq.precedence() != 3 { return Err(format!("expected {:?}, got {:?}", 3, BinaryOperator::Eq.precedence()).into()); }
        if BinaryOperator::Ne.precedence() != 3 { return Err(format!("expected {:?}, got {:?}", 3, BinaryOperator::Ne.precedence()).into()); }
        if BinaryOperator::Lt.precedence() != 4 { return Err(format!("expected {:?}, got {:?}", 4, BinaryOperator::Lt.precedence()).into()); }
        if BinaryOperator::Gt.precedence() != 4 { return Err(format!("expected {:?}, got {:?}", 4, BinaryOperator::Gt.precedence()).into()); }
        if BinaryOperator::Le.precedence() != 4 { return Err(format!("expected {:?}, got {:?}", 4, BinaryOperator::Le.precedence()).into()); }
        if BinaryOperator::Ge.precedence() != 4 { return Err(format!("expected {:?}, got {:?}", 4, BinaryOperator::Ge.precedence()).into()); }
        if BinaryOperator::Add.precedence() != 5 { return Err(format!("expected {:?}, got {:?}", 5, BinaryOperator::Add.precedence()).into()); }
        if BinaryOperator::Sub.precedence() != 5 { return Err(format!("expected {:?}, got {:?}", 5, BinaryOperator::Sub.precedence()).into()); }
        if BinaryOperator::Mul.precedence() != 6 { return Err(format!("expected {:?}, got {:?}", 6, BinaryOperator::Mul.precedence()).into()); }
        if BinaryOperator::Div.precedence() != 6 { return Err(format!("expected {:?}, got {:?}", 6, BinaryOperator::Div.precedence()).into()); }
        if BinaryOperator::Mod.precedence() != 6 { return Err(format!("expected {:?}, got {:?}", 6, BinaryOperator::Mod.precedence()).into()); }
        Ok(())
    }

    #[test]
    fn test_binary_operator_precedence_order() -> Result<(), Box<dyn std::error::Error>> {
        // Verify multiplicative > additive > comparison > equality > and > or
        if BinaryOperator::Mul.precedence() <= BinaryOperator::Add.precedence() { return Err("assertion failed".into()); }
        if BinaryOperator::Add.precedence() <= BinaryOperator::Lt.precedence() { return Err("assertion failed".into()); }
        if BinaryOperator::Lt.precedence() <= BinaryOperator::Eq.precedence() { return Err("assertion failed".into()); }
        if BinaryOperator::Eq.precedence() <= BinaryOperator::And.precedence() { return Err("assertion failed".into()); }
        if BinaryOperator::And.precedence() <= BinaryOperator::Or.precedence() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_binary_operator_is_left_associative() -> Result<(), Box<dyn std::error::Error>> {
        if !(BinaryOperator::Add.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Sub.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Mul.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Div.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Mod.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::And.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Or.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Eq.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Ne.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Lt.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Gt.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Le.is_left_associative()) { return Err("assertion failed".into()); }
        if !(BinaryOperator::Ge.is_left_associative()) { return Err("assertion failed".into()); }
        Ok(())
    }

    // =========================================================================
    // Expr::span() Tests
    // =========================================================================

    #[test]
    fn test_expr_span_literal_nil() -> Result<(), Box<dyn std::error::Error>> {
        let expr = Expr::Literal(Literal::Nil);
        let span = expr.span();
        if span != Span::default() {
            return Err(format!("Literal::Nil should return default span, got {span:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_expr_span_literal_other() -> Result<(), Box<dyn std::error::Error>> {
        let expr = Expr::Literal(Literal::String("test".to_string()));
        let span = expr.span();
        if span != Span::default() {
            return Err(format!("Literal::String should return default span, got {span:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_expr_span_invocation() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(10, 20);
        let expr = Expr::Invocation {
            path: vec![Ident {
                name: "Test".to_string(),
                span: Span::default(),
            }],
            type_args: vec![],
            args: vec![],
            mounts: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_enum_instantiation() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(5, 15);
        let expr = Expr::EnumInstantiation {
            enum_name: Ident {
                name: "Status".to_string(),
                span: Span::default(),
            },
            variant: Ident {
                name: "active".to_string(),
                span: Span::default(),
            },
            data: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_inferred_enum() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(0, 5);
        let expr = Expr::InferredEnumInstantiation {
            variant: Ident {
                name: "red".to_string(),
                span: Span::default(),
            },
            data: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_array() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(100, 200);
        let expr = Expr::Array {
            elements: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_tuple() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(50, 60);
        let expr = Expr::Tuple {
            fields: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_reference() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(30, 40);
        let expr = Expr::Reference {
            path: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_binary_op() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(70, 80);
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Literal(Literal::Number(1.0))),
            op: BinaryOperator::Add,
            right: Box::new(Expr::Literal(Literal::Number(2.0))),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_for_expr() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(90, 100);
        let expr = Expr::ForExpr {
            var: Ident {
                name: "x".to_string(),
                span: Span::default(),
            },
            collection: Box::new(Expr::Array {
                elements: vec![],
                span: Span::default(),
            }),
            body: Box::new(Expr::Literal(Literal::Nil)),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_if_expr() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(110, 120);
        let expr = Expr::IfExpr {
            condition: Box::new(Expr::Literal(Literal::Boolean(true))),
            then_branch: Box::new(Expr::Literal(Literal::Nil)),
            else_branch: None,
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_match_expr() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(130, 140);
        let expr = Expr::MatchExpr {
            scrutinee: Box::new(Expr::Literal(Literal::Nil)),
            arms: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_group() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(150, 160);
        let expr = Expr::Group {
            expr: Box::new(Expr::Literal(Literal::Number(42.0))),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_dict_literal() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(210, 220);
        let expr = Expr::DictLiteral {
            entries: vec![],
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_dict_access() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(230, 240);
        let expr = Expr::DictAccess {
            dict: Box::new(Expr::Literal(Literal::Nil)),
            key: Box::new(Expr::Literal(Literal::String("key".to_string()))),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_closure() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(250, 260);
        let expr = Expr::ClosureExpr {
            params: vec![],
            body: Box::new(Expr::Literal(Literal::Number(0.0))),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }

    #[test]
    fn test_expr_span_let_expr() -> Result<(), Box<dyn std::error::Error>> {
        let test_span = Span::from_range(270, 280);
        let expr = Expr::LetExpr {
            mutable: false,
            pattern: BindingPattern::Simple(Ident {
                name: "x".to_string(),
                span: Span::default(),
            }),
            ty: None,
            value: Box::new(Expr::Literal(Literal::Number(42.0))),
            body: Box::new(Expr::Literal(Literal::Nil)),
            span: test_span,
        };
        if expr.span() != test_span { return Err(format!("expected {:?}, got {:?}", test_span, expr.span()).into()); }
        Ok(())
    }
}
