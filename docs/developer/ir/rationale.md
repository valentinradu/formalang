# Design Rationale

The IR design follows patterns from the Rust compiler (HIR / THIR / MIR):
the AST is preserved for source-level tooling, and a separate, type-
resolved, ID-keyed representation is used for code generation.

## Why Separate from AST?

- **Clean separation**: AST preserves source fidelity, IR optimises for
  codegen.
- **No syntax noise**: IR omits spans, comments, use statements,
  parentheses, grouping.
- **Different consumers**: Linters and LSPs use AST, code generators use IR.

## Why ID-Based References?

- **Copyable**: IDs are `Copy`, no lifetime complexity.
- **Cheap**: O(1) `Vec` lookup by index.
- **Type-safe**: `StructId` cannot be used where a `TraitId` is expected.
- **Stable**: IDs don't change when other definitions are added.

## Why Type on Every Expression?

- **No re-inference**: Code generators don't need to re-derive types.
- **Single source of truth**: Type is computed once during lowering.
- **Simpler codegen**: Just read `expr.ty()` and emit appropriate code.

## Why Visitor Pattern?

- **Selective processing**: Implement only the methods you need.
- **Controlled traversal**: Producer decides traversal order.
- **Extensible**: New node types don't break existing visitors.

## See Also

- [AST Reference](../ast/overview.md): for syntax analysis and
  source-level tooling
- [Architecture Overview](../architecture/design.md): overall compiler
  design
- [Built-in Passes](../architecture/passes.md):
  `MonomorphisePass`, `ResolveReferencesPass`, `DeadCodeEliminationPass`,
  `ConstantFoldingPass`
