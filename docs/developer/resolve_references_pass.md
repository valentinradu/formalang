# Resolve-references pass

**Status:** proposed
**Driver:** formawasm backend (and any future backend that emits
integer-indexed code — JVM, LLVM, native).
**Scope:** new IR pass `src/ir/resolve_refs.rs`; small additions to
`IrExpr` / `IrBlockStatement` / `IrFunctionParam` / `IrField` /
`IrEnumVariant` / `IrMatchArm`; `Pipeline` insertion; tests.

## Problem

The IR currently uses **strings** as the load-bearing identifier in
every reference site:

| Site | Current shape |
| --- | --- |
| `IrExpr::Reference` | `path: Vec<String>` |
| `IrExpr::LetRef` | `name: String` |
| `IrExpr::FunctionCall` | `path: Vec<String>` |
| `IrExpr::FieldAccess` | `field: String` |
| `IrExpr::SelfFieldRef` | `field: String` |
| `IrExpr::EnumInst` | `variant: String`, `fields: Vec<(String, _)>` |
| `IrExpr::StructInst` | `fields: Vec<(String, _)>` |
| `IrExpr::MethodCall` | `method: String` |
| `IrMatchArm` | `variant: String`, `bindings: Vec<(String, _)>` |
| `IrBlockStatement::Let` | `name: String` |

Every backend that emits integer-indexed code has to:

1. Walk modules to resolve `path` against `module.functions` /
   `module.structs` / `module.enums` / `module.lets` /
   `module.modules` (nested `mod` blocks).
2. Maintain a per-function scope stack to resolve `LetRef.name` and
   `Reference.path: [single_name]` against `IrBlockStatement::Let`
   bindings and `IrFunctionParam` parameters, including shadowing.
3. Look up `field` / `variant` / `method` strings against the type
   carried on the parent expression to turn names into
   field/variant/method indices.

Re-doing this in every backend is wasted work, error-prone (each
backend re-invents shadowing semantics), and forces the backend to
hold a name-resolution context that has nothing to do with code
generation.

## Goal

After this pass runs, every reference site in the IR carries a
**typed ID** that points unambiguously at its target. The original
string is preserved alongside the ID for diagnostics — backends never
need to re-resolve. New backends inherit the resolution work for
free.

## Why not drop strings entirely?

