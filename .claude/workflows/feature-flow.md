# Feature Development Flow

Complete workflow for implementing new features in FormaLang.

## Overview

```text
gitflow -> knowledge -> research -> audit* -> api-check -> test -> implement -> debug -> quality -> perf* -> knowledge -> gitflow
                                     ^                                                              ^
                                     * = if needed                                                  * = if needed
```

## Phase 1: Setup

**Agent**: gitflow

1. Create git worktree in `/tmp`:

   ```bash
   git worktree add /tmp/feature-<name> -b feature/<name>
   ```

2. Change to worktree directory
3. Confirm setup

## Phase 2: Knowledge Gathering

**Agent**: knowledge (retrieval mode)

1. Search codebase for related patterns
2. Find relevant documentation
3. Identify constraints and prior decisions
4. Produce context summary

## Phase 3: Requirements

**Agent**: research

1. Clarify goals with user
2. Define scope and constraints
3. Research approaches externally
4. Negotiate requirements with user
5. Produce requirements summary

**Checkpoint**: User confirms requirements.

## Phase 4: Dependency Audit (if needed)

**Agent**: audit

Only if new dependencies are proposed:

1. Check license compatibility
2. Verify maintenance status
3. Assess reputation
4. Report findings

**Checkpoint**: User approves dependencies.

## Phase 5: API Design

**Agent**: api-check

1. Propose FormaLang syntax changes (if any)
2. Design Rust public API
3. Validate cross-language compatibility
4. Check long-term viability
5. Assess compiler feasibility

**Checkpoint**: User approves API design.

## Phase 6: Test-First

**Agent**: test

1. Receive approved API specification
2. Write unit tests for public API
3. Write integration tests
4. Tests should fail (no implementation yet)

## Phase 7: Implementation

**Agent**: implement

1. Read specs and tests
2. Implement feature
3. Follow coding standards
4. Make tests pass
5. Document public APIs

## Phase 8: Validation

**Agents**: debug, quality, perf

Run in sequence:

1. **debug**: Run all tests
   - `cargo test --doc`
   - `cargo test --lib`
   - `cargo test --test '*'`
   - Markdown tests

2. **quality**: Check code quality
   - `cargo fmt --check`
   - `cargo clippy`
   - `markdownlint-cli2`
   - `cspell`

3. **perf** (if applicable): Run benchmarks

**All checks must pass.**

## Phase 9: Documentation

**Agent**: knowledge (writing mode)

1. Update relevant documentation
2. Add FormaLang examples
3. Update dates/status

## Phase 10: PR and Merge

**Agent**: gitflow

1. Commit with format: `feat(scope): description`
2. Create PR
3. Wait for user approval
4. Squash merge to main
5. Clean up worktree

---

## Checkpoints Summary

| Phase        | Requires User Approval |
| ------------ | ---------------------- |
| Requirements | Yes                    |
| Dependencies | Yes (if any)           |
| API Design   | Yes                    |
| PR Merge     | Yes                    |
