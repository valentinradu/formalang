# Expressions

Every expression carries its resolved type in the `ty` field. This
eliminates the need for code generators to re-infer types.

Every variant also carries a `span: IrSpan` field. The listing below
leaves it out. See [Source Spans](overview.md#source-spans-dwarf--source-map--line-table).

Several expression variants also carry **typed-id payloads**
(`ReferenceTarget`, `BindingId`, `FieldIdx`, `VariantIdx`, `MethodIdx`,
`FunctionId`, `DispatchKind`). Lowering emits placeholder `0`-valued ids
for `BindingId`, `FieldIdx`, `VariantIdx` and `MethodIdx`, and
`Unresolved` for `ReferenceTarget`. `ResolveReferencesPass` rewrites
them. The lowering sets `FunctionId` and `DispatchKind` itself.
Backends that consume integer-indexed code (wasm, JVM, native) should
run that pass; `Pipeline::for_codegen` holds it. Backends that re-walk
the module by name can skip it.

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
    /// A reference into another module that is not linked. The public
    /// entry points link each imported module, so their result has
    /// none of these.
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
    /// Direct call on a known concrete type: no runtime lookup needed.
    Static {
        impl_id: ImplId,
    },
    /// Trait method call through a generic type parameter. There is no
    /// trait object: a trait cannot be the type of a value, so semantic
    /// analysis rejects one before lowering. `MonomorphisePass`
    /// rewrites every one of these to `Static` once the receiver is
    /// concrete, and reports any that survive.
    Virtual {
        trait_id: TraitId,
        method_name: String,
        /// The type arguments of the trait in the bound: `[I32]` for
        /// `<T: Container<I32>>`. Empty for a trait with no type
        /// parameters, and then left out of the JSON.
        trait_args: Vec<ResolvedType>,
    },
}
```

One type can implement two instances of one generic trait, for
example `Container<I32>` and `Container<String>`. `trait_args` says
which instance a call means. `MonomorphisePass` points the dispatch at
the specialised trait of that instance, and then at its impl.

## IrExpr

```rust
pub enum IrExpr {
    /// Literal value: string, number, boolean, nil
    Literal {
        value: Literal,
        ty: ResolvedType,
    },

    /// Struct instantiation: `User(name: "Alice", age: 30)`. An optional
    /// field with no default that the call leaves out is in `fields` as
    /// a `nil` literal, as if the call wrote `field: nil`.
    StructInst {
        /// `None` for external structs: read `ty` instead.
        struct_id: Option<StructId>,
        /// Generic type args (e.g., `[String]` for `Box<String>`).
        type_args: Vec<ResolvedType>,
        /// Fields: `(name, field_idx, value)`. `field_idx` is the position
        /// in the target `IrStruct.fields`; lowering emits `FieldIdx(0)`
        /// and `ResolveReferencesPass` overwrites it.
        fields: Vec<(String, FieldIdx, IrExpr)>,
        ty: ResolvedType,
    },

    /// Enum variant instantiation: `Status.active` or `.active`
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

    /// Reference to a parameter, a loop variable, a module-level `let`,
    /// or a function-local binding. A path such as `user.name` keeps
    /// all its segments.
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

