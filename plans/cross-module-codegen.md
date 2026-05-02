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

---

## Plan

**Chosen direction:** A — inline imports into the entry-point
`IrModule` so `External` becomes a transient, never-reaching-codegen
artifact. Direction B's `CompiledProgram` shape is deferred until a
backend actually needs separate compilation; nothing in the plan
forecloses adding it later as a hybrid escape hatch.

### Current state of the relevant code

- `compile_to_ir_with_resolver` ([`src/lib.rs:193`](../../src/lib.rs))
  lowers only the entry-point AST and returns its `IrModule` —
  imported modules' IR is never produced.
- The semantic analyzer's `module_cache`
  ([`src/semantic/mod.rs:94`](../../src/semantic/mod.rs)) caches
  `(File, SymbolTable)` per imported `PathBuf`, not IR.
- `MonomorphisePass::with_imports`
  ([`src/ir/monomorphise/mod.rs:92`](../../src/ir/monomorphise/mod.rs))
  accepts `HashMap<Vec<String>, IrModule>` but the public API never
  populates it.
- `specialise_external_instantiations`
  ([`src/ir/monomorphise/external.rs:84`](../../src/ir/monomorphise/external.rs))
  clones imported structs/enums into the entry module — but only
  when `type_args` is non-empty (i.e. generics). Non-generic
  `External` references are untouched.
- IrModule names already carry `::`-qualified segments for nested
  source modules ([`src/ir/module.rs:95`](../../src/ir/module.rs) doc
  comment). The lexer rejects `::` inside identifiers, so the
  separator is already reserved — answers the design's open question
  on naming for free.

### Steps

1. **Build per-import `IrModule`s in `compile_to_ir_with_resolver`.**
   After semantic analysis, walk `analyzer.module_cache()` and call
   `ir::lower_to_ir(&file, &symbols)` for each entry to produce one
   `IrModule` per imported source file. Resolve each entry's
   `module_path: Vec<String>` from the entry module's `imports[*]`
   (the analyzer already records the `source_file` → `module_path`
   mapping via `IrImport.source_file`). Collect into
   `HashMap<Vec<String>, IrModule>`.
   *Files:* `src/lib.rs`, possibly a small helper in `src/ir/mod.rs`.

2. **Generalise `specialise_external_instantiations` to non-generics.**
   Drop the `if !type_args.is_empty()` gate at
   [`external.rs:42`](../../src/ir/monomorphise/external.rs) so the
   collector enqueues every `External` regardless of arity. In
   `specialise_external`, when `args.is_empty()`, skip substitution
   but still clone-and-rename the source struct/enum into the local
   module under its qualified name (`module::path::Name`), with
   `mangle_external_name` falling back to the bare qualified form
   when no type arguments are present.
   Extend the dispatch branch (currently only structs and enums) to
   also handle imported **traits** — trait references can appear in
   bounds and impls.

3. **Inline imported functions, impls, and lets.**
   New module `src/ir/monomorphise/external_items.rs` (or extension
   of `external.rs`). Walk imported `IrModule`s for every `pub fn`,
   `pub let`, and `impl` block reachable from the entry module's
   `External` references (transitive closure via the same worklist
   pattern). Clone each into the local tables under qualified names;
   add a parallel `rewrite_external_function_references` /
   `rewrite_external_let_references` walker (mirroring the existing
   `rewrite_external_references` for types) that updates call-sites
   and value-references.
   Hook this between Phase 1a (specialise types) and Phase 1b
   (generic instantiation collection) inside `MonomorphisePass::run`.

   **Cycle guard (defence in depth).** The pass maintains an
   `in_progress: HashSet<Vec<String>>` of module paths currently
   being cloned. If the worklist tries to enter a path already in
   `in_progress`, return `InternalError { detail: "monomorphise:
   cyclic import .." }`. Semantic analysis already rejects cycles;
   this catches a regression in that contract before it becomes a
   miscompile.

