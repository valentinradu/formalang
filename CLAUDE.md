# FormaLang — Agent Guidelines

**Status**: Active

## Project stage

**FormaLang is pre-1.0 and very early in its lifecycle.** Do not preserve
backwards compatibility for its own sake. When choosing between "this fix is
correct but would break some currently-compiling programs" and "leave the
silent wrongness in place for compat", pick the fix.

Concretely, when doing correctness work:

- Prefer hard errors (`CompilerError::InternalError`, `TooManyDefinitions`,
  etc.) over silent placeholder types or magic-string sentinels. The
  `"Unknown"` string sentinel and the `ResolvedType::TypeParam("Unknown")`
  shape were both retired in favour of structural variants
  (`ResolvedType::Error` for IR, `SemType::Unknown` plus
  `is_indeterminate()` for semantic). New code should match
  structurally, not on stringified names. Empty enum variants and the
  trait-id sentinel `0` are still banned as placeholders.
- Surface gaps loudly instead of masking them with a "documented design
  choice" comment. If a branch is unreachable under a real invariant, make
  that explicit; if it *is* reachable but produces nonsense, raise an error.
- Update the test suite alongside the change — tests are not load-bearing
  contracts at this stage.
- The only compat concerns that still apply are serialised `IrModule` /
  `File` JSON on disk (external consumers) and the public API surface
  advertised via `lib.rs` re-exports. Both can move, but warrant a note in
  the commit message and a bumped version.

This guidance takes precedence over anything else in this file that reads as
"don't break existing behaviour".

---

## Quick Reference

- **Commands**: See [.claude/commands/](.claude/commands/) for workflow
  commands (`/feature`, `/fix`, `/docs`, `/pr`, etc.)
- **Agents**: See [.claude/agents/](.claude/agents/) for detailed agent
  definitions
- **Workflows**: See [.claude/workflows/](.claude/workflows/) for detailed
  workflow documentation

---

## General Principles

### Communication

- **Be concise**: Minimize token usage
- **Be precise**: Reference code as `[file.rs:line](path/to/file.rs#Lline)`
- **No emojis**: Never, anywhere

### File Operations

- **Prefer editing** over creating new files
- **No unnecessary files**: Don't create markdown/docs unless explicitly
  needed

### Git Rules

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

- Tests in `tests/suite/` for integration. The suite is one binary: add
  a file there, and a `mod <name>;` line to `tests/suite/main.rs`
- Tests in `src/` with `#[cfg(test)]` for unit
- All public APIs need tests
- ```rust``` examples in doc comments are exercised by `cargo test --doc`

See [TESTING.md](TESTING.md) for the full taxonomy. Beyond the
example-based suites there are five surfaces:

| Surface | Command |
| ------- | ------- |
| Metamorphic, property and conformance tests | `cargo test` |
| Open defects and unfinished features | `cargo test --no-fail-fast -- --ignored` |
| Differential testing against an oracle | `PROPTEST_CASES=N cargo test --release --test suite differential::` |
| Benchmarks and scaling | `cargo bench` |
| Fuzzing (needs nightly) | `scripts/fuzz.sh` |
| Loom model check | `RUSTFLAGS="--cfg loom" cargo test --bin fvc loom_watch` |

To add a language rule, add a file to `tests/conformance/` — one rule
per file, with a `// expect:` header. No Rust required.

An `#[ignore]` test fails today; its reason says why. Drop the
attribute when the defect is fixed or the feature lands.

### The toolchain

`rust-toolchain.toml` pins the exact compiler that CI and a developer
machine both use. Do not run `rustup update` to fix a lint failure:
that moves your machine off the pin and hides the problem instead of
fixing it. To move the pin, raise `channel` in that file, run the
clippy gate, and fix the new lints in the same commit.

The pin is not the minimum supported Rust version. That is
`rust-version` in `Cargo.toml`.

---

## FormaLang Compiler

### About

Forma is a declarative language compiler written in Rust. It is a pure
compiler frontend library — parsing, semantic analysis, and IR lowering are
built-in; code generation is the responsibility of embedders via the plugin
system.

### Architecture

- **Compiler phases**: Lexer -> Parser -> Semantic Analyser -> IR Lowering ->
  Plugin System
- **Library design**: In-process, single-crate library
- **Output**: `IrModule` (type-resolved IR) via `compile_to_ir`; raw `File`
  (AST) via `parse_only`
- **Plugin system**: `IrPass` for IR transforms; `Backend` for code
  generation; composed with `Pipeline`
- **Built-in passes**: `DeadCodeEliminationPass`, `ConstantFoldingPass`,
  `MonomorphisePass` in `formalang::ir`

### Module Resolution

- **Always import via `use`**: Never append or concatenate source files
- **Use `FileSystemResolver`**: Resolve imports from disk via
  `compile_with_resolver`
- **No `include_str!`**: Never splice one module's source into another
- **Impl block defaults**: Importing a struct/enum also imports its impl
  block
- **Test setup**: Use `PathBuf::from(".")` (or a `tempfile::tempdir`) as
  resolver root

### Code Formatting

- Use ```formalang``` for FormaLang code blocks
- Use ```rust``` for Rust examples

### Documentation Quality

- `markdownlint-cli2`: Zero errors required
- `cspell`: Zero spelling errors required
- Custom words in `.cspell.json`

---

## Agents

| Agent     | Purpose                                              | File                                                       |
| --------- | ---------------------------------------------------- | ---------------------------------------------------------- |
| knowledge | Retrieve and maintain documentation                  | [.claude/agents/knowledge.md](.claude/agents/knowledge.md) |
| research  | Requirements gathering and technical research        | [.claude/agents/research.md](.claude/agents/research.md)   |
| api-check | API design, validation, cross-language compatibility | [.claude/agents/api-check.md](.claude/agents/api-check.md) |
| test      | Write tests (black-box, no implementation access)    | [.claude/agents/test.md](.claude/agents/test.md)           |
| implement | Feature implementation                               | [.claude/agents/implement.md](.claude/agents/implement.md) |
| debug     | Run tests and analyze failures                       | [.claude/agents/debug.md](.claude/agents/debug.md)         |
| quality   | Static analysis and quality checks                   | [.claude/agents/quality.md](.claude/agents/quality.md)     |
| perf      | Performance benchmarking                             | [.claude/agents/perf.md](.claude/agents/perf.md)           |
| audit     | Dependency license and maintenance checks            | [.claude/agents/audit.md](.claude/agents/audit.md)         |
| gitflow   | Git workflow and version control                     | [.claude/agents/gitflow.md](.claude/agents/gitflow.md)     |

---

## Feature Workflow (Summary)

```text
gitflow -> knowledge -> research -> audit* -> api-check -> test -> implement -> debug -> quality -> perf* -> knowledge -> gitflow
```

See [.claude/workflows/feature-flow.md](.claude/workflows/feature-flow.md)
for details.

### User Checkpoints

- Requirements confirmation
- Dependency approval (if any)
- API design approval
- PR merge approval

---

## Quality Gates

Before any PR:

- `cargo test` passes (`#[ignore]` tests describe unfinished
  features and are excluded by design)
- `cargo fmt --check` passes
- `cargo clippy` passes
- `cargo audit` passes (install with `cargo install cargo-audit --locked`
  if not present; CI runs it via `rustsec/audit-check`)
- `markdownlint-cli2` passes
- `cspell` passes
- VSCode shows zero errors
