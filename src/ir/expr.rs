//! IR expression types.

use crate::ast::{BinaryOperator, Literal, ParamConvention, UnaryOperator};

use super::block::{IrBlockStatement, IrMatchArm};
use super::{EnumId, ImplId, ResolvedType, StructId, TraitId};

/// How a method call should be dispatched.
///
/// Backends must pick the correct emission strategy depending on whether
/// the receiver's concrete type is known at compile time. Static dispatch
/// resolves to a specific `impl` block; virtual dispatch must go through a
/// vtable keyed by the trait and method name.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DispatchKind {
    /// Direct call on a known concrete type — no runtime lookup needed.
    Static {
        /// The impl block that provides the method body.
        impl_id: ImplId,
    },
    /// Trait method call through a generic type parameter or trait object.
    /// The backend must resolve the concrete method at runtime (monomorphised
    /// or through a vtable, depending on the target).
    Virtual {
        /// The trait declaring the method.
        trait_id: TraitId,
        /// The method name on the trait.
        method_name: String,
    },
}

/// An expression in the IR.
///
/// Every expression variant includes a `ty` field containing its resolved type.
/// Code generators can use this to emit properly typed code without re-inferring.
///
/// # Type Contract
///
/// The `ty` field is guaranteed to be correct after lowering from the AST.
/// For example:
/// - `Literal { value: Literal::Number(n), ty }` → `ty` is `ResolvedType::Primitive(n.primitive_type())` (the suffix's primitive when present, or `I32` / `F64` defaulted from the source kind)
/// - `BinaryOp { op: Eq, .. }` → `ty` is `ResolvedType::Primitive(Boolean)`
/// - `For { .. }` → `ty` is `ResolvedType::Array(body_type)`
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
        fields: Vec<(String, Self)>,
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
        fields: Vec<(String, Self)>,
        /// Resolved type (the enum type or External)
        ty: ResolvedType,
    },

    /// Array literal: `[1, 2, 3]`
    Array {
        /// Array elements
        elements: Vec<Self>,
        /// Resolved type: `Array(element_type)`
        ty: ResolvedType,
    },

    /// Tuple literal: `(x: 1, y: 2)`
    Tuple {
        /// Named fields
        fields: Vec<(String, Self)>,
        /// Resolved type: `Tuple(fields)`
        ty: ResolvedType,
    },

    /// Variable or field reference: `user` or `user.name`
    ///
    /// Note: For `self.field` references within impl blocks, use [`IrExpr::SelfFieldRef`] instead.
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
        object: Box<Self>,
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
        left: Box<Self>,
        /// Operator
        op: BinaryOperator,
        /// Right operand
        right: Box<Self>,
        /// Resolved type (operand type for arithmetic, Boolean for comparison/logical)
        ty: ResolvedType,
    },

    /// Unary operation: `-x`, `!flag`
    UnaryOp {
        /// Operator
        op: UnaryOperator,
        /// Operand
        operand: Box<Self>,
        /// Resolved type (operand type for negation, Boolean for logical not)
        ty: ResolvedType,
    },

    /// Conditional expression: `if cond { a } else { b }`
    If {
        /// Condition (must be Boolean)
        condition: Box<Self>,
        /// Then branch
        then_branch: Box<Self>,
        /// Else branch (optional)
        else_branch: Option<Box<Self>>,
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
        collection: Box<Self>,
        /// Loop body
        body: Box<Self>,
        /// Resolved type: `Array(body_type)`
        ty: ResolvedType,
    },

    /// Match expression: `match x { A => ..., B => ... }`
    Match {
        /// Value being matched (must be Enum)
        scrutinee: Box<Self>,
        /// Match arms
        arms: Vec<IrMatchArm>,
        /// Resolved type (same as arm bodies)
        ty: ResolvedType,
    },

    /// Function call: `sin(angle: x)` or `builtin::math::sin(angle: x)`
    FunctionCall {
        /// Function path (e.g., `["builtin", "math", "sin"]`)
        path: Vec<String>,
        /// Arguments: (`optional_parameter_name`, value)
        /// Some(name) for named args, None for positional args
        args: Vec<(Option<String>, Self)>,
        /// Resolved return type
        ty: ResolvedType,
    },

    /// Method call: `self.fill.sample(coords)`
    MethodCall {
        /// Receiver expression
        receiver: Box<Self>,
        /// Method name
        method: String,
        /// Named arguments: (`parameter_name`, value) - None for positional args
        args: Vec<(Option<String>, Self)>,
        /// Dispatch strategy (static call into a specific impl block, or
        /// virtual call through a trait).
        dispatch: DispatchKind,
        /// Resolved return type
        ty: ResolvedType,
    },

    /// Closure expression: `|x: f32, y: f32| -> f32 { x + y }`
    Closure {
        /// Parameter conventions, names, and types
        params: Vec<(ParamConvention, String, ResolvedType)>,
        /// Free variables referenced by the body that are bound in an
        /// enclosing scope. Each entry is
        /// `(binding_name, capture_mode, resolved_type)` — the mode
        /// mirrors the `ParamConvention` of the outer binding (or `Let`
        /// for a plain immutable capture) so backends can choose between
        /// copy, move, reference, or sink semantics.
        ///
        /// Populated during IR lowering by walking the body and collecting
        /// every [`Reference`](Self::Reference) / [`LetRef`](Self::LetRef)
        /// whose name is not introduced inside the closure itself. Backends
        /// use this to emit capture-environment structs, vtable closures,
        /// or to reject closures that capture values whose lifetime cannot
        /// be satisfied by the target language.
        ///
        /// Capture entries are deduplicated by name and ordered by the
        /// first reference encountered during the traversal.
        captures: Vec<(String, ParamConvention, ResolvedType)>,
        /// Closure body
        body: Box<Self>,
        /// Resolved type: `Closure { param_tys, return_ty }`
        ty: ResolvedType,
    },

    /// Reference to a lifted closure: a top-level function paired with
    /// a runtime environment value carrying its captures.
    ///
    /// Produced by `ClosureConversionPass`. After that pass runs, every
    /// [`IrExpr::Closure`] in the module has been replaced by a
    /// `ClosureRef` whose `funcref` names the lifted top-level
    /// `IrFunction` (its first parameter is the env struct, followed by
    /// the original closure parameters) and whose `env_struct` is an
    /// expression that constructs the corresponding capture-environment
    /// `IrStruct`.
    ///
    /// Backends targeting closure-supporting languages can render a
    /// `ClosureRef` as a function-pointer / environment pair (the
    /// classic representation behind `funcref` + `call_indirect` in
    /// `WebAssembly`, for example).
    ClosureRef {
        /// Path to the lifted top-level function (e.g.,
        /// `["__closure_make_adder_0"]`). Lookup follows the same
        /// convention as [`Self::FunctionCall::path`].
        funcref: Vec<String>,
        /// Expression constructing the runtime capture environment —
        /// typically an [`Self::StructInst`] whose fields hold the
        /// captured values. May be a unit / empty struct when the
        /// original closure captured nothing.
        env_struct: Box<Self>,
        /// Resolved type: same closure type carried by the original
        /// [`Self::Closure`] node (`Closure { param_tys, return_ty }`).
        ty: ResolvedType,
    },

    /// Dictionary literal: `["key": value, "key2": value2]`
    DictLiteral {
        /// Key-value entries
        entries: Vec<(Self, Self)>,
        /// Resolved type: `Dictionary { key_ty, value_ty }`
        ty: ResolvedType,
    },

    /// Dictionary access: `dict["key"]` or `dict[index]`
    DictAccess {
        /// The dictionary being accessed
        dict: Box<Self>,
        /// The key expression
        key: Box<Self>,
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
        result: Box<Self>,
        /// Resolved type (same as result expression)
        ty: ResolvedType,
    },
}

