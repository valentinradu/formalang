# Common Rules for All Agents

Rules that apply to every agent in the FormaLang project.

## General Principles

### Communication

- **Be concise**: Minimize token usage
- **Be precise**: Reference code as `[file.rs:line](path/to/file.rs#Lline)`
- **No emojis**: Never, anywhere

### Step Discipline

- **Never skip steps** without explicit justification + user confirmation
- Complete workflows in order
- Report blockers immediately

### Boundaries

- Stay within your agent's domain
- Hand off to appropriate agents
- Don't operate outside your specialization

## Git Rules

- **Never commit to main directly**
- **No Claude attribution** in commits, PRs, or code
- Commit format: `type(scope): description`
- Types: feat, fix, docs, refactor, test, chore
- Max 72 characters in commit title

## Code Standards

### Rust

- Idiomatic Rust: pattern matching, Result types, iterators
- No `unsafe` without justification
- No `unwrap`/`expect` in library code
- Proper error handling with descriptive messages
- No hidden errors unless 100% justified

### Documentation

- Code blocks: ```formalang``` for FormaLang, ```rust``` for Rust
- Only FormaLang examples in docs (no Rust implementation code)
- Update dates and status fields
- Single source of truth (no duplicates)

### Testing

- Tests in `tests/` for integration
- Tests in `src/` with `#[cfg(test)]` for unit
- All public APIs need tests
- Markdown code tests are mandatory

## Quality Gates

Before any PR:

- All tests pass
- `cargo fmt --check` passes
- `cargo clippy` passes
- `markdownlint-cli2` passes (zero errors)
- `cspell` passes (zero errors)
- VSCode shows zero errors

## Collaboration

### Agent Hand-offs

| From      | To        | When                                      |
| --------- | --------- | ----------------------------------------- |
| knowledge | research  | Context gathered, needs external research |
| research  | api-check | Requirements defined, needs API design    |
| api-check | test      | API approved, needs tests                 |
| test      | implement | Tests written, needs implementation       |
| implement | debug     | Code written, needs testing               |
| debug     | quality   | Tests pass, needs quality check           |
| quality   | knowledge | Quality passes, needs docs                |
| knowledge | gitflow   | Docs updated, ready for PR                |

### User Checkpoints

Always get user approval for:

- Requirements confirmation
- Dependency additions
- API design approval
- PR merge
