//! IR expression types.

use crate::ast::{BinaryOperator, Literal};

use super::{EnumId, ResolvedType, StructId};

/// An expression in the IR.
///
/// Every expression variant includes a `ty` field containing its resolved type.
/// Code generators can use this to emit properly typed code without re-inferring.
///
/// # Type Contract
///
/// The `ty` field is guaranteed to be correct after lowering from the AST.
/// For example:
/// - `Literal { value: Literal::Number(_), ty }` → `ty` is `ResolvedType::Primitive(Number)`
/// - `BinaryOp { op: Eq, .. }` → `ty` is `ResolvedType::Primitive(Boolean)`
/// - `For { .. }` → `ty` is `ResolvedType::Array(body_type)`
#[derive(Clone, Debug)]
pub enum IrExpr {
    /// Literal value (string, number, boolean, etc.)
    Literal {
        /// The literal value
        value: Literal,
        /// Resolved type of this literal
        ty: ResolvedType,
    },

    /// Struct instantiation: `User(name: "Alice", age: 30)`
    StructInst {
        /// The struct being instantiated.
        /// `None` for external structs - use `ty` field instead.
        struct_id: Option<StructId>,
        /// Generic type arguments (e.g., `[String]` for `Box<String>`)
        type_args: Vec<ResolvedType>,
        /// Regular field arguments
        fields: Vec<(String, IrExpr)>,
        /// Mount field arguments
        mounts: Vec<(String, IrExpr)>,
        /// Resolved type (the struct type or External)
        ty: ResolvedType,
    },

    /// Enum variant instantiation: `Status::Active` or `.Active`
    EnumInst {
        /// The enum being instantiated.
        /// `None` for external enums - use `ty` field instead.
        enum_id: Option<EnumId>,
        /// Variant name
        variant: String,
        /// Associated data fields
        fields: Vec<(String, IrExpr)>,
        /// Resolved type (the enum type or External)
        ty: ResolvedType,
    },

    /// Array literal: `[1, 2, 3]`
    Array {
        /// Array elements
        elements: Vec<IrExpr>,
        /// Resolved type: `Array(element_type)`
        ty: ResolvedType,
    },

    /// Tuple literal: `(x: 1, y: 2)`
    Tuple {
        /// Named fields
        fields: Vec<(String, IrExpr)>,
        /// Resolved type: `Tuple(fields)`
        ty: ResolvedType,
    },

    /// Variable or field reference: `user` or `user.name`
    Reference {
        /// The reference path (single name or dotted path)
        path: Vec<String>,
        /// Resolved type of the referenced value
        ty: ResolvedType,
    },

    /// Binary operation: `a + b`, `x == y`, `p && q`
    BinaryOp {
        /// Left operand
        left: Box<IrExpr>,
        /// Operator
        op: BinaryOperator,
        /// Right operand
        right: Box<IrExpr>,
        /// Resolved type (operand type for arithmetic, Boolean for comparison/logical)
        ty: ResolvedType,
    },

    /// Conditional expression: `if cond { a } else { b }`
    If {
        /// Condition (must be Boolean)
        condition: Box<IrExpr>,
        /// Then branch
        then_branch: Box<IrExpr>,
        /// Else branch (optional)
        else_branch: Option<Box<IrExpr>>,
        /// Resolved type (same as branches)
        ty: ResolvedType,
    },

    /// For loop: `for item in items { body }`
    For {
        /// Loop variable name
        var: String,
        /// Loop variable type
        var_ty: ResolvedType,
        /// Collection being iterated (must be Array)
        collection: Box<IrExpr>,
        /// Loop body
        body: Box<IrExpr>,
        /// Resolved type: `Array(body_type)`
        ty: ResolvedType,
    },

    /// Match expression: `match x { A => ..., B => ... }`
    Match {
        /// Value being matched (must be Enum)
        scrutinee: Box<IrExpr>,
        /// Match arms
        arms: Vec<IrMatchArm>,
        /// Resolved type (same as arm bodies)
        ty: ResolvedType,
    },
}

/// A match arm: `Variant(bindings) => body`
#[derive(Clone, Debug)]
pub struct IrMatchArm {
    /// Variant name being matched
    pub variant: String,

    /// Bindings for associated data: `(name, type)`
    pub bindings: Vec<(String, ResolvedType)>,

    /// Body expression
    pub body: IrExpr,
}

impl IrExpr {
    /// Get the resolved type of this expression.
    pub fn ty(&self) -> &ResolvedType {
        match self {
            IrExpr::Literal { ty, .. } => ty,
            IrExpr::StructInst { ty, .. } => ty,
            IrExpr::EnumInst { ty, .. } => ty,
            IrExpr::Array { ty, .. } => ty,
            IrExpr::Tuple { ty, .. } => ty,
            IrExpr::Reference { ty, .. } => ty,
            IrExpr::BinaryOp { ty, .. } => ty,
            IrExpr::If { ty, .. } => ty,
            IrExpr::For { ty, .. } => ty,
            IrExpr::Match { ty, .. } => ty,
        }
    }
}
