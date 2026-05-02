# Source Spans on the IR

**Status**: open / not yet implemented
**Last updated**: 2026-05-02

This document is for backend authors. It describes why the IR
today carries no source-location information and what would need
to change for backends to emit source-mapped debug info (DWARF,
source maps, JVM line tables).

The motivating consumer is the WebAssembly backend at
`~/projects/formawasm` Phase 5 #5, which planned to emit DWARF
sections and found there's no per-expression / per-function span
to anchor them against.

---

## What exists today

The AST carries spans pervasively:

- [`Type::span`](../../src/ast/types.rs), expression spans on
  `Expression` variants, statement spans on `Statement` variants,
  `UseStmt::span`, `Visibility::span`, etc.
- The lexer threads `Span` through every `Token`, the parser
  preserves them, semantic-analysis errors point at them.

The IR does not. A grep for `pub span` / `span: Span` across
[`src/ir/`](../../src/ir/) finds spans only in:

- Synthesized AST nodes that internal passes build on the fly
  (`closure_conv/mod.rs`, `monomorphise/specialise.rs`,
  `module.rs` synthetic let bindings) — every one of these uses
  `Span::default()`, so they carry no real location.
- `IrLowerer.current_span` (a transient field used by the lowerer
  itself for diagnostics during the AST → IR conversion).

Every `IrExpr` variant, every `IrFunction`, every `IrStruct`,
every `IrField` is span-free in the public surface. After
`compile_to_ir` returns, source-location information has been
discarded.

## What backends want

Concrete asks from the WebAssembly backend:

- **Per-`IrExpr` span** so each emitted wasm instruction can be
  paired with `(file, line, column)` in a `.debug_line` table.
- **Per-`IrFunction` span** for `DW_TAG_subprogram` entries
  describing each function's source range.
- **Per-`IrStruct` / `IrEnum` / `IrField` span** for type-info
  DWARF (`DW_TAG_structure_type` / `DW_TAG_member`) so a debugger
  can introspect aggregate values at runtime.
- **A canonical source-file table** mapping a stable index to a
  filesystem path so DWARF's `.debug_line` doesn't repeat path
  strings.

A TypeScript backend would consume the same data for source-map
emission. A Kotlin / JVM backend would consume it for
`LineNumberTable` attributes. A Swift backend, similarly.

### Specifically: source maps for the wasm backend

