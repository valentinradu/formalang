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

## Open questions

1. **Synthesized nodes' spans.** `closure_conv` lifts `IrExpr::Closure`
   into a top-level function with new bodies. What span does the
   lifted function carry — the original closure's span, the
   capture-environment-construction site's span, or
   `Span::default`?
2. **Monomorphisation and specialisation.** Cloned generic
   functions instantiated per type-arg-tuple all derive from the
   same source span. Is that desirable (debuggers see one source
   for `Vec<I32>::push` and `Vec<String>::push`) or do we want
   per-specialisation distinguishability?
3. **Source-file identity.** Multiple files participate via
   `use`. The IR-side span needs to record both byte range and
   source file. AST `Span` is currently
   `(byte_start, byte_end)` — does it grow to include file id?
4. **`IrFunction::body` per-instruction granularity.** wasm's
   `.debug_line` is conventionally one entry per call-site /
   meaningful expression boundary. Per-`IrExpr` is granular
   enough; per-byte is overkill.
5. **DWARF `DW_AT_decl_file` table.** Backends need a stable
   `(file_id, path)` mapping. Does the IR own this table or
   does each backend reconstruct it from spans?

## Status in the formawasm backend

Phase 5 #5 was queued for DWARF emission and found no spans to
anchor against. The backend already has an emit pipeline
(`src/component.rs::wrap_component`) that could plug a
`.debug_line` and `.debug_info` section into the wrapped artifact;
the missing piece is the IR-level span data. Backend lifting is a
new module (`src/dwarf.rs`) gated behind a `dwarf` cargo feature
once the IR shape lands — likely 5-8 commits depending on which
direction upstream picks.
