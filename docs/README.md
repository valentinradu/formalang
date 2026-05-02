# FormaLang Documentation

**Last Updated**: 2026-05-02

FormaLang is a declarative DSL compiler frontend written in Rust. It parses
`.fv` source files, performs semantic analysis, and produces a type-resolved
Intermediate Representation (IR). Code generation is handled by external
backends via the plugin system.

## For Users

The [User Guide](user/core.md) covers FormaLang syntax and features —
core constructs, type system, definitions, expressions, control flow,
generics, modules.

## For Developers

- [Architecture Overview](developer/architecture/design.md) — system
  design, compiler pipeline, plugin system, built-in passes
- [AST Reference](developer/ast/overview.md) — Abstract Syntax Tree for
  tooling
- [IR Reference](developer/ir/overview.md) — Intermediate Representation
  for code generation

## Design Notes

Open / forthcoming features carry their own status pages:

- [Cross-Module Code Generation](developer/cross-module-codegen.md)
- [Default Parameter Values](developer/default-parameters.md)
- [String Built-In Methods](developer/string-builtins.md)