4. **Merge `IrModuleNode` trees.**
   For each imported `IrModule`, splice its `modules: Vec<IrModuleNode>`
   into the entry module's tree under a path matching the import's
   `module_path`. If the entry already has a node along the path
   (e.g. nested local `mod foo { ... }` plus `use foo::bar::Helper`
   touching the same `foo`), merge child lists rather than
   duplicating. The flat per-type vectors stay authoritative; this
   tree update is purely informational, ignored by codegen, consumed
   by source-introspection tools.

5. **Wire the populated `imported_modules` into the public pipeline.**
   In `compile_to_ir_with_resolver`, after `lower_to_ir`, run
   ```rust
   Pipeline::new()
       .pass(MonomorphisePass::default().with_imports(imports_map))
       .run(module)
   ```
   Single-file `compile_to_ir` keeps its current shape (empty imports
   map → no inlining work, fast path preserved).

6. **Tests.**
   Add a two-file integration test (`tests/integration_cross_module.rs`):
   `main.fv` calls a non-generic function from `helper.fv` and reads
   a non-generic struct field. Assert the resulting IR has the
   helper's struct + function inlined under qualified names and
   contains zero `ResolvedType::External` references.
   Add a cycle-guard regression test that constructs an
   `imported_modules` map with a manufactured cycle and asserts the
   pass returns the `InternalError { detail: "monomorphise: cyclic
   import .." }` (the semantic-rejection contract is tested
   separately).
   Update existing IR-snapshot tests that currently observe `External`
   surviving (the design note flags this as a known consequence).

7. **Documentation.**
   Update `docs/developer/ir.md` to state that `External` is a
   transient IR type which never reaches the backend post-`MonomorphisePass`.
   Update the doc-comments on `compile_to_ir_with_resolver` to
   describe the inlining behaviour. Note in `IrImport`'s doc-comment
   that the field stays populated for source-attribution tooling.

### Resolved open questions

1. **Symbol naming.** Qualified `module::path::Name` form. `::` is
   already reserved by the lexer and already used by the lowerer for
   nested-module type names; the same convention extends to inlined
   imports.
2. **`IrImport.source_file` post-inline.** Stays populated. Backends
   ignore it; diagnostic / source-attribution tools rely on it.
3. **Cyclic imports.** Still rejected at semantic time **and**
   defence-in-depth at the IR layer: the inline pass tracks the
   in-progress import path on its worklist and errors with
   `InternalError { detail: "monomorphise: cyclic import .." }` if a
   module re-enters before its clone completes. The semantic
   rejection is the contract; the IR guard catches a regression in
   that contract instead of silently producing miscompiled output.
4. **Visibility post-inline.** Automatic — the IR has no
   public/private split (visibility is enforced at semantic time
   only); all inlined definitions are simply module-local.
5. **`module.modules` after inline.** Keep mirroring source `mod`
   hierarchy across inlined imports. Codegen ignores it; tools that
   introspect the source-module tree get a complete picture.
6. **Pipeline semantics.** N/A under Direction A — single `IrModule`
   throughout. `MonomorphisePass` / `DeadCodeEliminationPass` /
   `ClosureConversionPass` keep their current single-module
   contracts unchanged.

### Exit criteria

- `compile_to_ir_with_resolver` returns an `IrModule` whose IR
  contains zero `ResolvedType::External` references after the
  pipeline runs.
- formawasm Phase 4 R2 (`~/projects/formawasm`) lowers a two-file
  program with cross-module struct + function references end-to-end;
  layout planner / type mapper / expression lowering no longer reject
  with `NotYetSupported { kind: "External(..)" }`.
- This plan file is deleted as part of the implementing PR.

## Status in the formawasm backend

`~/projects/formawasm` Phase 4 closed with `extern_abi`-bearing
function imports working end-to-end (host-provided externs called
through `wasmtime::component::Linker`). Cross-module type
references stay rejected by the layout planner / type mapper / per-
expression lowering with `NotYetSupported { kind: "External(..)" }`,
documented as a known restriction in
`~/projects/formawasm/PLAN.md`. The backend is ready to lift either
Direction A or Direction B once upstream commits to one.
