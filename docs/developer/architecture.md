# Architecture Overview

**Last Updated**: 2025-11-19

## Execution Model

FormaLang is a **compile-time code generator**. It compiles declarative `.forma` source files into native code at build time.

```text
Build time: .forma source → FormaLang compiler → Native code (.ts/.swift/.kt)
Runtime: App calls generated functions with data → Returns computed output
```

## Compiler Design

FormaLang is a Rust library designed to work in-process. The compiler outputs a validated AST as a Rust data structure.

### Compiler Phases

```text
Lexer → Parser → Semantic Analyzer → Validated AST
```

- **Lexer**: Tokenization
- **Parser**: Builds AST from tokens
- **Semantic Analyzer**: Validates AST
- **Output**: Validated AST (Rust data structure)

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
