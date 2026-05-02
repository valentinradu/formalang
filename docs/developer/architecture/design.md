# Design

FormaLang is a **pure compiler frontend library** written in Rust. It
parses `.fv` source files, validates them, and produces an Intermediate
Representation (IR). Code generation is **not built in** — backends are
external and plug in via the `IrPass`/`Backend` trait system.

```text
.fv source → FormaLang library → IrModule → [your Backend] → output
```

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
