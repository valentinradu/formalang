# IR Gaps and Backend Guidance

**Last Updated**: 2026-04-26
**Status**: Living document

FormaLang ships as a compiler frontend: it produces a type-resolved
`IrModule` and leaves code generation to embedder-supplied backends.
Several lowering problems that a typed target normally expects from a
frontend are *not* performed here. This document lists those gaps,
what the IR gives you today, and what a backend has to fill in.

## 1. Monomorphisation (implemented end-to-end except generic traits)

`formalang::ir::MonomorphisePass` is a real pass — it collects every
`ResolvedType::Generic { base, args }` instantiation, clones each
generic struct, enum, **or function** once per unique argument tuple,
substitutes `ResolvedType::TypeParam` references in field / method /
param / body types, then rewrites every `Generic` reference and every
`FunctionCall` path to point at the specialised clone and drops the
original generic definitions.

```rust
use formalang::{compile_to_ir, Pipeline};
use formalang::ir::MonomorphisePass;

let source = "pub struct Box<T> { value: T }\n\
              pub let b: Box<Number> = Box<Number>(value: 1)";
let module = compile_to_ir(source).unwrap();
let result = Pipeline::new().pass(MonomorphisePass::default()).run(module).unwrap();
// After the pass: no structs with `generic_params` remain; `Box<Number>`
// has been replaced by a concrete clone `Box__Number`.
```

`MonomorphisePass` runs in five sub-phases: 1a specialises external
generic types via `with_imports`, 2 specialises generic structs/enums
and rewrites references, 2b clones impls per specialisation, 2c
rewrites `DispatchKind::Static { impl_id }` at call sites, 2d
specialises generic functions and rewrites their call sites, and 2e
devirtualises every `DispatchKind::Virtual` whose receiver became
concrete after specialisation. FormaLang has no dynamic dispatch —
trait values are rejected at semantic time
(`CompilerError::TraitUsedAsValueType`), so any virtual dispatch on a
concrete receiver surviving Phase 2e is reported as an
`InternalError`.

Phase 2c rewrites `DispatchKind::Static { impl_id }` at every call
site so it points at the per-specialisation impl clone (audit #5b).
Phase 1a specialises external generic types via
`MonomorphisePass::with_imports`, cloning imported generic
definitions into the current module with substituted arguments
(audit #45).

### Remaining limitation

- **Generic traits are not supported.** A trait declaration with
  non-empty `generic_params` that survives the pass is reported as an
  `InternalError`. Source has no way to use a generic trait yet
  (`<T: Trait<X>>` constraint args and `impl Trait<X> for Foo`
  aren't parsed), so declared-but-unused generic traits are the only
  source-reachable case today. Tracked as its own follow-up PR.

## 2. Constant folding (intentionally bounded)

`formalang::ir::ConstantFoldingPass` evaluates:

- Numeric binary ops on literals (`2 + 3 → 5`)
- Boolean and comparison operators
- Unary negation and `!`
- String concatenation of string literals
- `if true / false` collapsing to the taken branch (also performed by
  DCE on dead branches; the two passes coexist by design)

It does **not** fold:

- Struct, enum, tuple, array, or dict literals with all-literal
  contents. The IR has no "constant aggregate" representation, no
  pass in this crate consumes such a marker, and backends that emit
  static data do their own scan over `IrExpr::Literal` children.
  Adding the fold without a consumer is performative; if a backend
  needs it, it lives behind a `Pipeline::pass` opt-in there.
- Method or function calls on constants. Compile-time evaluation
  requires a `pure` / `const fn` language-level annotation that
  FormaLang does not currently model. That's a language-design item
  on top of a frontend extension; tracked separately rather than
  forced into this doc.

## 3. Escape analysis and lifetime elision

Not implemented as a general pass. The semantic analyser enforces
targeted escape constraints — closures returned from a function may
only capture `sink` parameters or outer-scope module-level bindings
(`ClosureCaptureEscapesLocalBinding`); closures stored in arrays /
tuples / dicts / struct fields, returned aggregates (struct / enum /
tuple / array / dict), or assigned to outer-scope `mut` bindings are
all run through the same outlives-the-frame rule. No region inference
happens in the IR; backends targeting reference-counted or arena-
allocated languages must compute lifetimes themselves.

---

## Implemented (no longer gaps)

The items below were listed as gaps in earlier revisions and are now
covered by the frontend. They are kept here briefly for the benefit of
backends written against older snapshots; consult `docs/developer/ir.md`
for the canonical descriptions.

- **Closure capture mode.** `IrExpr::Closure.captures` is
  `Vec<(String, ParamConvention, ResolvedType)>`. Each entry inherits
  the outer binding's convention (`Let` / `Mut` / `Sink`) so backends
  can decide between move/borrow/mutation/ownership semantics.
- **Inlining hints.** `IrFunction.attributes` (and
  `IrFunctionSig.attributes`) is `Vec<FunctionAttribute>`. Source
  syntax: `inline fn`, `no_inline fn`, `cold fn` keyword prefixes
  before `fn`. Frontend passes them through unchanged; backends with
  inlining heuristics consume them as hints.
- **FFI calling conventions.** `IrFunction.extern_abi` is
  `Option<ExternAbi>`. Source syntax: `extern fn` (defaults to
  `ExternAbi::C`), `extern "C" fn`, `extern "system" fn`. Unknown ABI
  strings are rejected at parse time. Inherent `is_extern()` method
  preserves the boolean check at call sites that don't care which ABI.
- **Module nesting.** `IrModule.modules` is a `Vec<IrModuleNode>` that
  mirrors the source `mod foo { ... }` tree. Each node carries
  `Vec<StructId>` / `Vec<TraitId>` / `Vec<EnumId>` /
  `Vec<FunctionId>` for definitions declared directly in that module
  plus nested sub-modules. The flat per-type vectors on `IrModule`
  remain authoritative; the tree is an *index* on top of them, opt-in
  for backends that need the hierarchy.
- **Dead code elimination.** `DeadCodeEliminationPass` analyses used
  structs, traits, and enums, prunes unreachable branches of `if`
  expressions with constant conditions, and physically removes unused
  definitions by rewriting every `StructId` / `TraitId` / `EnumId`
  reference across the module. Impl blocks whose target is removed
  are also dropped. Note: a bare `impl` block does **not** keep its
  target type alive.

---

This list is the authoritative record of what the frontend does *not*
lower for you. When a pass is implemented, move the entry to
`docs/developer/ir.md` and remove it (or move it to the section
above) here.
