//! Abstract Syntax Tree (AST) for FormaLang
//!
//! # Invocation Disambiguation
//!
//! FormaLang uses a two-phase approach for handling struct instantiation and function calls:
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: Ident,
    pub constraints: Vec<GenericConstraint>, // e.g., [Container] for T: Container
    pub span: Span,
}

/// Constraint on a generic parameter (e.g., Container in T: Container)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GenericConstraint {
    Trait(Ident), // Trait bound: T: TraitName
}

/// Root node representing a complete .fv file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// Top-level statement (use, let, or definition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Use(UseStmt),
    Let(Box<LetBinding>), // Boxed to reduce enum size (LetBinding is 576+ bytes)
    Definition(Box<Definition>), // Boxed to reduce enum size (Definition is 592+ bytes)
}

/// Definition (trait, struct, impl, enum, or module)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,  // pub
    Private, // default
}

/// Use statement (import items from modules)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseStmt {
    pub visibility: Visibility,
    pub path: Vec<Ident>,
    pub items: UseItems,
    pub span: Span,
}

/// Items to import (single, multiple, or glob)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UseItems {
    Single(Ident),
    Multiple(Vec<Ident>),
    /// Glob import (`use module::*`) - imports all public symbols
    Glob,
}

/// Let binding (file-level constant)
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: Option<Ident>, // Trait being implemented (None for inherent impl)
    pub name: Ident,               // Struct/enum name being implemented
    pub generics: Vec<GenericParam>, // Type parameters
    pub functions: Vec<FnDef>,     // Function definitions
    pub span: Span,
}

/// Function definition (inside impl blocks)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<FnParam>,      // Parameters (first is typically `self`)
    pub return_type: Option<Type>, // Return type (None = unit/void)
    pub body: Expr,                // Function body expression
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnParam {
    pub name: Ident,
    pub ty: Option<Type>,      // None for `self` parameter
    pub default: Option<Expr>, // Default value expression
    pub span: Span,
}

/// Enum definition (sum type)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>, // Generic parameters
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// Enum variant (with optional named associated data)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<FieldDef>, // Named fields (empty for simple variants)
    pub span: Span,
}

/// Module definition (namespace for grouping types)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub definitions: Vec<Definition>, // Nested definitions (trait, model, view, enum, module)
    pub span: Span,
}

/// Field definition (used in traits)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// Type expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Primitive(PrimitiveType),
    Ident(Ident), // Type reference (trait, model, or enum)

    // Generic type application: Box<String> or Container<T>
    Generic {
        name: Ident,     // The generic type name (e.g., "Box")
        args: Vec<Type>, // Type arguments (e.g., [String])
        span: Span,
    },

    Array(Box<Type>),       // Array type: [T]
    Optional(Box<Type>),    // Optional type: T?
    Tuple(Vec<TupleField>), // Named tuple type: (name1: T1, name2: T2)

    // Dictionary type: [K: V]
    Dictionary {
        key: Box<Type>,
        value: Box<Type>,
    },

    // Closure type: () -> T, T -> U, or T, U -> V
    Closure {
        params: Vec<Type>, // Parameter types (empty for () -> T)
        ret: Box<Type>,    // Return type
    },

    // Reference to a type parameter: T in Box<T>(value: T)
    TypeParameter(Ident),
}

