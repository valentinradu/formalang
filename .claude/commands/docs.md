# Documentation Workflow

**Task**: $ARGUMENTS

You are making documentation changes only. Follow this workflow.

## Phase 1: Setup (gitflow)

1. Create a git worktree in `/tmp` with a docs branch:

   ```bash
   git worktree add /tmp/docs-<short-name> -b docs/<short-name>
   ```

2. Change working directory to the worktree

## Phase 2: Research (knowledge + research)

1. **Knowledge agent**: Review existing documentation
   - Find related docs
   - Check for duplicates
   - Identify single source of truth

2. **Research agent** (if needed): Gather external information
   - Only if documenting external concepts

## Phase 3: Write (knowledge)

1. **Knowledge agent**: Make documentation changes
   - Follow single source of truth principle
   - Use proper formatting (```formalang```, ```rust```)
   - Only FormaLang examples, no Rust implementation suggestions
   - Update dates and status fields
   - Use references/links instead of duplicating

## Phase 4: Validation (quality)

1. **Quality agent**: Check documentation quality
   - `markdownlint-cli2` (zero errors)
   - `cspell` (zero spelling errors)

**All checks must pass.**

## Phase 5: PR (gitflow)

1. Commit: `docs(scope): description`
2. Create PR
3. **Checkpoint**: User approval
4. Squash merge
5. Clean up worktree

---

**Start now with Phase 1.**
