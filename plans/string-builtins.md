# String Built-In Methods

**Status**: open / not yet implemented
**Last updated**: 2026-05-02

This document is for backend authors. It describes the gap between
what `String` looks like today (an opaque scalar type at the IR
level) and what code generators want for ergonomic string handling
(`s.len()`, `s.slice(start, end)`, indexed access, formatting).

The motivating consumer is the WebAssembly backend at
`~/projects/formawasm` Phase 5 #2, which planned to ship `len` and
`slice` as in-module helpers but found there is no IR shape for
either operation today.

---

## What exists today

`String` is one of the primitive variants in
[`ResolvedType::Primitive`](../../src/ir/resolved_type.rs):

```rust
pub enum PrimitiveType {
    I32, I64, F32, F64,
    Boolean,
    String, Path, Regex,
    Never,
}
```

Construction goes through `IrExpr::Literal { value: Literal::String(text), .. }`.
String operators are limited to:

- `BinaryOp::Add` for concatenation, lowered by every backend as a
  call to whatever runtime helper the target has (formawasm calls
  `__str_concat`).
- `BinaryOp::Eq` / `BinaryOp::Ne` for equality / inequality.

There is no `IrExpr::MethodCall` shape for built-ins. Method
dispatch (see
[`src/ir/lower/expr/dispatch.rs`](../../src/ir/lower/expr/dispatch.rs))
resolves only for receiver types of `Struct(_)` / `Enum(_)` /
`Trait(_)` / `TypeParam(_)`. A `ResolvedType::Primitive(String)`
receiver currently produces a `cannot resolve dispatch` internal
error at semantic time.

Indexed access is similarly unsupported: `IrExpr::DictAccess` is
the only sub-script-style accessor, and it requires the receiver
to be `Dictionary<K, V>` or `Array<T>`. Indexing a `String` returns
no IR shape.

## What backends want

For the WebAssembly backend at minimum:

- **`s.len() -> I32`** — read the `len` slot from the
  `{ ptr, len }` header. Pure data-op, no allocation, no syscalls.
- **`s.slice(start: I32, end: I32) -> String`** — return a fresh
  header pointing at `(s.ptr + start, end - start)`. Zero-copy
  borrow into the same byte buffer. No allocation if we accept the
  aliasing, one bump-allocator call if we copy.
- **`s.is_empty() -> Boolean`**, **`s.starts_with(prefix: String)
  -> Boolean`**, **`s.contains(needle: String) -> Boolean`** as
  natural follow-ups.
- **`s[i] -> I32`** for byte-indexed access (rune access is a
  bigger UTF-8 question).
- **`format!`-style interpolation** for templating. Substantially
  bigger lift; needs a runtime formatter or per-call code-gen.

Other backends (TypeScript, Kotlin, Swift) want the same shapes,
each lowering to the host language's native string API. So this
isn't a wasm-specific question.

## Two design directions

### Direction A — built-in methods via dispatch

Extend `resolve_dispatch_kind` to recognise a `Primitive(String)`
receiver and emit a new `DispatchKind::Builtin { name }`. The IR
gets one new dispatch variant; backends pattern-match on
`(receiver_ty, builtin_name)` and emit the right code.

```rust
pub enum DispatchKind {
    Static { impl_id: ImplId },
    Virtual { trait_id: TraitId, method_name: String },
    Builtin { name: String },  // new
}
```

Built-in methods get registered in a dedicated table at semantic
time:

```text
String::len()   -> () -> I32
String::slice(start: I32, end: I32) -> String
String::is_empty() -> Boolean
...
```

Type-checker validates argument count + types against the table.
The IR carries `MethodCall { receiver, method, method_idx,
dispatch: Builtin { name }, ty }` exactly as it does for
`Static` / `Virtual` today; backends switch on `dispatch` and
route the `Builtin` arm into a per-method emitter.

