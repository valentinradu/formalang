# IR Gaps and Backend Guidance

**Last Updated**: 2026-04-24
**Status**: Living document

FormaLang ships as a compiler frontend: it produces a type-resolved
`IrModule` and leaves code generation to embedder-supplied backends. Several
lowering problems that a typed target normally expects from a frontend are
*not* performed here. This document lists those gaps, what the IR gives you
today, and what a backend has to fill in.

## 1. Monomorphisation (implemented; limitations remain)

`formalang::ir::MonomorphisePass` is a real pass now — it collects every
`ResolvedType::Generic { base, args }` instantiation, clones each generic
struct or enum once per unique argument tuple, substitutes
`ResolvedType::TypeParam` references in field / method / body types, then
rewrites every `Generic` reference to point at the specialised clone and
drops the original generic definitions.

```rust
use formalang::{compile_to_ir, Pipeline};
use formalang::ir::MonomorphisePass;

let source = "pub struct Box<T> { value: T }\n\
              pub let b: Box<Number> = Box<Number>(value: 1)";
let module = compile_to_ir(source).unwrap();
let result = Pipeline::new().pass(MonomorphisePass).run(module).unwrap();
// After the pass: no structs with `generic_params` remain; `Box<Number>`
// has been replaced by a concrete clone `Box__Number`.
```

### Remaining limitations

- **Generic traits are not supported.** A trait declaration with non-empty
  `generic_params` that survives the pass is reported as an
  `InternalError`. There is no mechanism to specialise a generic trait.
- **Impl-block dispatch ids are not rewritten.** Phase 2b clones each
  generic impl block per specialisation so the methods are attached to
  the specialised struct/enum, but `DispatchKind::Static { impl_id }` at
  call sites still points at the original generic-impl slot. Backends
  that locate methods by walking `module.impls` (matching on
  `ImplTarget`) resolve correctly; backends that honour the per-call
  `impl_id` need a follow-up pass to rewrite those ids from the receiver
  type.
- **External generic type args are not specialised.** The pass walks
  generic arguments on imported types (`ResolvedType::External { type_args, .. }`)
  but does not chase through them to specialise definitions in other
  modules.
- **`ResolvedType::TypeParam` residues are tolerated.** The leftover
  scanner intentionally does not flag `TypeParam(name)` after the pass
  because IR lowering still emits `TypeParam` as a placeholder in a few
  spots (empty array/dict element types, `nil` literal); this is tracked
  as part of the IR-lowering cleanup, not a monomorphise bug.

## 2. Trait method dispatch

`IrExpr::MethodCall` carries a `DispatchKind`:

- `Static { impl_id }` — direct call on a concrete struct/enum with a known
  `impl` block. Backends emit a direct function call.
- `Virtual { trait_id, method_name }` — method call through a type
  parameter bound or a trait object. The IR provides only the declaring
  trait and the method name; it does **not** provide a vtable layout,
  concrete impl resolution, or monomorphised dispatch.

**Recommended backend posture**: for `Virtual` dispatch, the backend must
decide between a vtable (runtime dispatch), duck-typing, or running a
project-local resolver pass that rewrites `Virtual` into `Static` given a
specific receiver type.

## 3. Closure captures (covered)

`IrExpr::Closure` has a `captures: Vec<(String, ResolvedType)>` field
populated during lowering. It lists every free variable the closure body
references, with its type, de-duplicated and in first-reference order.
Closure parameters and locally-bound values are excluded.

Backends can use this to emit capture-environment structs or reject
closures that capture values whose lifetime the target language cannot
express.

Not covered (backend is on its own):

- Capture mode (move vs reference / `mut` vs `sink`): the IR does not
  annotate each capture with an ownership mode.
- Captures through nested data structures: if a closure stores a captured
  value inside another closure that itself escapes, the inner capture is
  still listed but lifetime analysis across the nesting is the backend's
  responsibility.

## 4. Constant folding (partial)

`formalang::ir::ConstantFoldingPass` evaluates:

- Numeric binary ops on literals (`2 + 3 → 5`)
- Boolean and comparison operators
- Unary negation and `!`
- String concatenation of string literals

It does **not** fold struct literals with constant fields, `if` expressions
with a constant condition, method or function calls on constants, or dict
and array literals. Those are left as-is for backends or project-local
passes to handle.

## 5. Dead code elimination (covered)

`DeadCodeEliminationPass` analyses used structs, traits, and enums, prunes
unreachable branches of `if` expressions with constant conditions, and
physically removes unused definitions by rewriting every `StructId` /
`TraitId` / `EnumId` reference across the module. Impl blocks whose
target is removed are also dropped.

DCE semantics: a bare `impl` block does **not** keep its target type
alive. The type must be referenced from a field, a function parameter,
an expression, or a trait constraint for it to survive.

## 6. Escape analysis and lifetime elision

Not implemented. The semantic analyser enforces that closures returned
from a function only capture `sink` parameters or outer-scope bindings
(see `ClosureCaptureEscapesLocalBinding`), but no further escape analysis
or region inference happens in the IR. Backends targeting reference-counted
or arena-allocated languages must compute lifetimes themselves.

## 7. Inlining hints

Not implemented. The IR does not carry `inline` / `no_inline` annotations
or cost estimates. Backends with inlining heuristics use whatever signals
they can derive from the IR structure (function size, call-graph depth).

## 8. FFI calling conventions

`is_extern: true` on an `IrFunction` marks the declaration as implemented
outside FormaLang (no body). The IR does not specify:

- Calling convention (C, stdcall, fastcall)
- Native-symbol mapping (which symbol name the extern binds to)
- Type marshalling rules across the FFI boundary

Backends targeting FFI must encode these outside the IR — typically via a
side table keyed on function name, or a compiler plugin.

## 9. Module nesting

Currently flattened during lowering. Nested modules in source become
qualified names (`outer::inner::Type`) in the IR. Backends that need to
preserve a module hierarchy in their output must reconstruct it from the
qualified names.

---

This list is the authoritative record of what the frontend does *not*
lower for you. When a pass is implemented, move the entry to
`docs/developer/ir.md` and remove it here.