/// Named tuple field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TupleField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// Primitive types
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    // Literals (remain in final AST)
    Literal(Literal),

    /// Invocation expression: struct instantiation or function call
    ///
    /// The parser cannot distinguish between `Point(x: 1)` (struct) and `max(a, b)` (function)
    /// syntactically. Semantic analysis resolves this by looking up the name in the symbol table:
    /// - If it's a struct → struct instantiation (requires named args, type_args and mounts valid)
    /// - If it's a function → function call (uses positional args, type_args and mounts must be empty)
    Invocation {
        /// The name/path being invoked (struct name or function name)
        /// Can be module-qualified: `module::Name`
        path: Vec<Ident>,
        /// Generic type arguments (only valid for struct instantiation)
        type_args: Vec<Type>,
        /// Arguments: Some(name) for named args, None for positional args
        /// Structs require named args, functions use positional args
        args: Vec<(Option<Ident>, Expr)>,
        /// Mount field arguments (only valid for struct instantiation)
        mounts: Vec<(Ident, Expr)>,
        span: Span,
    },

    EnumInstantiation {
        enum_name: Ident,
        variant: Ident,
        data: Vec<(Ident, Expr)>, // Named parameters: (field_name, value)
        span: Span,
    },

    // Inferred enum instantiation: .variant(...) where enum type is inferred from context
    InferredEnumInstantiation {
        variant: Ident,           // Variant name (without enum name)
        data: Vec<(Ident, Expr)>, // Named parameters: (field_name, value)
        span: Span,
    },

    // Array literal (remains in final AST)
    Array {
        elements: Vec<Expr>,
        span: Span,
    },

    // Tuple literal (remains in final AST)
    Tuple {
        fields: Vec<(Ident, Expr)>, // Named fields: (name1: expr1, name2: expr2)
        span: Span,
    },

    // Reference (remains in final AST)
    Reference {
        path: Vec<Ident>, // e.g., user.name or UserType::admin
        span: Span,
    },

    // Binary operation (evaluated by evaluator crate)
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
        span: Span,
    },

    // Unary operation (evaluated by evaluator crate)
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expr>,
        span: Span,
    },

    // For expression (validated by semantic analyzer, expanded by codegen)
    ForExpr {
        var: Ident,
        collection: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },

    // If expression (validated by semantic analyzer, expanded by codegen)
    IfExpr {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },

    // Match expression (validated by semantic analyzer, expanded by codegen)
    MatchExpr {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    // Grouped expression (parentheses)
    Group {
        expr: Box<Expr>,
        span: Span,
    },

    // Dictionary literal: ["key": value, "key2": value2] or [:]
    DictLiteral {
        entries: Vec<(Expr, Expr)>, // Key-value pairs
        span: Span,
    },

    // Dictionary access: dict["key"] or dict[index]
    DictAccess {
        dict: Box<Expr>,
        key: Box<Expr>,
        span: Span,
    },

    // Field access on arbitrary expressions: expr.field
    // Used when the base is not a simple reference (e.g., (-chord).y, (a + b).len)
    FieldAccess {
        object: Box<Expr>,
        field: Ident,
        span: Span,
    },

    // Closure expression: x -> expr, x, y -> expr, () -> expr, x: T -> expr
    ClosureExpr {
        params: Vec<ClosureParam>, // Parameters (empty for () -> expr)
        body: Box<Expr>,
        span: Span,
    },

    // Let expression: let pattern = value, let pattern: Type = value, let mut pattern = value
    // Local binding inside blocks (for, if, match, mount children)
    LetExpr {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>, // Optional type annotation
        value: Box<Expr>,
        body: Box<Expr>, // Continuation expression after the let
        span: Span,
    },

    /// Method call: expr.method(arg1, arg2, ...)
    ///
    /// Methods are always called on a receiver expression, so there's no ambiguity
    /// with struct instantiation. Uses positional arguments since builtins don't
    /// have parameter names.
    MethodCall {
        receiver: Box<Expr>, // The object/value to call method on
        method: Ident,       // Method name
        args: Vec<Expr>,     // Positional arguments
        span: Span,
    },

    /// Block expression: { let x = 1; let y = 2; x + y }
    ///
    /// A sequence of let bindings followed by a final expression.
    /// The final expression's value becomes the block's value.
    Block {
        statements: Vec<BlockStatement>,
        result: Box<Expr>, // Final expression (the block's value)
        span: Span,
    },
}

/// A statement within a block expression
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>, // Optional type annotation
    pub span: Span,
}

