# Cross-Module Code Generation

**Status**: design / open question
**Last updated**: 2026-05-02

This document is for backend authors. It describes what FormaLang's
public IR exposes today for programs that span multiple source files
(`use other::Helper;`), why that exposure is insufficient for a code
generator that wants to actually emit cross-module code, and the
design options on the table to fix it.

The motivating consumer is the WebAssembly Component Model backend at
`~/projects/formawasm`, but every backend with a notion of separate
compilation will hit the same wall. Suggested reading order: this
document, then [`docs/developer/ir.md`](ir.md) for the surrounding IR
shapes.

---

## What the public API exposes today

```rust
pub fn compile_to_ir(source: &str) -> Result<IrModule, Vec<CompilerError>>;

pub fn compile_to_ir_with_resolver<R: ModuleResolver>(
    source: &str,
    resolver: R,
) -> Result<IrModule, Vec<CompilerError>>;
```

Both entry points return a single `IrModule` for the entry-point
source file. The resolver-bearing variant loads other source files
through the `ModuleResolver` trait when the entry-point file's `use`
statements reference them — but those imported modules' IR is **not**
returned to the caller. Internally the semantic analyzer caches them
in `module_cache` keyed on `PathBuf`; the public API surface drops
the cache on the floor when it returns.

The entry-point `IrModule` does record what was imported:

- `module.imports: Vec<IrImport>` — one entry per source module that
  contributed at least one symbol, carrying
  `(module_path, items: Vec<IrImportItem>, source_file: PathBuf)`.
- Type uses against an imported symbol become
  `ResolvedType::External { module_path, name, kind, type_args }`,
  i.e. an opaque reference.

Generic external instantiations are partially handled: see
[`src/ir/monomorphise/external.rs`](../../src/ir/monomorphise/external.rs),
which clones each imported generic into the current module under a
fresh local id and rewrites the `External` references to point at
that clone. Non-generic externals stay opaque.

## Why that's not enough for codegen

A backend cannot emit code for a function whose signature mentions
an `External` struct without knowing the struct's layout.

```formalang
// in helper.fv
pub struct Helper {
    a: I32,
    b: I32,
}

// in main.fv
use helper::Helper;

pub fn read_a(h: Helper) -> I32 {
    h.a
}
```

Lowering `read_a` to wasm requires:

- The wasm value type for parameter `h` (a pointer to `Helper`'s
  linear-memory representation — `i32`).
- The byte offset of field `a` inside `Helper` (zero, here, but the
  backend doesn't know that without seeing `Helper`'s `IrStruct`).

The current public API gives the backend `External { name: "Helper",
.. }` and nothing else. There is no way to ask for `Helper`'s
`IrStruct`, its fields, or its layout.

Workarounds that don't work:

- **Treat `External` as an opaque pointer-sized value.** Lets the
  backend compile the function signature, but `h.a` has no offset to
  load from.
- **Re-run `compile_to_ir` against the imported source file from
  inside the backend.** The backend would have to reimplement module
  resolution and would re-do work the analyzer already did, with no
  guarantee that the second compilation produces the same struct ids.
- **Re-resolve `External` to a local id by walking
  `module.imports[*].source_file`.** The path is exposed but the
  cached `IrModule` behind it is not — the backend would still have
  to recompile each imported file from scratch.

There is no consumer-side fix for this. The information the backend
needs lives inside the analyzer's `module_cache`; until that cache
is exposed (or the imports are inlined into the entry-point
`IrModule` before return), backends are blocked.

## Two design directions

### Direction A — inline imports into the flat IrModule

Make the IR lowerer merge every imported module's definitions into
the entry-point `IrModule`'s flat tables (`structs`, `traits`,
`enums`, `functions`, `impls`, `lets`) and rewrite every
`ResolvedType::External` to its now-local `Struct(StructId)` /
`Trait(TraitId)` / `Enum(EnumId)`. After this pass, `External` is a
transient IR artifact and never reaches the backend.

The skeleton already exists for the generic case: the
`compact` / `external` passes under
[`src/ir/monomorphise/`](../../src/ir/monomorphise/) clone imported
generics under fresh local ids and rewrite references. The work is
to extend that handling to non-generic imports too, run it
unconditionally during `lower_to_ir`, and add a name-collision
strategy (every imported definition needs a fresh qualified name).

What the backend sees afterwards: one `IrModule` whose every
`Struct(StructId)` / `Enum(EnumId)` / `Function(FunctionId)`
references a definition present in the same flat tables. Backends
keep their current single-module assumption — no API churn.

