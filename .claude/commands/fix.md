# Bug Fix Workflow

**Task**: $ARGUMENTS

You are fixing a bug. Follow this workflow.

## Phase 1: Setup (gitflow)

1. Create a git worktree in `/tmp` with a fix branch:

   ```bash
   git worktree add /tmp/fix-<short-name> -b fix/<short-name>
   ```

2. Change working directory to the worktree

## Phase 2: Investigation (knowledge + debug)

1. **Knowledge agent**: Gather context
   - Find related code, docs, issues
   - Understand the expected behavior

2. **Debug agent**: Reproduce and analyze
   - Run existing tests to confirm failure
   - Identify root cause
   - Report findings with file:line references

3. **Checkpoint**: Confirm understanding of the bug with user

## Phase 3: Test (test)

1. **Test agent**: Write a failing test that reproduces the bug
   - Test should fail before fix
   - Test should pass after fix

## Phase 4: Fix (implement)

1. **Implement agent**: Fix the bug
   - Minimal changes
   - No scope creep
   - Follow coding standards

## Phase 5: Validation (debug + quality)

1. **Debug agent**: Run all tests (must pass)
2. **Quality agent**: Check code quality

**All checks must pass.**

## Phase 6: PR (gitflow)

1. Commit: `fix(scope): description`
2. Create PR
3. **Checkpoint**: User approval
4. Squash merge
5. Clean up worktree

---

**Start now with Phase 1.**
