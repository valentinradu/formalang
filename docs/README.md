# FormaLang Documentation

**Last Updated**: 2026-04-14

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
