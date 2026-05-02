# Expressions

Every expression carries its resolved type in the `ty` field. This
eliminates the need for code generators to re-infer types.

Several expression variants also carry **typed-id payloads**
(`ReferenceTarget`, `BindingId`, `FieldIdx`, `VariantIdx`, `MethodIdx`,
`FunctionId`, `DispatchKind`). Lowering emits placeholder `0`-valued ids
for these slots and the optional `ResolveReferencesPass` rewrites them.
Backends that consume integer-indexed code (wasm, JVM, native) should
run that pass; backends that re-walk the module by name can skip it.

## ReferenceTarget

Identifies what an `IrExpr::Reference` resolves to. Pre-resolve, every
reference carries `Unresolved`; `ResolveReferencesPass` rewrites it to
the matching variant. Backends dispatch on the variant directly without
re-walking module symbol tables. The original `path` is preserved
alongside for diagnostics.

```rust
pub enum ReferenceTarget {
    /// A standalone function (resolved against `IrModule::functions`).
    Function(FunctionId),
    /// A struct definition used as a value or type.
    Struct(StructId),
    /// An enum definition.
    Enum(EnumId),
    /// A trait definition.
    Trait(TraitId),
    /// A module-scope `let` binding.
    ModuleLet(LetId),
    /// A function-local `let` binding (introduced by `IrBlockStatement::Let`).
    Local(BindingId),
    /// A function parameter (introduced by `IrFunctionParam`).
    Param(BindingId),
    /// A reference into another module that has not yet been linked
    /// (cross-module linking is per-backend).
    External {
        module_path: Vec<String>,
        name: String,
        kind: ImportedKind,
    },
    /// Pre-`ResolveReferencesPass` placeholder; backends should never see it.
    Unresolved,
}
```

## DispatchKind

How a method call should be dispatched.

```rust
pub enum DispatchKind {
    /// Direct call on a known concrete type — no runtime lookup needed.
    Static {
        impl_id: ImplId,
    },
    /// Trait method call through a generic type parameter or trait object.
    /// Monomorphisation devirtualises these on concrete receivers; surviving
    /// `Virtual` calls require a vtable in the target.
    Virtual {
        trait_id: TraitId,
        method_name: String,
    },
}
```

## IrExpr