impl IrExpr {
    /// Get the resolved type of this expression.
    #[must_use]
    pub const fn ty(&self) -> &ResolvedType {
        match self {
            Self::Literal { ty, .. }
            | Self::StructInst { ty, .. }
            | Self::EnumInst { ty, .. }
            | Self::Array { ty, .. }
            | Self::Tuple { ty, .. }
            | Self::Reference { ty, .. }
            | Self::SelfFieldRef { ty, .. }
            | Self::FieldAccess { ty, .. }
            | Self::LetRef { ty, .. }
            | Self::BinaryOp { ty, .. }
            | Self::UnaryOp { ty, .. }
            | Self::If { ty, .. }
            | Self::For { ty, .. }
            | Self::Match { ty, .. }
            | Self::FunctionCall { ty, .. }
            | Self::MethodCall { ty, .. }
            | Self::Closure { ty, .. }
            | Self::ClosureRef { ty, .. }
            | Self::DictLiteral { ty, .. }
            | Self::DictAccess { ty, .. }
            | Self::Block { ty, .. } => ty,
        }
    }

    /// Get a mutable reference to the resolved type of this expression.
    pub const fn ty_mut(&mut self) -> &mut ResolvedType {
        match self {
            Self::Literal { ty, .. }
            | Self::StructInst { ty, .. }
            | Self::EnumInst { ty, .. }
            | Self::Array { ty, .. }
            | Self::Tuple { ty, .. }
            | Self::Reference { ty, .. }
            | Self::SelfFieldRef { ty, .. }
            | Self::FieldAccess { ty, .. }
            | Self::LetRef { ty, .. }
            | Self::BinaryOp { ty, .. }
            | Self::UnaryOp { ty, .. }
            | Self::If { ty, .. }
            | Self::For { ty, .. }
            | Self::Match { ty, .. }
            | Self::FunctionCall { ty, .. }
            | Self::MethodCall { ty, .. }
            | Self::Closure { ty, .. }
            | Self::ClosureRef { ty, .. }
            | Self::DictLiteral { ty, .. }
            | Self::DictAccess { ty, .. }
            | Self::Block { ty, .. } => ty,
        }
    }

    /// Whether this expression is a constant aggregate — a literal, or an
    /// aggregate (array / tuple / struct / enum / dict) whose every leaf is a
    /// literal. After [`fold_constants`](crate::ir::fold_constants) this
    /// predicate is the load-bearing marker for "static initializer": backends
    /// emitting read-only data segments can short-circuit on it instead of
    /// re-walking children themselves.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Literal { .. } => true,
            Self::Array { elements, .. } => elements.iter().all(Self::is_constant),
            Self::Tuple { fields, .. }
            | Self::StructInst { fields, .. }
            | Self::EnumInst { fields, .. } => fields.iter().all(|(_, e)| e.is_constant()),
            Self::DictLiteral { entries, .. } => entries
                .iter()
                .all(|(k, v)| k.is_constant() && v.is_constant()),
            Self::Reference { .. }
            | Self::SelfFieldRef { .. }
            | Self::FieldAccess { .. }
            | Self::LetRef { .. }
            | Self::BinaryOp { .. }
            | Self::UnaryOp { .. }
            | Self::If { .. }
            | Self::For { .. }
            | Self::Match { .. }
            | Self::FunctionCall { .. }
            | Self::MethodCall { .. }
            | Self::Closure { .. }
            | Self::ClosureRef { .. }
            | Self::DictAccess { .. }
            | Self::Block { .. } => false,
        }
    }
}
