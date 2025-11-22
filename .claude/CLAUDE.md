# Claude Agent Guidelines

**Last Updated**: 2025-11-22
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
- **Markdown code tests are mandatory**

---

## FormaLang Compiler

### About

Forma is a declarative language compiler written in Rust.

### Architecture

- **Compiler phases**: Lexer -> Parser -> Semantic Analyser
- **Library design**: In-process, designed as a library
- **Output**: Validated AST as Rust data structure
- **Modular design**: Separate crates for distinct phases

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
