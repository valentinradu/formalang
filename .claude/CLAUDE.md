# Claude Agent Guidelines

**Last Updated**: 2026-04-14
**Status**: Active

## Quick Reference

- **Commands**: See [commands/](commands/) for workflow commands (`/feature`, `/fix`, `/docs`, `/pr`, etc.)
- **Agents**: See [agents/](agents/) for detailed agent definitions
- **Workflows**: See [workflows/](workflows/) for detailed workflow documentation

---

## General Principles

### Communication

- **Be concise**: Minimize token usage
- **Be precise**: Reference code as `[file.rs:line](path/to/file.rs#Lline)`
- **No emojis**: Never, anywhere

### File Operations

- **Prefer editing** over creating new files
- **No unnecessary files**: Don't create markdown/docs unless explicitly needed

### Git Rules

- **Never commit to main directly**
- **No Claude attribution** in commits, PRs, or code
- Commit format: `type(scope): description` (max 72 chars)
- Types: feat, fix, docs, refactor, test, chore

---

## Coding Standards

### Rust

- **Idiomatic Rust**: Pattern matching, Result types, iterators
- **Safety first**: No `unsafe` without explicit justification
- **Error handling**:
  - Proper error types, descriptive messages
  - No `unwrap`/`expect` in library code
  - **No hidden errors** unless 100% justified
  - All branches must be errorless or throw early
- **Documentation**: Public APIs must have doc comments with examples
- **Comments**: Only where non-obvious

### Dependencies

- **Minimize** for common tasks (prefer std lib)
- **Use ecosystem** for complex problems
- **All dependencies must pass audit** (see audit agent)

### Testing

- Tests in `tests/` for integration
- Tests in `src/` with `#[cfg(test)]` for unit
- All public APIs need tests
- ```rust``` examples in doc comments are exercised by `cargo test --doc`

---

## FormaLang Compiler

### About

Forma is a declarative language compiler written in Rust. It is a pure
compiler frontend library — parsing, semantic analysis, and IR lowering
are built-in; code generation is the responsibility of embedders via the
plugin system.

### Architecture

- **Compiler phases**: Lexer -> Parser -> Semantic Analyser -> IR Lowering -> Plugin System
- **Library design**: In-process, single-crate library
- **Output**: `IrModule` (type-resolved IR) via `compile_to_ir`; raw `File` (AST) via `compile`
- **Plugin system**: `IrPass` for IR transforms; `Backend` for code generation; composed with `Pipeline`
- **Built-in passes**: `DeadCodeEliminationPass`, `ConstantFoldingPass` in `formalang::ir`

### Module Resolution

- **Always import via `use`**: Never append or concatenate source files
- **Use `FileSystemResolver`**: Resolve imports from disk via `compile_with_resolver`
- **No `include_str!`**: Never splice one module's source into another
- **Impl block defaults**: Importing a struct/enum also imports its impl block
- **Test setup**: Use `PathBuf::from(".")` (or a `tempfile::tempdir`) as resolver root

### Code Formatting

- Use ```formalang``` for FormaLang code blocks
- Use ```rust``` for Rust examples

### Documentation Quality

- `markdownlint-cli2`: Zero errors required
- `cspell`: Zero spelling errors required
- Custom words in `.cspell.json`

---

## Agents

| Agent     | Purpose                                              | File                                       |
| --------- | ---------------------------------------------------- | ------------------------------------------ |
| knowledge | Retrieve and maintain documentation                  | [agents/knowledge.md](agents/knowledge.md) |
| research  | Requirements gathering and technical research        | [agents/research.md](agents/research.md)   |
| api-check | API design, validation, cross-language compatibility | [agents/api-check.md](agents/api-check.md) |
| test      | Write tests (black-box, no implementation access)    | [agents/test.md](agents/test.md)           |
| implement | Feature implementation                               | [agents/implement.md](agents/implement.md) |
| debug     | Run tests and analyze failures                       | [agents/debug.md](agents/debug.md)         |
| quality   | Static analysis and quality checks                   | [agents/quality.md](agents/quality.md)     |
| perf      | Performance benchmarking                             | [agents/perf.md](agents/perf.md)           |
| audit     | Dependency license and maintenance checks            | [agents/audit.md](agents/audit.md)         |
| gitflow   | Git workflow and version control                     | [agents/gitflow.md](agents/gitflow.md)     |

---

## Feature Workflow (Summary)

```text
gitflow -> knowledge -> research -> audit* -> api-check -> test -> implement -> debug -> quality -> perf* -> knowledge -> gitflow
```

See [workflows/feature-flow.md](workflows/feature-flow.md) for details.

### User Checkpoints

- Requirements confirmation
- Dependency approval (if any)
- API design approval
- PR merge approval

---

## Quality Gates

Before any PR:

- All tests pass
- `cargo fmt --check` passes
- `cargo clippy` passes
- `markdownlint-cli2` passes
- `cspell` passes
- VSCode shows zero errors
