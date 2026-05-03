//! IR expression types.

mod accessors;
mod reference;

pub use reference::{DispatchKind, ReferenceTarget};

use crate::ast::{BinaryOperator, Literal, ParamConvention, UnaryOperator};

use super::block::{IrBlockStatement, IrMatchArm};
use super::{
    BindingId, EnumId, FieldIdx, FunctionId, MethodIdx, ResolvedType, StructId, VariantIdx,
};

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
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Struct instantiation: `User(name: "Alice", age: 30)`
    StructInst {
        /// The struct being instantiated.
        /// `None` for external structs - use `ty` field instead.
        struct_id: Option<StructId>,
        /// Generic type arguments (e.g., `[String]` for `Box<String>`)
        type_args: Vec<ResolvedType>,
        /// Regular field arguments: `(name, field_idx, value)`. The
        /// `field_idx` is the field's position in [`super::IrStruct`]'s
        /// `fields` vector. Lowering emits `FieldIdx(0)` and
        /// `ResolveReferencesPass` overwrites it.
        fields: Vec<(String, FieldIdx, Self)>,
        /// Resolved type (the struct type or External)
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Enum variant instantiation: `Status::Active` or `.Active`
    EnumInst {
        /// The enum being instantiated.
        /// `None` for external enums - use `ty` field instead.
        enum_id: Option<EnumId>,
        /// Variant name (preserved for diagnostics).
        variant: String,
        /// Variant index within the enum's `variants` vector.
        /// Lowering emits `VariantIdx(0)` and `ResolveReferencesPass`
        /// overwrites it.
        variant_idx: VariantIdx,
        /// Associated data fields: `(name, field_idx, value)`. The
        /// `field_idx` is the field's position in the
        /// matched [`super::IrEnumVariant`]'s `fields` vector.
        fields: Vec<(String, FieldIdx, Self)>,
        /// Resolved type (the enum type or External)
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Array literal: `[1, 2, 3]`
    Array {
        /// Array elements
        elements: Vec<Self>,
        /// Resolved type: `Array(element_type)`
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Tuple literal: `(x: 1, y: 2)`
    Tuple {
        /// Named fields
        fields: Vec<(String, Self)>,
        /// Resolved type: `Tuple(fields)`
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Variable or field reference: `user` or `user.name`
    ///
    /// Note: For `self.field` references within impl blocks, use [`IrExpr::SelfFieldRef`] instead.
    Reference {
        /// The reference path (single name or dotted path) — preserved
        /// alongside [`Self::Reference::target`] for diagnostics.
        path: Vec<String>,
        /// Resolved target of this reference. Set to
        /// [`ReferenceTarget::Unresolved`] by lowering and rewritten by
        /// `ResolveReferencesPass` to the matching variant.
        target: ReferenceTarget,
        /// Resolved type of the referenced value
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
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
        /// The field name being accessed (without the `self.` prefix);
        /// preserved alongside [`Self::SelfFieldRef::field_idx`] for
        /// diagnostics.
        field: String,
        /// Position of `field` in the impl's struct's `fields` vector.
        /// Lowering emits `FieldIdx(0)` and `ResolveReferencesPass`
        /// overwrites it.
        field_idx: FieldIdx,
        /// Resolved type of the field
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Field access on arbitrary expressions: `(-chord).y`, `(a + b).len`
    ///
    /// Unlike `Reference` which handles compile-time known paths like `user.name`,
    /// this handles field access on computed expressions where the base is not
    /// a simple identifier path.
    FieldAccess {
        /// The base expression to access a field on
        object: Box<Self>,
        /// The field name to access (preserved for diagnostics).
        field: String,
        /// Position of `field` in `object`'s struct's `fields` vector.
        /// Lowering emits `FieldIdx(0)` and `ResolveReferencesPass`
        /// overwrites it.
        field_idx: FieldIdx,
        /// Resolved type of the field
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Reference to a function-local `let` binding by name.
    ///
    /// This is the function-scope counterpart to [`Self::Reference`] —
    /// when lowering encounters a single-segment path that resolves to a
    /// `let` introduced inside the current function, it emits `LetRef`
    /// instead of `Reference`. (Module-scope `let`s use
    /// [`Self::Reference`] with [`ReferenceTarget::ModuleLet`] after
    /// `ResolveReferencesPass`.)
    LetRef {
        /// The name of the let binding (preserved for diagnostics).
        name: String,
        /// Per-function-unique binding identifier — paired with the
        /// `BindingId` on the introducing
        /// [`super::IrBlockStatement::Let`]. Lowering emits
        /// `BindingId(0)` and `ResolveReferencesPass` overwrites it.
        binding_id: BindingId,
        /// Resolved type of the binding
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
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
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Unary operation: `-x`, `!flag`
    UnaryOp {
        /// Operator
        op: UnaryOperator,
        /// Operand
        operand: Box<Self>,
        /// Resolved type (operand type for negation, Boolean for logical not)
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
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
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// For loop: `for item in items { body }`
    For {
        /// Loop variable name
        var: String,
        /// Loop variable type
        var_ty: ResolvedType,
        /// Per-function-unique binding identifier for the loop
        /// variable — paired with the [`Self::LetRef::binding_id`] on
        /// references to `var` inside `body`. Lowering emits
        /// `BindingId(0)` and `ResolveReferencesPass` overwrites it.
        var_binding_id: BindingId,
        /// Collection being iterated (must be Array)
        collection: Box<Self>,
        /// Loop body
        body: Box<Self>,
        /// Resolved type: `Array(body_type)`
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Match expression: `match x { A => ..., B => ... }`
    Match {
        /// Value being matched (must be Enum)
        scrutinee: Box<Self>,
        /// Match arms
        arms: Vec<IrMatchArm>,
        /// Resolved type (same as arm bodies)
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Function call: `sin(angle: x)` or `builtin::math::sin(angle: x)`
    FunctionCall {
        /// Function path (e.g., `["builtin", "math", "sin"]`) —
        /// preserved alongside [`Self::FunctionCall::function_id`]
        /// for diagnostics and as a fallback when resolution fails
        /// (e.g. cross-module call where the target lives in another
        /// `IrModule` not yet linked).
        path: Vec<String>,
        /// Resolved target function id. `None` when the path is
        /// genuinely external (cross-module) or when lowering /
        /// `ResolveReferencesPass` couldn't bind it. Backends key on
        /// this id to dispatch directly without re-walking
        /// `IrModule.functions` and without inferring scope from the
        /// caller's module prefix.
        function_id: Option<FunctionId>,
        /// Arguments: (`optional_parameter_name`, value)
        /// Some(name) for named args, None for positional args
        args: Vec<(Option<String>, Self)>,
        /// Resolved return type
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Indirect call of a closure-typed value: `f(x)` where `f` is a
    /// closure-typed local binding (a parameter, a `let`, a struct
    /// field, ...). The `closure` sub-expression evaluates to a
    /// closure value (post-conversion: an [`Self::ClosureRef`] or any
    /// expression that produces one); its lifted top-level function is
    /// invoked indirectly with the captured environment + the explicit
    /// arguments.
    ///
    /// Lowering only emits this variant after a path resolves to a
    /// closure-typed binding rather than a top-level function (see the
    /// AST→IR lowering of [`crate::ast::Expr::Invocation`]). For
    /// statically-known top-level calls, [`Self::FunctionCall`] is
    /// still produced.
    ///
    /// The closure-conversion pass walks into `closure` like any other
    /// sub-expression — it does not need to rewrite the call site,
    /// since the call shape is already "closure value + args".
    /// Backends that target indirect-call ABIs (e.g. `WebAssembly`'s
    /// `call_indirect`) read the funcref and env pointer out of the
    /// closure value at the call site.
    CallClosure {
        /// Expression producing the closure value being invoked.
        /// Typically an [`Self::LetRef`] / [`Self::Reference`] /
        /// [`Self::FieldAccess`] whose type is
        /// [`ResolvedType::Closure`].
        closure: Box<Self>,
        /// Arguments: `(optional_parameter_name, value)`. Mirrors
        /// [`Self::FunctionCall::args`]; closures don't currently
        /// carry parameter names so the optional name is always
        /// `None` after lowering, but the structure is kept for
        /// consistency.
        args: Vec<(Option<String>, Self)>,
        /// Resolved return type — the `return_ty` from the closure's
        /// [`ResolvedType::Closure`] type.
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Method call: `self.fill.sample(coords)`
    MethodCall {
        /// Receiver expression
        receiver: Box<Self>,
        /// Method name (preserved for diagnostics).
        method: String,
        /// Position of the method within its containing definition:
        /// for [`DispatchKind::Static`], the index into the impl's
        /// `functions`; for [`DispatchKind::Virtual`], the index into
        /// the trait's `methods`. Lowering emits `MethodIdx(0)` and
        /// `ResolveReferencesPass` overwrites it.
        method_idx: MethodIdx,
        /// Named arguments: (`parameter_name`, value) - None for positional args
        args: Vec<(Option<String>, Self)>,
        /// Dispatch strategy (static call into a specific impl block, or
        /// virtual call through a trait).
        dispatch: DispatchKind,
        /// Resolved return type
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Closure expression: `|x: f32, y: f32| -> f32 { x + y }`
    Closure {
        /// Parameter conventions, binding ids, names, and types.
        /// `binding_id` is fresh per-function; lowering emits
        /// `BindingId(0)` and `ResolveReferencesPass` overwrites it.
        params: Vec<(ParamConvention, BindingId, String, ResolvedType)>,
        /// Free variables referenced by the body that are bound in an
        /// enclosing scope. Each entry is
        /// `(outer_binding_id, name, capture_mode, resolved_type)` — the
        /// mode mirrors the `ParamConvention` of the outer binding (or
        /// `Let` for a plain immutable capture) so backends can choose
        /// between copy, move, reference, or sink semantics. The
        /// `outer_binding_id` is the [`BindingId`] of the introducing
        /// binding *in the enclosing function*; lowering emits
        /// `BindingId(0)` and `ResolveReferencesPass` overwrites it.
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
        captures: Vec<(BindingId, String, ParamConvention, ResolvedType)>,
        /// Closure body
        body: Box<Self>,
        /// Resolved type: `Closure { param_tys, return_ty }`
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
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
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Dictionary literal: `["key": value, "key2": value2]`
    DictLiteral {
        /// Key-value entries
        entries: Vec<(Self, Self)>,
        /// Resolved type: `Dictionary { key_ty, value_ty }`
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },

    /// Dictionary access: `dict["key"]` or `dict[index]`
    DictAccess {
        /// The dictionary being accessed
        dict: Box<Self>,
        /// The key expression
        key: Box<Self>,
        /// Resolved type: the value type of the dictionary
        ty: ResolvedType,
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
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
        /// Source span for DWARF / source-map emission.
        #[serde(default, skip_serializing_if = "super::IrSpan::is_default")]
        span: super::IrSpan,
    },
}
