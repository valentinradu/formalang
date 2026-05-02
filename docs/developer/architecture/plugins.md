# Plugin System

Defined in `src/pipeline.rs`:

```text
IrModule → [IrPass, IrPass, ...] → IrModule → Backend → Output
```

- **`IrPass`**: Takes ownership of `IrModule`, transforms it, returns
  `Result<IrModule, Vec<CompilerError>>`. Use for optimization, specialization,
  or lowering. A failing pass aborts the pipeline and returns its errors.
- **`Backend`**: Borrows an `IrModule`, produces any `Output` type.
  Use for code generation.
- **`Pipeline`**: Chains passes with `.pass(...)` and drives a backend
  with `.emit(module, &backend)`.

For a tour of the passes shipped in the box, see [Built-in Passes](passes.md).
