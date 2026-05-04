# Compiler Pipeline

```text
Source → Lexer → Parser → Semantic Analyzer → IR Lowering → (Plugin System)
           │        │            │                 │               │
           ▼        ▼            ▼                 ▼               ▼
        Tokens     AST     Validated AST       IrModule      IrPass / Backend
```

- **Lexer**: Tokenizes source with `logos`.
- **Parser**: Builds AST from tokens with `chumsky` (Pratt precedence).
- **Semantic Analyzer**: 6-pass validation (Pass 0 resolves modules, Passes
  1–5 build symbol tables, resolve types, validate expressions, validate
  traits, detect cycles). Inference, validation, and the per-function
  scratch state (`local_let_bindings`, `inference_scope_stack`) operate
  on `SemType`, a structural type representation. There are no string
  sentinels (`"Unknown"`, `"InferredEnum"`, `"Nil"`) inside the analyzer;
  callers gate on `SemType::Unknown` / `is_indeterminate()` directly.
  The `SymbolTable::LetInfo::inferred_type` slot is the only string-typed
  storage that remains — it is the public boundary with IR lowering and
  external consumers (LSP queries, downstream tooling).
- **IR Lowering**: Converts the validated AST + symbol table into a
  fully type-resolved `IrModule`. Module nesting is **flattened in
  the per-type vectors**: inline `mod foo { struct Bar { ... } }`
  lowers to a top-level `IrStruct { name: "foo::Bar", ... }`, so
  backends that don't care about source structure see a flat list of
  definitions keyed by qualified name. A parallel `IrModule.modules:
  Vec<IrModuleNode>` tree mirrors the source `mod` hierarchy with
  per-module ID lists for backends that need namespaced output.
- **Plugin System**: External `IrPass` transforms and `Backend` emitters
  composed through `Pipeline`: see [Plugin System](plugins.md).

## Compiler Outputs

| Output | Type       | Use case                                   |
| ------ | ---------- | ------------------------------------------ |
| AST    | `File`     | Syntax analysis, source-level tooling, LSP |
| IR     | `IrModule` | Code generation, type-aware analysis       |

See the [AST Reference](../ast/overview.md) and [IR Reference](../ir/overview.md)
for the data shapes each phase produces.