/// Literal values
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    /// Negation: -x
    Neg,
    /// Logical not: !x
    Not,
}

/// Match arm
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

/// Pattern (for match expressions)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    Variant {
        name: Ident,
        bindings: Vec<Ident>, // For associated data
    },
    /// Wildcard pattern: `_` matches anything
    Wildcard,
}

/// Binding pattern (for let bindings with destructuring)
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
        elements: Vec<BindingPattern>,
        span: Span,
    },
}

/// Element in an array destructuring pattern
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructPatternField {
    /// Field name to destructure
    pub name: Ident,
    /// Optional rename: `name as alias`
    pub alias: Option<Ident>,
}

/// Identifier with source location
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
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(lit) => match lit {
                Literal::Nil => Span::default(),
                _ => Span::default(), // Will be set during parsing
            },
            Expr::Invocation { span, .. } => *span,
            Expr::EnumInstantiation { span, .. } => *span,
            Expr::InferredEnumInstantiation { span, .. } => *span,
            Expr::Array { span, .. } => *span,
            Expr::Tuple { span, .. } => *span,
            Expr::Reference { span, .. } => *span,
            Expr::BinaryOp { span, .. } => *span,
            Expr::UnaryOp { span, .. } => *span,
            Expr::ForExpr { span, .. } => *span,
            Expr::IfExpr { span, .. } => *span,
            Expr::MatchExpr { span, .. } => *span,
            Expr::Group { span, .. } => *span,
            Expr::DictLiteral { span, .. } => *span,
            Expr::DictAccess { span, .. } => *span,
            Expr::FieldAccess { span, .. } => *span,
            Expr::ClosureExpr { span, .. } => *span,
            Expr::LetExpr { span, .. } => *span,
            Expr::MethodCall { span, .. } => *span,
            Expr::Block { span, .. } => *span,
        }
    }
}

