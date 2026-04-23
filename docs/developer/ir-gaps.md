# IR Gaps and Backend Guidance

**Last Updated**: 2026-04-23
**Status**: Living document

FormaLang ships as a compiler frontend: it produces a type-resolved
`IrModule` and leaves code generation to embedder-supplied backends. Several
lowering problems that a typed target (C, WGSL, TypeScript, Swift, Kotlin)
normally expects from a frontend are *not* performed here. This document
lists those gaps, what the IR gives you today, and what a backend has to
fill in.

## 1. Monomorphisation (not implemented)

Generics survive lowering. A struct like

```formalang
pub struct Box<T> { value: T }
```

appears in the IR with `generic_params` still populated and
`ResolvedType::Generic { base, args }` wherever it is instantiated.

A real monomorphisation pass would clone each generic definition once per
concrete instantiation (`Box<User>`, `Box<Post>`, ...), substitute type
parameters, and rewrite references. It has not been written.

As a placeholder, `formalang::ir::MonomorphisePass` rejects any module that
still contains `ResolvedType::Generic` after lowering so backends that
cannot handle generics fail loudly rather than emit wrong code.

```rust
use formalang::{compile_to_ir, Pipeline};
use formalang::ir::MonomorphisePass;

let module = compile_to_ir(source).unwrap();
let mut pipeline = Pipeline::new().pass(MonomorphisePass);
pipeline.run(module).unwrap(); // errors if generics remain
```

**Recommended backend posture**: either require concrete types in source
until the pass lands, or implement a project-local monomorphisation pass
before the backend runs.

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

## 5. Dead code elimination (partial)

`DeadCodeEliminationPass` analyses and reports unused structs, traits, and
enums, and prunes unreachable branches of `if` expressions with constant
conditions. It does **not** physically remove unused definitions — doing
so would require rewriting every `StructId` / `TraitId` / `EnumId` in the
module to account for the shifted indices. Until a full reference rewriter
is implemented, the IR contains unused definitions even after DCE runs.

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
