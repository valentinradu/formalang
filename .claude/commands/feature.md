# Feature Workflow

**Task**: $ARGUMENTS

You are starting a new feature implementation. Follow this workflow strictly.

## Phase 1: Setup (gitflow)

1. Pull latest main from origin:

   ```bash
   git fetch origin && git pull origin main
   ```

2. Create a git worktree in `/tmp` with a feature branch:

   ```bash
   git worktree add /tmp/feature-<short-name> -b feature/<short-name>
   ```

3. Change working directory to the worktree
4. Confirm setup complete before proceeding

## Phase 2: Knowledge Gathering (knowledge + research)

Use FormaLens semantic search and ast-grep for code searches (see CLAUDE.md).

1. **Knowledge agent**: Retrieve relevant context
   - Search existing docs, RFCs, code patterns
   - Identify related features and constraints
   - Summarize findings

2. **Research agent**: Deep analysis + requirements
   - Analyze the task requirements
   - Research approaches if needed (web, similar projects)
   - **Negotiate with user** to define clear requirements
   - Produce a requirements summary

**Checkpoint**: User must confirm requirements before proceeding.

## Phase 3: Dependency Audit (audit) - If Needed

If new dependencies are proposed:

1. **Audit agent**: Check license, maintenance, reputation
2. Report findings
3. **Checkpoint**: User approval for any new dependencies

## Phase 4: API Design (api-check)

1. **API agent**: Propose public API changes
   - Design FormaLang syntax changes (if any)
   - Design Rust public API changes
   - Validate cross-language compatibility (Swift/TS/Rust/Kotlin)
   - Check compiler feasibility
2. Present API design
3. **Checkpoint**: User must approve API design

## Phase 5: Test-First Development (test)

1. **Test agent**: Write tests based on approved API
   - Unit tests for public API
   - Integration tests for feature behavior
   - **Does NOT look at implementation code**
   - Tests should fail initially (no implementation yet)
2. Confirm tests are written

## Phase 6: Implementation (implement)

1. **Implement agent**: Write the implementation
   - Follow coding standards
   - No hidden errors
   - Make tests pass
2. Verify code compiles

## Phase 7: Validation (debug + quality + perf)

1. **Debug agent**: Run all tests
   - `cargo test --doc`
   - `cargo test --lib`
   - `cargo test --test '*'`
   - Markdown code block tests
   - Report any failures

2. **Quality agent**: Check code quality
   - `cargo fmt --check`
   - `cargo clippy`
   - `markdownlint-cli2`
   - `cspell`

3. **Perf agent** (if applicable): Run benchmarks
   - Only for performance-critical features

**All checks must pass before proceeding.**

## Phase 8: Documentation (knowledge)

1. **Knowledge agent**: Update documentation
   - Update relevant docs
   - Add FormaLang examples (no Rust implementation code)
   - Update CLAUDE.md if needed

## Phase 9: PR and Merge (gitflow)

1. Commit with proper message format: `feat(scope): description`
2. Create PR with:
   - Summary
   - Changes list
   - Testing section
   - Docs updated: yes/no
3. **Checkpoint**: User approval for merge
4. Squash merge to main
5. Clean up worktree

---

**Start now with Phase 1.**