impl BinaryOperator {
    /// Get operator precedence (higher = tighter binding)
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOperator::Range => 0, // Lowest precedence
            BinaryOperator::Or => 1,
            BinaryOperator::And => 2,
            BinaryOperator::Eq | BinaryOperator::Ne => 3,
            BinaryOperator::Lt | BinaryOperator::Gt | BinaryOperator::Le | BinaryOperator::Ge => 4,
            BinaryOperator::Add | BinaryOperator::Sub => 5,
            BinaryOperator::Mul | BinaryOperator::Div | BinaryOperator::Mod => 6,
        }
    }

    /// Check if operator is left-associative
    pub fn is_left_associative(&self) -> bool {
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
    fn test_binary_operator_precedence_all() {
        assert_eq!(BinaryOperator::Or.precedence(), 1);
        assert_eq!(BinaryOperator::And.precedence(), 2);
        assert_eq!(BinaryOperator::Eq.precedence(), 3);
        assert_eq!(BinaryOperator::Ne.precedence(), 3);
        assert_eq!(BinaryOperator::Lt.precedence(), 4);
        assert_eq!(BinaryOperator::Gt.precedence(), 4);
        assert_eq!(BinaryOperator::Le.precedence(), 4);
        assert_eq!(BinaryOperator::Ge.precedence(), 4);
        assert_eq!(BinaryOperator::Add.precedence(), 5);
        assert_eq!(BinaryOperator::Sub.precedence(), 5);
        assert_eq!(BinaryOperator::Mul.precedence(), 6);
        assert_eq!(BinaryOperator::Div.precedence(), 6);
        assert_eq!(BinaryOperator::Mod.precedence(), 6);
    }

    #[test]
    fn test_binary_operator_precedence_order() {
        // Verify multiplicative > additive > comparison > equality > and > or
        assert!(BinaryOperator::Mul.precedence() > BinaryOperator::Add.precedence());
        assert!(BinaryOperator::Add.precedence() > BinaryOperator::Lt.precedence());
        assert!(BinaryOperator::Lt.precedence() > BinaryOperator::Eq.precedence());
        assert!(BinaryOperator::Eq.precedence() > BinaryOperator::And.precedence());
        assert!(BinaryOperator::And.precedence() > BinaryOperator::Or.precedence());
    }

    #[test]
    fn test_binary_operator_is_left_associative() {
        assert!(BinaryOperator::Add.is_left_associative());
        assert!(BinaryOperator::Sub.is_left_associative());
        assert!(BinaryOperator::Mul.is_left_associative());
        assert!(BinaryOperator::Div.is_left_associative());
        assert!(BinaryOperator::Mod.is_left_associative());
        assert!(BinaryOperator::And.is_left_associative());
        assert!(BinaryOperator::Or.is_left_associative());
        assert!(BinaryOperator::Eq.is_left_associative());
        assert!(BinaryOperator::Ne.is_left_associative());
        assert!(BinaryOperator::Lt.is_left_associative());
        assert!(BinaryOperator::Gt.is_left_associative());
        assert!(BinaryOperator::Le.is_left_associative());
        assert!(BinaryOperator::Ge.is_left_associative());
    }

    // =========================================================================
    // Expr::span() Tests
    // =========================================================================

    #[test]
    fn test_expr_span_literal_nil() {
        let expr = Expr::Literal(Literal::Nil);
        let _ = expr.span(); // Just verify it doesn't panic
    }

    #[test]
    fn test_expr_span_literal_other() {
        let expr = Expr::Literal(Literal::String("test".to_string()));
        let _ = expr.span();
    }

    #[test]
    fn test_expr_span_invocation() {
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
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_enum_instantiation() {
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
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_inferred_enum() {
        let test_span = Span::from_range(0, 5);
        let expr = Expr::InferredEnumInstantiation {
            variant: Ident {
                name: "red".to_string(),
                span: Span::default(),
            },
            data: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_array() {
        let test_span = Span::from_range(100, 200);
        let expr = Expr::Array {
            elements: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_tuple() {
        let test_span = Span::from_range(50, 60);
        let expr = Expr::Tuple {
            fields: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_reference() {
        let test_span = Span::from_range(30, 40);
        let expr = Expr::Reference {
            path: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_binary_op() {
        let test_span = Span::from_range(70, 80);
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Literal(Literal::Number(1.0))),
            op: BinaryOperator::Add,
            right: Box::new(Expr::Literal(Literal::Number(2.0))),
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_for_expr() {
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
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_if_expr() {
        let test_span = Span::from_range(110, 120);
        let expr = Expr::IfExpr {
            condition: Box::new(Expr::Literal(Literal::Boolean(true))),
            then_branch: Box::new(Expr::Literal(Literal::Nil)),
            else_branch: None,
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_match_expr() {
        let test_span = Span::from_range(130, 140);
        let expr = Expr::MatchExpr {
            scrutinee: Box::new(Expr::Literal(Literal::Nil)),
            arms: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_group() {
        let test_span = Span::from_range(150, 160);
        let expr = Expr::Group {
            expr: Box::new(Expr::Literal(Literal::Number(42.0))),
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_dict_literal() {
        let test_span = Span::from_range(210, 220);
        let expr = Expr::DictLiteral {
            entries: vec![],
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_dict_access() {
        let test_span = Span::from_range(230, 240);
        let expr = Expr::DictAccess {
            dict: Box::new(Expr::Literal(Literal::Nil)),
            key: Box::new(Expr::Literal(Literal::String("key".to_string()))),
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_closure() {
        let test_span = Span::from_range(250, 260);
        let expr = Expr::ClosureExpr {
            params: vec![],
            body: Box::new(Expr::Literal(Literal::Number(0.0))),
            span: test_span,
        };
        assert_eq!(expr.span(), test_span);
    }

    #[test]
    fn test_expr_span_let_expr() {
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
        assert_eq!(expr.span(), test_span);
    }
}
