# Architecture Overview

**Last Updated**: 2026-04-14

## Design

FormaLang is a **pure compiler frontend library** written in Rust. It
parses `.fv` source files, validates them, and produces an Intermediate
Representation (IR). Code generation is **not built in** — backends are
external and plug in via the `IrPass`/`Backend` trait system.

```text
.fv source → FormaLang library → IrModule → [your Backend] → output
```

## Compiler Phases

```text
Source → Lexer → Parser → Semantic Analyzer → IR Lowering → (Plugin System)
           │        │            │                 │               │
           ▼        ▼            ▼                 ▼               ▼
        Tokens     AST     Validated AST       IrModule      IrPass / Backend
```

- **Lexer**: Tokenizes source with `logos`
- **Parser**: Builds AST from tokens with `chumsky` (Pratt precedence)
- **Semantic Analyzer**: 6-pass validation; builds symbol table, resolves
  types, validates traits, detects cycles
- **IR Lowering**: Converts the validated AST + symbol table into a
  fully type-resolved `IrModule`
- **Plugin System**: External `IrPass` transforms and `Backend` emitters
  composed through `Pipeline`

## Public API Entry Points

Defined in `src/lib.rs`:

| Function | Returns | Use case |
|---|---|---|
| `compile_to_ir(src)` | `Result<IrModule, Vec<CompilerError>>` | Code generation |
| `compile(src)` | `Result<File, Vec<CompilerError>>` | Syntax analysis, tooling |
| `compile_with_analyzer(src)` | `Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>>` | LSP |
| `parse_only(src)` | `Result<File, ...>` | Parsing without validation |

## Plugin System

Defined in `src/pipeline.rs`:

```text
IrModule → [IrPass, IrPass, ...] → IrModule → Backend → Output
```

- **`IrPass`**: Takes ownership of `IrModule`, transforms it, returns
  a new `IrModule`. Use for optimization, specialization, or lowering.
- **`Backend`**: Borrows an `IrModule`, produces any `Output` type.
  Use for code generation.
- **`Pipeline`**: Chains passes with `.pass(...)` and drives a backend
  with `.emit(module, &backend)`.

Built-in passes exported from `formalang::ir`:

- `DeadCodeEliminationPass` — removes unreachable definitions
- `ConstantFoldingPass` — evaluates constant expressions at compile time

## Compiler Outputs

| Output | Type | Use case |
|---|---|---|
| AST | `File` | Syntax analysis, source-level tooling, LSP |
| IR | `IrModule` | Code generation, type-aware analysis |

See [AST Reference](ast.md) and [IR Reference](ir.md) for details.

## Single-Crate Design

The compiler is a single Rust crate (`formalang`). All phases share
types directly — no IPC, serialization, or process boundaries.

## Logic Model

FormaLang logic is pure and declarative:

- Conditionals (`if`/`else`, optional unwrapping)
- Iteration (`for` over arrays)
- Pattern matching (`match` on enums)
- Struct/enum/trait definitions with generics and constraints

The IR carries all possible state resolved at compile time. Given
runtime data, a backend computes the current state in the target language.

## Use Cases

- **Design systems and design tokens**: define shared types/values once,
  generate platform-specific code per target
- **Cross-platform type generation**: emit TypeScript, Swift, Kotlin, or
  any other language from a single `.fv` schema
- **LSP tooling**: hover, completion, go-to-definition via `compile_with_analyzer`
- **Static analysis and linting**: traverse the AST or IR with the visitor pattern