    /// Reference to a function-local `let` binding by name. The
    /// lowering also uses it for the closure of a `CallClosure` on a
    /// named binding. Module-scope `let`s use `Reference` with
    /// `ReferenceTarget::ModuleLet`.
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
        /// `Seq<body_type>`: a lazy sequence. `.collect()` makes an array.
        ty: ResolvedType,
    },

    /// Match expression: `match x { .a: ..., .b(v): ... }`.
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrMatchArm>,
        ty: ResolvedType,
    },

    /// Direct call to a top-level function: `sin(angle: x)` or
    /// `math::sin(angle: x)`. For closure values, see `CallClosure`.
    FunctionCall {
        /// Function path. For an imported function, the path holds
        /// the module path, for example `["geom", "double_x"]`.
        path: Vec<String>,
        /// Resolved target. The lowering sets it, and each pass keeps
        /// it in step with `path`. `None` when no function in the
        /// module has the path. Backends key on this id to dispatch
        /// directly without re-walking `IrModule.functions`.
        function_id: Option<FunctionId>,
        /// `(optional_parameter_name, value)`.
        args: Vec<(Option<String>, IrExpr)>,
        ty: ResolvedType,
    },

    /// Indirect call of a closure value. The lowering gives it for:
    /// - `f(x)`, where `f` is a closure-typed parameter, local binding
    ///   or module-level `let` (the closure is a `LetRef`);
    /// - the AST node `Expr::Call`: a call of the value of any other
    ///   expression, such as `make()(4)` (the closure is that
    ///   expression, here a `FunctionCall`).
    CallClosure {
        /// Expression producing the closure value.
        closure: Box<IrExpr>,
        /// `(optional_label, value)`, as in `FunctionCall::args`.
        args: Vec<(Option<String>, IrExpr)>,
        /// `return_ty` from the closure type.
        ty: ResolvedType,
    },

    /// Method call: `self.fill.sample(coords)`.
    /// A static method call, `Counter.zero()`, has a `Reference` to
    /// the struct as its receiver: its `target` is
    /// `ReferenceTarget::Struct` and its `ty` is the struct. A static
    /// method has no `self`, so a backend does not evaluate that
    /// receiver.
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

    /// Closure expression: `(x: F32, y: F32) -> x + y`.
    ///
    /// Convention on each parameter constrains the **caller** of the
    /// closure (`Mut` requires a mutable argument; `Sink` moves it).
    ///
    /// `captures` lists every free variable referenced by the body that's
    /// bound in an enclosing scope. Each capture entry is
    /// `(outer_binding_id, name, capture_mode, resolved_type)`. The mode
    /// mirrors the outer binding's `ParamConvention` (or `Let` for plain
    /// immutable captures). A closure captures by value: it holds a copy
    /// of each value when it is made, and a later assignment to the
    /// binding does not change the copy. A returned closure can capture
    /// a local, so a capture never refers to the frame of its function.
    /// A `Sink` capture moves the value. Capture entries are deduplicated by name and ordered by
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

    /// Dictionary access: `dict["key"]`. An array index `a[i]` is a
    /// `DictAccess` too. A string index `s[i]` lowers to the method
    /// call `s.byte_at(i)`.
    DictAccess {
        dict: Box<IrExpr>,
        key: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Block expression: `{ ... }` with statements and a result.
    Block {
        statements: Vec<IrBlockStatement>,
        result: Box<IrExpr>,
        ty: ResolvedType,
    },
}
```

`IrMatchArm` and `IrBlockStatement`: referenced from `Match` and `Block`
above: are defined on the [Match Arms & Block Statements](blocks.md) page.

## Type Contract

The `ty` field is guaranteed correct after lowering:

| Expression | Type |
| ---------- | ---- |
| `Literal { value: Number(_), .. }` | `Primitive(I32 / I64 / F32 / F64)`: the suffix, else the type that the context expects, else `I32` for an integer and `F64` for a fraction |
| `Literal { value: String(_), .. }` | `Primitive(String)` |
| `Literal { value: Boolean(_), .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: Add/Sub/Mul/Div/Mod, .. }` | Same as operands |
| `BinaryOp { op: Eq/Ne/Lt/Gt/Le/Ge, .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: And/Or, .. }` | `Primitive(Boolean)` |
| `UnaryOp { op: Neg, .. }` | Same as operand |
| `UnaryOp { op: Not, .. }` | `Primitive(Boolean)` |
| `For { body, .. }` | `Generic { base: Struct(seq_id), args: [body.ty()] }` |
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
    ResolvedType::Generic { .. } => {
        if let Some(inner) = module.array_element_ty(ty) {
            // Generate array handling code
        }
    }
    // ...
}
```