What changes upstream:

- A new pass (or extension of `monomorphise/external.rs`) that
  inlines every `IrImport`'s items.
- `module.imports` becomes informational only — kept for
  source-fidelity tools, ignored by codegen.
- Definitions get qualified names (e.g. `helper::Helper`); the
  IR-lowerer's `try_track_imported_type` path becomes a clone-and-
  rewrite path.
- Tests that observe `External` in IR output need to be updated.

Trade-offs:

- Pro: backends consume one flat IR. The mental model collapses.
  Code-gen for TypeScript / Kotlin / wasm / LLVM all work the same.
- Pro: monomorphisation already does this for generics; extending it
  is structurally familiar work.
- Con: programs with circular imports (already rejected at semantic
  time) stay rejected, but the inlining pass needs a topological
  walk to be deterministic.
- Con: the entry-point IrModule grows; debug info / source spans
  point at qualified names instead of the original module path. This
  is recoverable by walking `module.imports` for ownership
  attribution.

### Direction B — expose a multi-module compilation result

Change the public API to return the cached IrModules:

```rust
pub struct CompiledProgram {
    pub entry: IrModule,
    pub imports: Vec<(PathBuf, IrModule)>,
}

pub fn compile_to_ir_with_resolver<R: ModuleResolver>(
    source: &str,
    resolver: R,
) -> Result<CompiledProgram, Vec<CompilerError>>;
```

`External` references stay in the IR; the backend resolves them by
walking the right `IrModule` from the `imports` map keyed on
`source_file`. This mirrors the way real linkers work — separate
compilation units with cross-references resolved at link time.

What changes upstream:

- New `CompiledProgram` (or named tuple) return type from
  `compile_to_ir_with_resolver`.
- Either keep `compile_to_ir(source)` returning `IrModule` directly
  (single-file is a degenerate `CompiledProgram` with empty
  `imports`), or migrate it too.
- Documentation in `developer/ir.md` of how `External` resolution
  flows through `imports[(source_file, IrModule)]`.

Trade-offs:

- Pro: cleaner separation of concerns. Each `IrModule` represents
  one compilation unit; the backend gets to decide whether to inline
  or emit cross-unit references (e.g. wasm component imports).
- Pro: matches how separate compilation works in C / Rust / Kotlin
  toolchains. Future `--incremental` modes wouldn't need a redesign.
- Con: every backend now has to handle a multi-`IrModule` shape
  even for single-file programs. The simple case gets more verbose.
- Con: monomorphisation, DCE, closure conversion need to grow a
  cross-module mode. Today they assume one IrModule.

### Hybrid

Both directions can co-exist: inline by default (Direction A's
ergonomic story), and offer a `compile_to_ir_separate(...)` variant
that surfaces the multi-module shape (Direction B) for backends that
want true separate compilation. The choice belongs to the call site.

## Open questions for whichever direction

1. **Symbol naming under Direction A.** How are imported items
   re-named in the entry-point's flat tables? Two struct fields can
   share a name across modules; the qualified name needs to be
   stable, parseable, and not collide with user-chosen names.
2. **`IrImport.source_file` semantics post-inline.** Does the field
   stay populated for diagnostic / source-attribution tooling, or
   does the inline pass clear it?
3. **Cyclic imports.** Already rejected at semantic time; the
   inlining pass needs a topological walk so the cycle check fires
   before the clone work begins.
4. **Public-surface promotion.** When a private struct in module A
   is referenced by a public function in module B's import set, what
   happens to its visibility post-inline? Inline-time visibility
   normalisation needs a defined rule.
5. **`module.modules` after inline.** Does the `IrModuleNode` tree
   continue to mirror the source `mod` hierarchy across inlined
   imports? Useful for tools that want to introspect; meaningless
   for codegen.
6. **Direction B's `Pipeline` semantics.** `MonomorphisePass` /
   `DeadCodeEliminationPass` / `ClosureConversionPass` are written
   against one `IrModule` today. Each needs a multi-module variant
   (or cross-module driver) before Direction B can ship.

## Status in the formawasm backend

`~/projects/formawasm` Phase 4 closed with `extern_abi`-bearing
function imports working end-to-end (host-provided externs called
through `wasmtime::component::Linker`). Cross-module type
references stay rejected by the layout planner / type mapper / per-
expression lowering with `NotYetSupported { kind: "External(..)" }`,
documented as a known restriction in
`~/projects/formawasm/PLAN.md`. The backend is ready to lift either
Direction A or Direction B once upstream commits to one.