Pro: one new variant, minimal IR churn. Backends only grow a
new case in the dispatch arm. Method names stay source-level
strings, which is debuggable.
Con: backends now have to know every built-in method name (no
trait blanket). Adding a new built-in is a coordinated change
across upstream + every consumer.
Con: name collisions between user-defined methods and built-ins
need a precedence rule.

### Direction B — built-in methods via synthetic impls

Synthesize an `IrImpl` per primitive type at IR-lowering time.
`String::len()` becomes a regular static method on a synthetic
impl block; the IR shape is identical to user-written impls. The
backend's existing `DispatchKind::Static` path handles them with no
changes.

The synthetic impl's methods need bodies. Two sub-options:

- **Pre-lowered IR bodies.** The synthetic methods carry IR that
  reads the header / does the slice math directly. Every backend
  consumes the same IR — zero per-backend coordination. Requires
  IR shapes for low-level ops the language doesn't otherwise
  expose (raw pointer arithmetic, header field access).
- **Stub bodies + backend-specific intrinsics.** The synthetic
  method's body is `IrExpr::Builtin { name }` (a new variant).
  Backends emit their own impl. Same coordination cost as Direction
  A, just packaged differently.

Pro of Direction B: backends get one uniform shape (everything is
`Static` dispatch). User-written impls and built-ins look the same.
Con: pre-lowered bodies need primitive operations the IR doesn't
have today. Stub bodies recreate Direction A's coordination.

### Hybrid

Define the built-in table once. Use Direction A for
backend-specific operations (slice, equality — anything where
backends need to reach into native APIs) and Direction B for
operations whose IR can be portably expressed (length read, empty
check, slice math when raw pointer arithmetic is exposed).

---

## Plan

> **Reframing.** Directions A / B / Hybrid above all introduce a
> *new* mechanism (built-in dispatch, synthetic impls) parallel to
> `extern`. That premise is rejected: FormaLang already injects
> host-provided behaviour exclusively through `extern`. The actual
> gap is that `ImplTarget` admits only `Struct(StructId) |
> Enum(EnumId)` ([`src/ir/types.rs:191`](../../src/ir/types.rs)) —
> primitives can't be impl targets. Closing that gap dissolves Q1
> (namespace), Q4 (Path / Regex), and Q5 (numerics) for free.

**Chosen approach.** Extend `extern impl` to accept primitive
receivers. Declare `String` (and later `Path` / `Regex` / numeric)
methods in a compiler-shipped prelude `.fv` file. Backends provide
implementations through their existing extern-binding paths
(`extern_abi`, `wasmtime::component::Linker`, etc.). No new dispatch
mechanism, no synthetic impls, no built-in registry.

### Current state of the relevant code

- `extern impl` works on structs and enums via `ImplTarget::Struct |
  Enum` ([`src/ir/types.rs:191`](../../src/ir/types.rs)). Primitives
  can't be impl targets.
- Method dispatch
  ([`src/ir/lower/expr/dispatch.rs`](../../src/ir/lower/expr/dispatch.rs))
  resolves only `Struct(_) | Enum(_) | Trait(_) | TypeParam(_)`
  receivers. `Primitive(_)` produces `cannot resolve dispatch`.
- `s[i]` lowers to `IrExpr::DictAccess`
  ([`src/ir/lower/expr/mod.rs:97`](../../src/ir/lower/expr/mod.rs))
  — only valid for `Dictionary<K, V>` / `Array<T>` receivers.
- No prelude concept; nothing auto-imports today.
- formawasm Phase 4 closed with `extern_abi`-bearing function
  imports working end-to-end. The wiring backends need to consume
  this plan is already in place.

### Steps

1. **Add `ImplTarget::Primitive(PrimitiveType)` to the IR.**
   Update `src/ir/types.rs:191`. Update `trait_id` / `struct_id` /
   `enum_id` helpers — they return `None` for the new variant.
   Update DCE / monomorphise / serde / debug-printers wherever they
   exhaustively match `ImplTarget`.

2. **Parser — accept `extern impl <PrimitiveType>`.**
   `src/parser/defs/extern_decls.rs` and the impl-block parser. The
   receiver type already parses as a type expression; lift the
   constraint that an impl target must be a struct/enum. Add the
   equivalent variant on the AST's impl-target enum.

