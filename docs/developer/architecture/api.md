# Public API

The crate root (`src/lib.rs`) exposes four entry points covering the
canonical compile path, LSP/AST tooling, and CLI-friendly error reporting:

| Function                     | Returns                                                                         | Use case                   |
| ---------------------------- | ------------------------------------------------------------------------------- | -------------------------- |
| `compile_to_ir(src)`         | `Result<IrModule, Vec<CompilerError>>`                                          | Code generation (canonical) |
| `compile_with_analyzer(src)` | `Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>>`      | LSP, AST-level tooling     |
| `parse_only(src)`            | `Result<File, ...>`                                                             | Parsing without validation |
| `compile_and_report(src, f)` | `Result<IrModule, String>`                                                      | CLI: compile + formatted errors |

All four read source as a `&str`; there is no I/O inside the library.
For multi-file projects, pair `compile_to_ir_with_resolver` with a
`FileSystemResolver` (or a custom `ModuleResolver` impl) to load
`.fv` files from anywhere.
