# Architecture Overview

**Last Updated**: 2025-11-23

## Execution Model

FormaLang is a **compile-time code generator**. It compiles declarative `.forma` source files into native code at build time.

```text
Build time: .forma source → FormaLang compiler → Native code (.ts/.swift/.kt)
Runtime: App calls generated functions with data → Returns computed output
```

## Compiler Design

FormaLang is a Rust library designed to work in-process. The compiler produces an IR (Intermediate Representation) optimized for code generation.

### Compiler Phases

```text
Source → Lexer → Parser → Semantic Analyzer → IR Lowering → Code Generation
           │        │            │                 │              │
           ▼        ▼            ▼                 ▼              ▼
        Tokens     AST     Validated AST       IrModule      TS/Swift/Kotlin
```

- **Lexer**: Tokenizes source into tokens
- **Parser**: Builds AST from tokens
- **Semantic Analyzer**: Validates AST, builds symbol table
- **IR Lowering**: Converts AST + symbols into IR with resolved types
- **Code Generation**: Produces target language code from IR

### Compiler Outputs

| Output | Use Case                                         |
|--------|--------------------------------------------------|
| AST    | Syntax analysis, tooling, source-level transforms|
| IR     | Code generation, type-aware analysis             |

See [AST Reference](ast.md) and [IR Reference](ir.md) for details.

### Modular Design

Separate crates for distinct compiler phases, designed for library consumption by other Rust code.

## Code Generation

### Target Languages

- TypeScript
- Swift
- Kotlin

### Generated Code Characteristics

- **Pure functions**: No side effects
- **Zero dependencies**: No runtime libraries required
- **Type-safe**: Full type safety in target language

## Logic Model

FormaLang logic is pure and basic:

- Conditionals (if/else)
- Transforms
- Constraints

The language AST contains all possible state. Given runtime data, the generated code computes the current state.

## Distribution

- **TypeScript**: npm
- **Swift**: Swift Package Manager (SPM)
- **Kotlin**: Gradle

## Use Cases

Primary use case:

- Design systems and design tokens
- Structure/template generation with runtime data

Other use cases:

- Dynamic templating with runtime data
- Constraint-based computations
- Cross-platform code generation