3. **Semantic — primitive impl resolution.**
   Update dispatch resolution to look up impls keyed on `Primitive(_)`
   receivers when the receiver type is a primitive. `self` parameter
   on a primitive impl method has type `ResolvedType::Primitive(_)`
   matching the impl target — no special handling beyond that.
   Validation: reject **non-extern** `impl <PrimitiveType>` for v1
   (only `extern impl` is needed for the prelude). Allow it later if
   a use case appears.

4. **Compiler-shipped prelude module.**
   Add `src/prelude.fv`, embedded into the binary via `include_str!`.
   Contents (v1 surface):
   ```formalang
   extern impl String {
       fn len(self) -> I32
       fn is_empty(self) -> Boolean
       fn slice(self, start: I32, end: I32) -> String  // zero-copy
       fn starts_with(self, prefix: String) -> Boolean
       fn contains(self, needle: String) -> Boolean
       fn byte_at(self, i: I32) -> I32
   }
   ```
   The compiler driver loads the prelude into every compilation
   before user source. `compile_to_ir` and
   `compile_to_ir_with_resolver` resolve prelude symbols first.
   Document the immutability + zero-copy aliasing contract in a
   doc-comment on the prelude's `extern impl String` block.

5. **Lower `s[i]` for `String` receivers.**
   In [`src/ir/lower/expr/mod.rs:97`](../../src/ir/lower/expr/mod.rs),
   before the generic `Expr::DictAccess` lowering, dispatch on
   receiver type: if `dict: Primitive(String)`, lower to
   `IrExpr::MethodCall { receiver, method: "byte_at", dispatch:
   Static { impl_id }, ty: Primitive(I32) }`. Backends only see a
   method call; no new IR shape needed for indexed string access.

