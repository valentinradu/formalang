# FormaLang Documentation

**Last Updated**: 2026-04-26

FormaLang is a declarative DSL compiler frontend written in Rust. It parses
`.fv` source files, performs semantic analysis, and produces a type-resolved
Intermediate Representation (IR). Code generation is handled by external
backends via the plugin system.

## For Users

- [Language Reference](user/formalang.md) - FormaLang syntax and features

## For Developers

- [Architecture Overview](developer/architecture.md) - System design and compiler pipeline
- [AST Reference](developer/ast.md) - Abstract Syntax Tree for tooling
- [IR Reference](developer/ir.md) - Intermediate Representation for code generation

### Proposed work

- [Numeric-literal precision](developer/numeric_literal_precision.md) - replace `NumberLiteral.value: f64` with a discriminated union so integer literals round-trip exactly