Diagnostics. Source-level error messages need names ("`undefined
variable 'foo'`", "`field 'bar' has wrong type`"), and the backend
emits them as part of its own typed errors when its layout planner
or codegen rejects something. Keeping `name: String` alongside
`binding_id: BindingId` costs ~24 bytes per node and makes every
diagnostic 100% better.

## Where it lives in the pipeline

```text
SemanticAnalyzer
   ↓
lower_to_ir
   ↓
MonomorphisePass               -- specialises generic types/impls
   ↓
ResolveReferencesPass          -- ★ this pass
   ↓
ClosureConversionPass          -- lifts closures; rewrites captures
   ↓
DeadCodeEliminationPass
   ↓
WasmBackend / other backends
```

Insertion points:

- **After `MonomorphisePass`** — monomorphisation creates new `IrFunction`s
  and rewrites types; its outputs would otherwise leak unresolved
  refs into the rewritten bodies. Easier to resolve once everything
  has settled.
- **Before `ClosureConversionPass`** — closure conv looks at each
  closure's `captures` list and decides whether each captured name
  refers to an outer-scope binding. With resolved `BindingId`s, the
  "outer scope?" check becomes "is this `BindingId` introduced inside
  the closure body, yes/no" — much simpler than re-doing the scope
  walk.

## Proposed IR shape changes

### New ID types

```rust
// src/ir/mod.rs

/// Identifier for a binding inside a function body — either a
/// `Let` introduced by an `IrBlockStatement::Let` or a parameter
/// from `IrFunctionParam`. Unique within the containing function
/// only; not stable across functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BindingId(pub u32);

/// Position of a field within `IrStruct.fields` (or, for tuples,
/// the `IrExpr::Tuple.fields` Vec).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldIdx(pub u32);

/// Position of a variant within `IrEnum.variants`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantIdx(pub u32);

/// Position of a method within `IrImpl.functions` (Static dispatch)
/// or `IrTrait.methods` (Virtual dispatch).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MethodIdx(pub u32);

/// Identifier for a module-scope `let` binding in `IrModule.lets`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LetId(pub u32);
```

### Reference-site resolution

A single resolved-target enum used by `IrExpr::Reference`:

```rust
pub enum ReferenceTarget {
    Function(FunctionId),
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
    ModuleLet(LetId),
    Local(BindingId),      // function-local `let` binding
    Param(BindingId),      // function parameter
    External {
        module_path: Vec<String>,
        name: String,
        kind: ImportedKind,
    },
}
```

External references stay name-keyed because cross-module linking
hasn't landed yet (Phase 4 in `formawasm/PLAN.md`); they're fully
resolved when that work happens.

### IR variant updates

```rust
// src/ir/expr.rs

pub enum IrExpr {
    Reference {
        path: Vec<String>,            // diagnostic
        target: ReferenceTarget,      // ★ NEW: load-bearing
        ty: ResolvedType,
    },
    LetRef {
        name: String,                 // diagnostic
        binding_id: BindingId,        // ★ NEW
        ty: ResolvedType,
    },
    FunctionCall {
        path: Vec<String>,            // diagnostic
        function_id: FunctionId,      // ★ NEW
        args: Vec<(Option<String>, Self)>,
        ty: ResolvedType,
    },
    FieldAccess {
        object: Box<Self>,
        field: String,                // diagnostic
        field_idx: FieldIdx,          // ★ NEW
        ty: ResolvedType,
    },
    SelfFieldRef {
        field: String,                // diagnostic
        field_idx: FieldIdx,          // ★ NEW
        ty: ResolvedType,
    },
    StructInst {
        struct_id: Option<StructId>,
        type_args: Vec<ResolvedType>,
        fields: Vec<(String, FieldIdx, Self)>,    // ★ NEW: field_idx
        ty: ResolvedType,
    },
    EnumInst {
        enum_id: Option<EnumId>,
        variant: String,                          // diagnostic
        variant_idx: VariantIdx,                  // ★ NEW
        fields: Vec<(String, FieldIdx, Self)>,    // ★ NEW: field_idx
        ty: ResolvedType,
    },
    MethodCall {
        receiver: Box<Self>,
        method: String,                           // diagnostic
        method_idx: MethodIdx,                    // ★ NEW
        args: Vec<(Option<String>, Self)>,
        dispatch: DispatchKind,
        ty: ResolvedType,
    },
    // unchanged: Literal, Closure, ClosureRef, BinaryOp, UnaryOp,
    // If, For, Match, Block, Array, Tuple, DictLiteral, DictAccess
}

pub enum IrBlockStatement {
    Let {
        binding_id: BindingId,        // ★ NEW
        name: String,
        mutable: bool,
        ty: Option<ResolvedType>,
        value: IrExpr,
    },
    Assign { target: IrExpr, value: IrExpr },
    Expr(IrExpr),
}

pub struct IrMatchArm {
    pub variant: String,                          // diagnostic
    pub variant_idx: VariantIdx,                  // ★ NEW
    pub is_wildcard: bool,
    pub bindings: Vec<(String, BindingId, ResolvedType)>,   // ★ NEW: BindingId
    pub body: IrExpr,
}
```

### IrFunctionParam

```rust
pub struct IrFunctionParam {
    pub binding_id: BindingId,        // ★ NEW
    pub name: String,
    // … existing fields unchanged
}
```

### Captures

`IrExpr::Closure.captures` already references outer-scope bindings.
After the resolve pass, captures carry `BindingId`s of the outer
binding so closure conversion has stable identity:

```rust
Closure {
    params:   Vec<(ParamConvention, BindingId, String, ResolvedType)>,  // ★ NEW: BindingId per param
    captures: Vec<(BindingId, String, ParamConvention, ResolvedType)>,  // ★ NEW
    body: Box<Self>,
    ty: ResolvedType,
}
```

ClosureConversionPass uses the captured `BindingId`s to detect which
references in the closure body need to be rewritten to env-struct
field accesses.

## The pass

`src/ir/resolve_refs.rs` provides `pub struct ResolveReferencesPass;`
implementing `IrPass`. The pass:

1. **Builds a module-scope symbol table** keyed by name within each
   module/sub-module: `{ functions, structs, enums, traits,
   module_lets }`. Emit a `CompilerError::DuplicateName` if any
   collision (semantic analysis already catches this, so the pass
   asserts the invariant rather than re-checking).
2. **Walks every function body** (`IrFunction.body`,
   `IrLet.value`, `IrImpl.functions[*].body`):
   - Maintain a stack of scopes. Push a scope for each `Block` /
     `For` / `Match` arm / `Closure` body.
   - Each `IrBlockStatement::Let` and `IrFunctionParam` is assigned
     a fresh `BindingId` (counter starts at 0 per function and
     increments). Inserted into the current scope by `name`.
   - `Reference { path }`:
     - Single-element path → look up local scope first, then module
       scope.
     - Multi-element path → walk module scope (nested `mod`s).
     - Set `target` to the matching variant of `ReferenceTarget`.
   - `LetRef { name }` → look up local scope. Must match a `Local` or
     `Param`; emit a `CompilerError::UnboundLocal` if missing.
   - `FunctionCall { path }` → resolve to `FunctionId`.
3. **Resolves field/variant/method indices** by walking the parent
   expression's type:
   - `FieldAccess` / `SelfFieldRef` → look up `field` in the
     `IrStruct.fields` of `object.ty()` (or the impl's struct for
     `SelfFieldRef`); set `field_idx`.
   - `StructInst.fields` → look up each `(name, _)` in the struct;
     set `field_idx`.
   - `EnumInst { variant, fields }` → look up `variant` in the
     `IrEnum.variants`; set `variant_idx`. Look up each `field` in
     the variant's `IrEnumVariant.fields`; set `field_idx`.
   - `MethodCall { method, dispatch }` → for `Static { impl_id }`,
     look up in `IrImpl.functions`; for `Virtual { trait_id, .. }`,
     look up in `IrTrait.methods`. Set `method_idx`.
   - `IrMatchArm.variant` → look up in the scrutinee's enum;
     set `variant_idx`. Each `bindings[*]` gets a fresh `BindingId`
     (the arm body sees them as `Local`s).
4. **Emits typed errors** for any unresolved name, wrapping the
   string and a context (module path + function name) so backends
   never see unresolved refs:
   - `CompilerError::UnboundReference { path, in_function }`
   - `CompilerError::UnboundLocal { name, in_function }`
   - `CompilerError::UnknownField { struct_name, field }`
   - `CompilerError::UnknownVariant { enum_name, variant }`
   - `CompilerError::UnknownMethod { receiver_type, method }`

## Three categories, one pass

The original distinction across three resolution categories
(module-scope, function-local, field/variant) maps to one pass with
three internal walkers. Backends consume the unified output:

| Category | What the pass produces | What the backend still does |
| --- | --- | --- |
| Module-scope items | `FunctionId` / `StructId` / `EnumId` / `LetId` | Look up its own emitted index for that ID (e.g. wasm function index). |
| Function-local bindings | Stable `BindingId` per binding | Map `BindingId` → target slot (wasm local index, JVM slot, …). |
| Type-dependent fields/variants/methods | `FieldIdx` / `VariantIdx` / `MethodIdx` | Map index → target offset/slot via its own layout planner. |

The backend's remaining work is purely about target storage — no
name resolution.

## Pipeline insertion

```rust
// src/lib.rs (re-exports)
pub use ir::ResolveReferencesPass;

// src/ir/mod.rs
pub use resolve_refs::ResolveReferencesPass;

mod resolve_refs;
```

Backend-side usage:

```rust
let bytes = Pipeline::new()
    .pass(MonomorphisePass::default())
    .pass(ResolveReferencesPass::new())   // ★ new
    .pass(ClosureConversionPass::new())
    .pass(DeadCodeEliminationPass::new())
    .emit(module, &WasmBackend::new())?;
```

The pass is **load-bearing for any backend that emits integer-indexed
code** (formawasm, eventual JVM/LLVM); a pure-source-printing
backend (e.g. a JS / TS emitter) can run without it. Backends that
require resolution should document it in their README.

## Test plan

`tests/resolve_refs_test.rs`:

```rust
type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult = Result<(), TestError>;

#[test]
fn local_binding_id_assigned_for_let() -> TestResult { /* … */ }

#[test]
fn function_call_path_resolves_to_function_id() -> TestResult { /* … */ }

#[test]
fn nested_module_path_resolves() -> TestResult { /* … */ }

#[test]
fn shadowing_resolves_to_innermost_binding() -> TestResult { /* … */ }

#[test]
fn field_access_resolves_to_field_idx() -> TestResult { /* … */ }

#[test]
fn enum_inst_resolves_variant_and_field_indices() -> TestResult { /* … */ }

#[test]
fn match_arm_bindings_get_fresh_binding_ids() -> TestResult { /* … */ }

#[test]
fn unbound_local_returns_typed_error() -> TestResult { /* … */ }
```

Plus end-to-end:

- After this pass runs, `find_residual_closures` still works.
- `ClosureConversionPass` produces the same output for resolved IR
  as it did for unresolved IR — the closure-conv tests should pass
  unchanged after the pass adapts to use `BindingId`s instead of
  string-based capture detection.

## Migration

Every match site against the affected `IrExpr` variants needs to add
the new fields. The pass has to land in one PR (otherwise the IR is
inconsistent between micro-commits).

Suggested commit shape:

1. `ir: add BindingId / FieldIdx / VariantIdx / MethodIdx / LetId types`
2. `ir: add target / binding_id / *_idx fields to expression variants` — defaulted to placeholder values; pre-existing matches updated.
3. `ir: ResolveReferencesPass — module-scope resolution`
4. `ir: ResolveReferencesPass — function-local resolution`
5. `ir: ResolveReferencesPass — field / variant / method resolution`
6. `ir: integrate ResolveReferencesPass before ClosureConversionPass`
7. `closure_conv: switch capture detection to BindingId`
8. `tests: end-to-end resolve-refs coverage + closure-conv unchanged`

## Out of scope

- **External `ResolvedType::External` references** — cross-module
  linking is Phase 4 in formawasm; the pass leaves them name-keyed.
- **Trait dispatch resolution** — `DispatchKind::Virtual` continues
  to look up via vtable at runtime; the pass only resolves the
  *trait* and *method* identity, not the concrete vtable slot.
- **Re-exported names** (`use foo::Bar;`) — semantic analyser
  resolves these into the import table; the pass treats them as
  module-scope items.

## Why not just do it in formawasm?

formawasm could maintain its own name-resolution context. Other
backends would each repeat the same logic. Resolution is
target-independent, so doing it once in formalang:

- Removes ~300 lines of duplicated walker code per backend.
- Catches name errors as `CompilerError`s before any backend runs,
  with a uniform diagnostic shape.
- Lets backends fail at codegen for *their own* invariants (layout,
  ABI), not for upstream name issues.

The cost is a one-time IR shape extension. After that, every
backend benefits.