6. **Wasm backend prerequisite work (formawasm Phase 5 #2).**
   New runtime helpers alongside the existing `__str_eq` /
   `__str_concat`: `__str_len`, `__str_is_empty`, `__str_slice`,
   `__str_starts_with`, `__str_contains`, `__str_byte_at`. Each
   `extern impl String::*` method becomes a function import on the
   wasm component, bound through the existing `wasmtime::component::Linker`
   path.

7. **Tests.**
   - Parser: `extern impl String { ... }` parses to
     `ImplTarget::Primitive(PrimitiveType::String)`.
   - Semantic: dispatch resolves `s.len()` to the prelude's extern;
     `s.unknown_method()` produces a clean diagnostic.
   - Lowering: `s[0]` desugars to `MethodCall { method: "byte_at" }`.
   - Integration: a one-file program calling all six methods plus
     `s[i]` lowers cleanly; resulting IR contains six method calls
     and one (desugared) `byte_at` call.

8. **Documentation.**
   Update `docs/user/formalang.md` Extern Declarations to document
   `extern impl <PrimitiveType>`. Update the String section to list
   the prelude methods + immutability + slice aliasing contract.
   `docs/developer/ir.md`: note `ImplTarget::Primitive` and the
   prelude-module assumption.

### Resolved open questions

1. **Method name namespace.** Dissolved. The existing receiver-type
   dispatch handles `extern impl String { fn len }` and
   `extern impl Array { fn len }` natively, the same way it handles
   two structs sharing a method name.
2. **`slice` aliasing.** Zero-copy. Strings are immutable end-to-end
   (no mutation primitive, no `&mut` analogue) — the immutability
   invariant must be confirmed during Step 3 (semantic validation
   sweep) and documented on the prelude's `slice` method.
3. **UTF-8 vs. byte indexing.** Byte. `s[i]` returns the i-th byte
   (`I32`). Code-point access deferred to a future
   `String::code_point_at` extern method if a consumer asks.
4. **`Path` and `Regex`.** Dissolved. Same `extern impl Path { ... }`
   / `extern impl Regex { ... }` mechanism applies, in the same
   prelude file when those methods land.
5. **Numeric built-ins (`I32::abs`, `F64::sqrt`, …).** Dissolved.
   `extern impl I32 { fn abs(self) -> I32 }` works under the same
   machinery, scoped to a separate plan. This plan ships only the
   String surface (plus the `ImplTarget::Primitive` mechanism that
   all primitives will reuse).

### Exit criteria

- A FormaLang program calling `s.len()`, `s.slice(0, 3)`,
  `s.is_empty()`, `s.starts_with("foo")`, `s.contains("bar")`, and
  `s[0]` compiles to IR with no errors.
- formawasm Phase 5 #2 lowers the same program end-to-end; the wasm
  component imports the six runtime helpers and host-provided
  bindings return correct values.
- This plan file deleted by the implementing PR.

---

## Implementation progress

Five commits landed on `string-builtins-design`:

- **SB-1** (`71e093f`): Add `ImplTarget::Primitive(PrimitiveType)`
  variant to the IR; patch all exhaustive matches across
  `dce/remap.rs`, `dce/tests.rs`, `lower/definitions.rs`,
  `lower/expr/helpers.rs`, `monomorphise/compact.rs`,
  `monomorphise/rewrite.rs`, plus two integration tests. Add
  `IrImpl::primitive()` accessor.
- **SB-2 / SB-3 (semantic)** (`53c62cb`): IR lowering recognises
  `extern impl <PrimitiveName>` (bare type names like `String`,
  `I32`) via `primitive_from_name` helper and emits
  `ImplTarget::Primitive`. Semantic `method_exists_on_type` gains a
  primitive branch — receiver types matching `is_primitive_name`
  walk impl blocks targeting that primitive name.
- **SB-4 (prelude)** (`2289cd8`): Ship `src/prelude.fv` with the v1
  String surface: `len`, `is_empty`, `slice`, `starts_with`,
  `contains`, `byte_at`. Embedded via `include_str!` and prepended
  to user source at `compile_with_analyzer_and_resolver`.
- **SB-5** (`bbbf7d6`): `s[i]` for String receivers desugars in
  `lower_dict_access` to `IrExpr::MethodCall { method: "byte_at" }`
  pointing at the prelude impl. Backends see only method calls.

### Remaining work

1. **Source-span fidelity for prelude.** Prepending the prelude
   shifts all user-source spans by the prelude's byte length. Error
   messages and IDE tooling show off-by-prelude-bytes line numbers.
   Cleaner fix: parse prelude separately and merge IR (no source
   concatenation).
2. **`self` parameter typing on primitive impls.** `IrFunctionParam.ty`
   is `None` for `self` today; for primitive impls the resolver
   should infer `Primitive(receiver_type)` for body type-checking.
   Lowerer / type-checker may need adjustments.
3. **ResolveReferencesPass primitive-aware method lookup.** The
   pass's `lookup_method_idx` walks impls — verify it correctly
   handles `ImplTarget::Primitive` lookups for receiver types.
4. **Tests and documentation** per plan steps 7 and 8. Integration
   tests covering the six methods + `s[i]`. Update
   `docs/user/formalang.md` Extern Declarations section.
5. **formawasm Phase 5 #2 (separate repo).** Wire the six runtime
   helpers (`__str_len`, `__str_slice`, etc.) via the existing
   `wasmtime::component::Linker` path. Tracked in formawasm's PLAN.

What's already shipped (SB-1 through SB-5) covers the common case:
a FormaLang program calling `s.len()`, `s.slice(...)`, etc., and
`s[i]` compiles end-to-end with backends seeing standard
`IrExpr::MethodCall` shapes. The prelude is automatic — no `use`
needed.

## Status in the formawasm backend

Phase 5 #2 was queued for `len` + `slice` as in-module helpers but
found no IR shape to consume. The `__str_eq` / `__str_concat`
runtime helpers (declared by `ModuleBuilder::declare_str_eq` /
`declare_str_concat`) are already wired and would be the obvious
neighbours for the new ops. The path stays a known-restriction
follow-up until upstream commits to a direction here.
