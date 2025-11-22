# Refactor Workflow

**Task**: $ARGUMENTS

You are refactoring existing code. No new features, no behavior changes.

## Phase 1: Setup (gitflow)

1. Create a git worktree in `/tmp` with a refactor branch:

   ```bash
   git worktree add /tmp/refactor-<short-name> -b refactor/<short-name>
   ```

2. Change working directory to the worktree

## Phase 2: Analysis (knowledge + research)

1. **Knowledge agent**: Understand current state
   - Find the code to refactor
   - Identify all usages and dependencies
   - Document current behavior

2. **Research agent** (if needed): Research better patterns
   - Only if refactoring toward a specific pattern

3. **Checkpoint**: Confirm refactoring scope with user
   - What changes
   - What stays the same
   - Expected benefits

## Phase 3: Test Verification (debug)

1. **Debug agent**: Run all existing tests
   - Establish baseline (all tests must pass)
   - Document current test coverage

## Phase 4: Refactor (implement)

1. **Implement agent**: Perform refactoring
   - No behavior changes
   - No new features
   - Follow coding standards
   - Preserve all existing functionality

## Phase 5: Validation (debug + quality + perf)

1. **Debug agent**: Run all tests
   - All tests must still pass
   - No regressions

2. **Quality agent**: Check code quality
   - `cargo fmt --check`
   - `cargo clippy`

3. **Perf agent** (if applicable): Verify no performance regression

**All checks must pass.**

## Phase 6: PR (gitflow)

1. Commit: `refactor(scope): description`
2. Create PR
3. **Checkpoint**: User approval
4. Squash merge
5. Clean up worktree

---

**Start now with Phase 1.**