```rust
pub enum IrExpr {
    /// Literal value: string, number, boolean, regex, path, nil
    Literal {
        value: Literal,
        ty: ResolvedType,
    },

    /// Struct instantiation: `User(name: "Alice", age: 30)`
    StructInst {
        /// `None` for external structs — read `ty` instead.
        struct_id: Option<StructId>,
        /// Generic type args (e.g., `[String]` for `Box<String>`).
        type_args: Vec<ResolvedType>,
        /// Fields: `(name, field_idx, value)`. `field_idx` is the position
        /// in the target `IrStruct.fields`; lowering emits `FieldIdx(0)`
        /// and `ResolveReferencesPass` overwrites it.
        fields: Vec<(String, FieldIdx, IrExpr)>,
        ty: ResolvedType,
    },

    /// Enum variant instantiation: `Status::Active` or `.Active`
    EnumInst {
        enum_id: Option<EnumId>,
        variant: String,
        /// Variant index in the target `IrEnum.variants`.
        variant_idx: VariantIdx,
        /// Associated data: `(name, field_idx, value)`.
        fields: Vec<(String, FieldIdx, IrExpr)>,
        ty: ResolvedType,
    },

    /// Array literal: `[1, 2, 3]`
    Array {
        elements: Vec<IrExpr>,
        ty: ResolvedType,
    },

    /// Tuple literal: `(x: 1, y: 2)`
    Tuple {
        fields: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },

    /// Variable or field reference.
    Reference {
        /// Original source path (preserved for diagnostics).
        path: Vec<String>,
        /// Resolved target. `Unresolved` pre-`ResolveReferencesPass`.
        target: ReferenceTarget,
        ty: ResolvedType,
    },

    /// `self.field` reference within an impl block.
    SelfFieldRef {
        field: String,
        /// Position in the impl's struct's `fields`.
        field_idx: FieldIdx,
        ty: ResolvedType,
    },

    /// Field access on an arbitrary expression: `(a + b).len`.
    FieldAccess {
        object: Box<IrExpr>,
        field: String,
        field_idx: FieldIdx,
        ty: ResolvedType,
    },

    /// Reference to a function-local `let` binding by name.
    /// Module-scope `let`s use `Reference` with `ReferenceTarget::ModuleLet`.
    LetRef {
        name: String,
        /// Per-function-unique id, paired with the introducing
        /// `IrBlockStatement::Let::binding_id`.
        binding_id: BindingId,
        ty: ResolvedType,
    },

    /// Binary operation: `a + b`, `x == y`, `p && q`.
    BinaryOp {
        left: Box<IrExpr>,
        op: BinaryOperator,
        right: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Unary operation: `-x`, `!flag`.
    UnaryOp {
        op: UnaryOperator,
        operand: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Conditional expression: `if cond { a } else { b }`.
    If {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Option<Box<IrExpr>>,
        ty: ResolvedType,
    },

    /// For loop: `for item in items { body }`.
    For {
        var: String,
        var_ty: ResolvedType,
        /// Per-function-unique id for the loop variable, paired with
        /// `LetRef::binding_id` on references to `var` inside `body`.
        var_binding_id: BindingId,
        collection: Box<IrExpr>,
        body: Box<IrExpr>,
        /// `Array(body_type)`.
        ty: ResolvedType,
    },

    /// Match expression: `match x { A => ..., B => ... }`.
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrMatchArm>,
        ty: ResolvedType,
    },

    /// Direct call to a top-level function: `sin(angle: x)` or
    /// `builtin::math::sin(angle: x)`. For closure-typed locals, see
    /// `CallClosure`.
    FunctionCall {
        /// Function path (preserved for diagnostics and as a fallback
        /// when resolution fails — e.g. cross-module calls).
        path: Vec<String>,
        /// Resolved target. `None` for genuinely external paths or when
        /// resolution couldn't bind. Backends key on this id to dispatch
        /// directly without re-walking `IrModule.functions`.
        function_id: Option<FunctionId>,
        /// `(optional_parameter_name, value)`.
        args: Vec<(Option<String>, IrExpr)>,
        ty: ResolvedType,
    },

    /// Indirect call of a closure-typed value: `f(x)` where `f` is a
    /// closure-typed local (parameter, `let`, struct field, ...).
    /// Lowering emits this when a path resolves to a closure-typed
    /// binding rather than a top-level function.
    CallClosure {
        /// Expression producing the closure value (typically a `LetRef`,
        /// `Reference`, `FieldAccess`, or post-conversion `ClosureRef`).
        closure: Box<IrExpr>,
        /// Closures don't currently carry parameter names, so the optional
        /// name is always `None`; the structure mirrors `FunctionCall::args`.
        args: Vec<(Option<String>, IrExpr)>,
        /// `return_ty` from the closure type.
        ty: ResolvedType,
    },

    /// Method call: `self.fill.sample(coords)`.
    MethodCall {
        receiver: Box<IrExpr>,
        method: String,
        /// Method position: index into the impl's `functions` for `Static`,
        /// or into the trait's `methods` for `Virtual`.
        method_idx: MethodIdx,
        args: Vec<(Option<String>, IrExpr)>,
        dispatch: DispatchKind,
        ty: ResolvedType,
    },

    /// Closure expression: `|x: f32, y: f32| x + y`.
    ///
    /// Convention on each parameter constrains the **caller** of the
    /// closure (`Mut` requires a mutable argument; `Sink` moves it).
    ///
    /// `captures` lists every free variable referenced by the body that's
    /// bound in an enclosing scope. Each capture entry is
    /// `(outer_binding_id, name, capture_mode, resolved_type)`. The mode
    /// mirrors the outer binding's `ParamConvention` (or `Let` for plain
    /// immutable captures) so backends can choose copy/move/reference/sink
    /// semantics. Capture entries are deduplicated by name and ordered by
    /// the first reference encountered during the body walk. Both `params`
    /// and `captures` carry `BindingId`s assigned by `ResolveReferencesPass`.
    Closure {
        params: Vec<(ParamConvention, BindingId, String, ResolvedType)>,
        captures: Vec<(BindingId, String, ParamConvention, ResolvedType)>,
        body: Box<IrExpr>,
        /// `ResolvedType::Closure { param_tys, return_ty }`.
        ty: ResolvedType,
    },

    /// Reference to a lifted closure: a top-level function paired with a
    /// runtime environment value carrying its captures.
    ///
    /// Produced by `ClosureConversionPass`. After that pass runs, every
    /// `IrExpr::Closure` has been replaced by a `ClosureRef` whose
    /// `funcref` names the lifted top-level function (its first parameter
    /// is the env struct, followed by the original closure parameters)
    /// and whose `env_struct` is an expression constructing the
    /// corresponding capture-environment `IrStruct`. Backends can render
    /// this as a function-pointer / environment pair (e.g. `funcref` +
    /// `call_indirect` in WebAssembly).
    ClosureRef {
        funcref: Vec<String>,
        env_struct: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Dictionary literal: `["key": value, ...]`.
    DictLiteral {
        entries: Vec<(IrExpr, IrExpr)>,
        ty: ResolvedType,
    },

    /// Dictionary access: `dict["key"]` or `dict[index]`.
    DictAccess {
        dict: Box<IrExpr>,
        key: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Block expression: `{ let x = 1; let y = 2; x + y }`.
    Block {
        statements: Vec<IrBlockStatement>,
        result: Box<IrExpr>,
        ty: ResolvedType,
    },
}
```

`IrMatchArm` and `IrBlockStatement` — referenced from `Match` and `Block`
above — are defined on the [Match Arms & Block Statements](blocks.md) page.

## Type Contract

The `ty` field is guaranteed correct after lowering:

| Expression | Type |
| ---------- | ---- |
| `Literal { value: Number(_), .. }` | `Primitive(I32 / I64 / F32 / F64)` — picked from the literal's suffix or source-syntax default (integer → `I32`, float → `F64`) |
| `Literal { value: String(_), .. }` | `Primitive(String)` |
| `Literal { value: Boolean(_), .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: Add/Sub/Mul/Div/Mod, .. }` | Same as operands |
| `BinaryOp { op: Eq/Ne/Lt/Gt/Le/Ge, .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: And/Or, .. }` | `Primitive(Boolean)` |
| `For { body, .. }` | `Array(body.ty())` |
| `If { then_branch, .. }` | Same as branches |
| `Match { arms, .. }` | Same as arm bodies |

## Getting Expression Type

```rust
impl IrExpr {
    /// Get the resolved type of this expression
    pub fn ty(&self) -> &ResolvedType;
}

// Example
let expr: &IrExpr = /* ... */;
let ty = expr.ty();
match ty {
    ResolvedType::Primitive(PrimitiveType::String) => {
        // Generate string handling code
    }
    ResolvedType::Array(inner) => {
        // Generate array handling code
    }
    // ...
}
```
