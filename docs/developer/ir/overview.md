# IR Overview

The IR is the recommended output for building code generators. Code
generation is not built into the library: backends are external and
plug in via the `IrPass`/`Backend` trait system defined in
`src/pipeline.rs`.

> **Note**: For syntax analysis or source-level tooling, use the
> [AST](../ast/overview.md) instead. The IR is optimized for code
> generation, not source fidelity.

## What the IR provides

The IR is a type-resolved representation of FormaLang programs,
produced after semantic analysis. Unlike the AST which preserves
source syntax, the IR provides:

- **Resolved types** on every expression
- **Linked references** (IDs pointing to definitions, not string names)
- **Flattened structure** optimized for code generation
- **Visitor pattern** for traversal

## Compiler Pipeline

```text
Source
  |
  v
Lexer -> Tokens
  |
  v
Parser -> AST (File)
  |
  v
Semantic Analyzer -> Validated AST + SymbolTable
  |
  v
IR Lowering -> IrModule  <-- This reference
  |
  v
Plugin System -> [IrPass, ...] -> Backend -> Output
```

## AST vs IR

| Feature                       | AST | IR                            |
| ----------------------------- | --- | ----------------------------- |
| Source locations (spans)      | Yes | Yes (`IrSpan`)                |
| Multi-file source identity    | No  | Yes (`FileId` + `file_table`) |
| Type resolution               | No  | Yes                           |
| ID-based references           | No  | Yes                           |
| String type names             | Yes | No                            |
| Use statements                | Yes | No                            |
| Comments                      | Yes | No                            |
| Parentheses/grouping          | Yes | No                            |

The IR intentionally omits:

- **Use statements**: already resolved during lowering
- **Comments**: purely syntactic, not needed for codegen
- **Parentheses/grouping**: expression structure is normalized
- **String type references**: all resolved to typed IDs

### Source Spans (DWARF / source-map / line-table)

Every IR shape carries an `IrSpan` field:

```rust
pub struct IrSpan {
    pub span: crate::location::Span,  // byte / line / column range
    pub file: FileId,                  // index into IrModule.file_table
}
```

`FileId(0)` is reserved for synthetic / hand-built nodes (closure-
converted lift wrappers, monomorphisation specialisations, test
fixtures). Real source files start at `FileId(1)` and live in
`IrModule.file_table: Vec<PathBuf>`. The lowerer registers files via
`IrModule.register_file(path)` which returns the assigned id.

Spans cover every data struct (`IrFunction`, `IrStruct`, `IrEnum`,
`IrEnumVariant`, `IrField`, `IrLet`, `IrTrait`, `IrFunctionSig`,
`IrFunctionParam`, `IrImpl`) and every `IrExpr` variant
(`Literal`, `Reference`, `FunctionCall`, `MethodCall`, `BinaryOp`,
`UnaryOp`, `If`, `For`, `Match`, `Block`, etc.).

Backends emit:

- **DWARF** `DW_TAG_subprogram` / `DW_AT_decl_file` / `.debug_line`
  by reading `IrFunction.span` + `IrModule.file_table`.
- **Source maps (v3)** by walking IR expressions, mapping each
  emitted instruction back to `IrSpan.span.start`.
- **JVM `LineNumberTable`** by mapping bytecode offsets to
  `IrFunctionSig.span.start.line`.

All `span` fields are `#[serde(default, skip_serializing_if =
"IrSpan::is_default")]`, so synthetic / round-tripped IR doesn't
bloat the serialised form.

Module nesting is flattened in the per-type vectors: a struct
inside `mod foo { ... }` is stored on `IrModule.structs` with a
qualified name `"foo::Bar"`. A parallel
`IrModule.modules: Vec<IrModuleNode>` tree mirrors the source `mod`
hierarchy with per-module ID lists for backends that need namespaced
output (see [IrModuleNode](module.md#irmodulenode--source-mod-hierarchy)).

## Relationship to the Symbol Table

The `SymbolTable` (built by the semantic analyzer) and the `IrModule`
(produced by IR lowering) carry overlapping definitions by design:

- `SymbolTable` keys everything by **name** and stores types as
  **strings** (e.g. `"User"`, `"[I32]?"`). It is the authoritative view
  for the validation passes and for LSP-style tooling that operates at
  the source level.
- `IrModule` keys everything by **typed IDs** (`StructId`, `TraitId`,
  `EnumId`, `FunctionId`, `ImplId`) and stores types as `ResolvedType`
  enums with embedded IDs. It is the authoritative view for code
  generators.

The two are built in sequence: the symbol table drives lowering, then
falls out of scope. Backends that need human-readable names can read
them from the IR directly; they never need to inspect the symbol table.