The wasm backend can emit a v3 source map (the JSON format browser
devtools and Chrome's wasm debugger consume) alongside or instead
of DWARF. Source maps need the same per-`IrExpr` span data as
DWARF — the difference is encoding (`mappings` VLQ string vs.
`.debug_line` opcodes) and packaging (separate `.wasm.map` file +
a `sourceMappingURL` custom section pointing at it, vs. DWARF's
inline `.debug_*` sections).

A backend that has the IR-side spans can emit either or both at
no extra IR-side cost; the picking happens at the codegen layer,
not upstream. Formats sharing the same IR-side input is the main
argument for keeping the span data in the IR rather than carving
a DWARF-specific side-table.

## Two design directions

### Direction A — add spans to every IR node

Mirror the AST shape: every `IrExpr` variant grows a `span: Span`
field, every `IrFunction` / `IrStruct` / `IrEnum` / `IrField`
likewise. The IR-lowerer already holds `current_span` per AST
node visited; piping that through to the synthesized IR nodes is
mostly mechanical.

Trade-offs:

- Pro: backends consume one uniform shape. Every codegen path
  has the span available where it needs it.
- Pro: pre-existing `IrLowerer.current_span` infrastructure means
  the wiring is bookkeeping, not new analysis.
- Con: every IR-shape test in the corpus needs to construct
  spans. `Span::default()` on synthetic nodes works but loses the
  fidelity those nodes could carry (e.g., a closure-converted
  body could point at the original closure expression's span).
- Con: serialised IR (the JSON round-trip path) gets larger
  because every node now carries span data. Serde
  `skip_serializing_if = "Span::is_default"` could mitigate.

### Direction B — parallel span-table keyed by IR id

Keep the IR span-free. Add a side table on `IrModule`:

```rust
pub struct IrSpanTable {
    pub by_function: HashMap<FunctionId, Span>,
    pub by_struct: HashMap<StructId, Span>,
    pub by_expr: HashMap<ExprId, Span>,  // requires new IrExpr id
    ...
}
```

`IrExpr` would need a stable id (it doesn't have one today; ids
live on definitions). The lowerer assigns an `ExprId` per node,
populates the side table from `current_span` at lowering time,
and ships the table alongside `IrModule`.

Trade-offs:

- Pro: IR shape unchanged. Existing tests / consumers don't
  recompile.
- Pro: serialized IR stays small unless the consumer asks for
  the span table.
- Con: every IR transformation pass has to maintain the span
  table — closure conversion lifts an expression into a
  synthesized function, and the side table has to follow.
  Today's transformations don't track ids; adding one risks
  span-table drift.
- Con: introduces a parallel IR-id space (`ExprId`) that has no
  use outside debug info. Cheap-looking but adds a pervasive
  invariant (every `IrExpr` mutation must update the span
  table).

---

## Plan

**Chosen direction:** A — spans on every IR node. Direction B's
parallel-table model is rejected: every transformation pass
(closure conv, monomorphise, DCE) would need to keep a side table
synced with IR mutations, an invariant that's easy to drift and has
no use outside debug info. Direction A's con (slightly larger
serialized IR) is mitigated with `skip_serializing_if =
"IrSpan::is_default"` on every span field.

**File identity:** new `IrSpan` struct distinct from AST `Span`.
The AST `Span` stays as-is (single-file byte range); the IR wraps
it: `IrSpan { span: ast::Span, file: FileId }`. Lowerer combines
the AST span with current-file context at construction time.
Cross-module inlining can therefore have one `IrModule` whose
expressions originate from many files without ambiguity.

### Current state of the relevant code

- AST `Span` is `{ start: Location, end: Location }`
  ([`src/location.rs:44`](../../src/location.rs)) — byte / line /
  column within an unspecified file.
- `IrLowerer.current_span` (transient lowerer field) tracks the
  AST node currently being lowered. Used for diagnostics, not
  threaded into IR.
- Every IR shape (`IrExpr`, `IrFunction`, `IrStruct`, `IrEnum`,
  `IrField`, `IrImpl`, `IrLet`) is span-free.
- `IrModule` already keys imports by `source_file: PathBuf`
  ([`src/ir/types.rs`](../../src/ir/types.rs) `IrImport`); the
  file-table machinery for cross-module spans can sit alongside.

### Steps

1. **Add `FileId` and `IrSpan`.**
   New module `src/ir/span.rs`:
   ```rust
   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
   pub struct FileId(pub u32);

   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
   pub struct IrSpan {
       pub span: crate::location::Span,
       pub file: FileId,
   }

   impl IrSpan {
       pub const fn is_default(&self) -> bool { /* ... */ }
   }
   ```
   `FileId(0)` reserved for "unknown / synthetic with no real file".

2. **Add `IrModule.file_table: Vec<PathBuf>`.**
   Indexed by `FileId.0` (with index 0 reserved). Lowerer registers
   each source file encountered (entry-point first, then each
   imported file in the order they're inlined per the cross-module
   plan) and assigns a fresh `FileId`. `IrModule::file_path(id)`
   accessor returns the `PathBuf` for a given id.

3. **Add `pub span: IrSpan` to every IR shape.**
   - `IrExpr` variants — every variant grows its own `span` field
     (matches the AST's per-variant pattern).
   - `IrFunction`, `IrStruct`, `IrEnum`, `IrField`, `IrImpl`,
     `IrLet`, `IrTrait`, `IrEnumVariant`.
   - `IrFunctionParam` — per-parameter spans for argument-position
     diagnostics.
   - All marked `#[serde(default, skip_serializing_if = "IrSpan::is_default")]`.

4. **Wire the lowerer.**
   Replace `IrLowerer.current_span: Span` with
   `current_ir_span: IrSpan`, including the lowerer's
   `current_file: FileId`. Update every IR-construction site
   (≈ every `IrExpr::*` literal in `src/ir/lower/`) to pass the
   current span through. Helpers `self.span()` / `self.span_with_file(file)`
   keep call sites tight.

5. **Synthesized-node spans (resolves Q1).**
   - **Closure conversion** (`src/ir/closure_conv/`) — the lifted
     top-level function carries the span of the originating
     `IrExpr::Closure`. Implementation: pass the closure's span
     into the lift-function constructor; reuse it for the
     synthesized `IrFunction.span`.
   - **Monomorphisation** (`src/ir/monomorphise/specialise.rs`,
     `external.rs`) — cloned specializations carry the originating
     generic's span. Same source line for every specialization
     (Q2 resolution: matches Rust / C++ / Kotlin convention).
   - **Synthetic let-bindings / wrappers** — carry the span of the
     expression that prompted their synthesis.

6. **Cross-module integration.**
   The cross-module-codegen plan (Direction A inline) clones
   imported types and functions into the entry module. Each clone
   keeps its originating `IrSpan { file: <imported_file_id>, ... }`.
   The entry module's `file_table` records every imported file's
   path. After inlining, expressions across multiple source files
   coexist in one `IrModule`, each disambiguated by its
   `IrSpan.file`.

7. **Tests.**
   - Lowering: a 2-statement function lowers to IR whose statement
     and expression spans match the source byte ranges.
   - Closure conversion: a closure expression at byte range `[a..b]`
     produces a lifted `IrFunction` with `span.span == (a..b)`.
   - Monomorphisation: `Vec<I32>::push` and `Vec<String>::push`
     specializations both carry the original generic's span.
   - Cross-module: an imported function's `IrFunction.span.file`
     differs from the entry-point's after inlining; the entry
     module's `file_table` resolves both correctly.
   - Serde round-trip: an `IrSpan::default()` field is omitted from
     JSON; a populated one round-trips losslessly.

8. **Documentation.**
   `docs/developer/ir.md` — document `IrSpan`, `FileId`,
   `IrModule.file_table`, the lowerer's file-context plumbing, and
   the synthesized-node span policy. Backend authors get a single
   reference for everything they need to anchor DWARF / source
   maps / line tables.

### Resolved open questions

1. **Synthesized nodes' spans.** Original-source span. Closure
   conversion's lifted function carries the closure expression's
   span; monomorphised specializations carry the generic's span;
   synthetic let-bindings carry the prompting expression's span.
2. **Monomorphisation and specialisation.** Single source span
   shared across specialisations. Matches Rust / C++ template /
   Kotlin convention — debuggers stepping into `Vec<I32>::push` and
   `Vec<String>::push` both land on the original generic's source
   line. Per-specialization distinguishability is unwanted (no
   per-specialization source exists).
3. **Source-file identity.** New `IrSpan { span, file: FileId }`
   wraps the AST `Span` rather than mutating it. AST stays
   single-file (byte range only); IR carries the file dimension.
   `IrModule.file_table: Vec<PathBuf>` maps `FileId` → filesystem
   path; `FileId(0)` is reserved for synthetic / unknown.
4. **Granularity.** Per-`IrExpr`. Sufficient for `.debug_line`
   entries (one per meaningful expression boundary), source maps
   (one mapping per expression), and JVM `LineNumberTable` (one per
   statement boundary). Per-byte / per-instruction is the backend's
   responsibility if it wants finer.
5. **DWARF `DW_AT_decl_file` table.** IR owns it.
   `IrModule.file_table` is the canonical `(FileId, PathBuf)`
   mapping; backends consume it directly. No backend-side
   reconstruction.

### Exit criteria

- A FormaLang program lowered with `compile_to_ir` produces an
  `IrModule` whose every `IrExpr` / `IrFunction` / `IrStruct` /
  `IrField` carries a populated `IrSpan` matching the source byte
  range and file.
- A two-file program lowered with `compile_to_ir_with_resolver`
  produces an `IrModule` whose `file_table` lists both source files
  and whose IR nodes correctly attribute each span to its
  originating file.
- formawasm Phase 5 #5 emits `.debug_line` and `.debug_info` DWARF
  sections (or a v3 source map) consuming `IrSpan` directly,
  end-to-end on a non-trivial program with cross-module references.
- This plan file deleted by the implementing PR.

## Status in the formawasm backend

Phase 5 #5 was queued for DWARF emission and found no spans to
anchor against. The backend already has an emit pipeline
(`src/component.rs::wrap_component`) that could plug a
`.debug_line` and `.debug_info` section into the wrapped artifact;
the missing piece is the IR-level span data. Backend lifting is a
new module (`src/dwarf.rs`) gated behind a `dwarf` cargo feature
once the IR shape lands — likely 5-8 commits depending on which
direction upstream picks.
