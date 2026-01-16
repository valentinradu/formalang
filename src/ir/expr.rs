//! IR expression types.

use crate::ast::{BinaryOperator, Literal, UnaryOperator};

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
    ///
    /// Note: For `self.field` references within impl blocks, use [`SelfFieldRef`] instead.
    Reference {
        /// The reference path (single name or dotted path)
        path: Vec<String>,
        /// Resolved type of the referenced value
        ty: ResolvedType,
    },

    /// Reference to a field on `self` within an impl block: `self.color`
    ///
    /// This is a specialized form of reference for accessing fields of the
    /// struct being implemented. Code generators should use this to emit
    /// appropriate self-referencing code in the target language.
    ///
    /// # Example
    ///
    /// ```formalang
    /// impl Button {
    ///     background: fill::Solid(color: self.color)
    /// }
    /// ```
    SelfFieldRef {
        /// The field name being accessed (without the `self.` prefix)
        field: String,
        /// Resolved type of the field
        ty: ResolvedType,
    },

    /// Field access on arbitrary expressions: `(-chord).y`, `(a + b).len`
    ///
    /// Unlike `Reference` which handles compile-time known paths like `user.name`,
    /// this handles field access on computed expressions where the base is not
    /// a simple identifier path.
    FieldAccess {
        /// The base expression to access a field on
        object: Box<IrExpr>,
        /// The field name to access
        field: String,
        /// Resolved type of the field
        ty: ResolvedType,
    },

    /// Reference to a module-level let binding: `primaryColor`, `headingFont`
    ///
    /// This is a specialized form of reference for accessing module-level
    /// constants and computed values. Code generators should use this to
    /// emit appropriate constant references in the target language.
    LetRef {
        /// The name of the let binding
        name: String,
        /// Resolved type of the binding
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

    /// Unary operation: `-x`, `!flag`
    UnaryOp {
        /// Operator
        op: UnaryOperator,
        /// Operand
        operand: Box<IrExpr>,
        /// Resolved type (operand type for negation, Boolean for logical not)
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

    /// Function call: `sin(angle: x)` or `builtin::math::sin(angle: x)`
    FunctionCall {
        /// Function path (e.g., ["builtin", "math", "sin"])
        path: Vec<String>,
        /// Arguments: (optional_parameter_name, value)
        /// Some(name) for named args, None for positional args
        args: Vec<(Option<String>, IrExpr)>,
        /// Resolved return type
        ty: ResolvedType,
    },

    /// Method call: `self.fill.sample(coords)`
    MethodCall {
        /// Receiver expression
        receiver: Box<IrExpr>,
        /// Method name
        method: String,
        /// Named arguments: (parameter_name, value) - None for positional args
        args: Vec<(Option<String>, IrExpr)>,
        /// Resolved return type
        ty: ResolvedType,
    },

    /// Event mapping: `() -> .submit` or `x -> .changed(value: x)`
    ///
    /// Event mappings are restricted closures that:
    /// - Have zero or one parameter
    /// - Return an enum variant instantiation
    /// - Cannot capture variables from outer scope
    ///
    /// These compile to metadata for the runtime, not executable GPU code.
    EventMapping {
        /// The enum being instantiated.
        /// `None` for external enums - use return type from `ty` field.
        enum_id: Option<EnumId>,
        /// Variant name being constructed
        variant: String,
        /// Parameter name (None for `() -> ...`)
        param: Option<String>,
        /// Maps enum variant fields to the parameter
        /// e.g., `x -> .changed(value: x)` produces `[("value", "x")]`
        field_bindings: Vec<EventFieldBinding>,
        /// Resolved type: the closure/event mapping type
        ty: ResolvedType,
    },

    /// Dictionary literal: `["key": value, "key2": value2]`
    DictLiteral {
        /// Key-value entries
        entries: Vec<(IrExpr, IrExpr)>,
        /// Resolved type: `Dictionary { key_ty, value_ty }`
        ty: ResolvedType,
    },

    /// Dictionary access: `dict["key"]` or `dict[index]`
    DictAccess {
        /// The dictionary being accessed
        dict: Box<IrExpr>,
        /// The key expression
        key: Box<IrExpr>,
        /// Resolved type: the value type of the dictionary
        ty: ResolvedType,
    },

    /// Block expression: `{ let x = 1; let y = 2; x + y }`
    ///
    /// A sequence of statements followed by a result expression.
    /// The result expression's value becomes the block's value.
    Block {
        /// Statements in the block (let bindings, assignments, expressions)
        statements: Vec<IrBlockStatement>,
        /// The final expression whose value is the block's value
        result: Box<IrExpr>,
        /// Resolved type (same as result expression)
        ty: ResolvedType,
    },
}

/// A statement within a block expression.
#[derive(Clone, Debug)]
pub enum IrBlockStatement {
    /// Let binding: `let x = expr` or `let mut x = expr`
    Let {
        /// Binding name
        name: String,
        /// Whether the binding is mutable
        mutable: bool,
        /// Optional type annotation
        ty: Option<ResolvedType>,
        /// Value expression
        value: IrExpr,
    },
    /// Assignment: `x = expr`
    Assign {
        /// Target expression (variable or field path)
        target: IrExpr,
        /// Value expression
        value: IrExpr,
    },
    /// Expression statement (evaluated for side effects)
    Expr(IrExpr),
}

impl IrBlockStatement {
    /// Transform all expressions in this statement using the provided function.
    ///
    /// This is useful for implementing transformations like constant folding
    /// or dead code elimination that need to recursively process expressions.
    pub fn map_exprs<F>(self, mut f: F) -> Self
    where
        F: FnMut(IrExpr) -> IrExpr,
    {
        match self {
            IrBlockStatement::Let {
                name,
                mutable,
                ty,
                value,
            } => IrBlockStatement::Let {
                name,
                mutable,
                ty,
                value: f(value),
            },
            IrBlockStatement::Assign { target, value } => IrBlockStatement::Assign {
                target: f(target),
                value: f(value),
            },
            IrBlockStatement::Expr(expr) => IrBlockStatement::Expr(f(expr)),
        }
    }
}

/// A field binding in an event mapping.
///
/// Maps an enum variant field to a source.
#[derive(Clone, Debug)]
pub struct EventFieldBinding {
    /// The field name in the enum variant
    pub field_name: String,
    /// The source of the value
    pub source: EventBindingSource,
}

/// Source of a value in an event mapping field binding.
#[derive(Clone, Debug)]
pub enum EventBindingSource {
    /// References the event mapping parameter: `x -> .changed(value: x)`
    Param(String),
    /// A literal value: `() -> .changed(value: 42)`
    Literal(Literal),
}

/// A match arm: `Variant(bindings) => body` or `_ => body`
#[derive(Clone, Debug)]
pub struct IrMatchArm {
    /// Variant name being matched (empty string for wildcard)
    pub variant: String,

    /// Whether this is a wildcard pattern (`_`)
    pub is_wildcard: bool,

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
            IrExpr::SelfFieldRef { ty, .. } => ty,
            IrExpr::FieldAccess { ty, .. } => ty,
            IrExpr::LetRef { ty, .. } => ty,
            IrExpr::BinaryOp { ty, .. } => ty,
            IrExpr::UnaryOp { ty, .. } => ty,
            IrExpr::If { ty, .. } => ty,
            IrExpr::For { ty, .. } => ty,
            IrExpr::Match { ty, .. } => ty,
            IrExpr::FunctionCall { ty, .. } => ty,
            IrExpr::MethodCall { ty, .. } => ty,
            IrExpr::EventMapping { ty, .. } => ty,
            IrExpr::DictLiteral { ty, .. } => ty,
            IrExpr::DictAccess { ty, .. } => ty,
            IrExpr::Block { ty, .. } => ty,
        }
    }
}
