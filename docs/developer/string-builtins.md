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

## Open questions

1. **Method name namespace.** `len` / `slice` / `is_empty` are
   common identifiers. Built-ins on `Primitive(String)` shouldn't
   collide with user-named functions. Per-receiver-type tables fix
   this but add a lookup dimension.
2. **Lifetime / aliasing rules for `slice`.** Zero-copy borrow is
   fast but exposes shared backing memory. Languages that bake in
   immutability (FormaLang likely) make this safe; the call still
   needs a defined rule.
3. **UTF-8 vs. byte indexing.** `s[i]` is conventionally rune-
   indexed in modern languages. Indexing by code-point requires a
   variable-length walk of the byte buffer. Decide before any
   `s[i]` syntax surfaces.
4. **`Path` and `Regex` follow-on.** Both lift to `string` at the
   WIT boundary today; do their methods (e.g.
   `Path::extension()`) get the same dispatch story?
5. **Generic numeric ops on numbers.** Once String gets built-ins,
   the same machinery wants to apply to `I32::abs()`,
   `F64::sqrt()`, etc. The design should leave room.

## Status in the formawasm backend

Phase 5 #2 was queued for `len` + `slice` as in-module helpers but
found no IR shape to consume. The `__str_eq` / `__str_concat`
runtime helpers (declared by `ModuleBuilder::declare_str_eq` /
`declare_str_concat`) are already wired and would be the obvious
neighbours for the new ops. The path stays a known-restriction
follow-up until upstream commits to a direction here.
