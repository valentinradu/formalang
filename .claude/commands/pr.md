# Pull Request Workflow

**Description**: $ARGUMENTS

You are preparing a pull request. Run all checks and create the PR.

## Phase 1: Pre-PR Validation

Run all checks in parallel where possible:

### Debug Agent: Run All Tests

```bash
cargo test --doc
cargo test --lib
cargo test --test '*'
# Markdown code block tests
```

**All tests must pass.**

### Quality Agent: Code Quality

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo check --all-targets --all-features
markdownlint-cli2 "**/*.md"
cspell "**/*.md"
```

**All checks must pass. VSCode must show zero errors.**

### Perf Agent: Benchmarks (if applicable)

If this is a performance-critical change:

```bash
cargo bench
```

**No performance regressions.**

## Phase 2: Pre-PR Checklist

Verify before proceeding:

- [ ] All tests pass
- [ ] All quality checks pass
- [ ] No performance regressions (if applicable)
- [ ] Documentation updated (if needed)
- [ ] New dependencies audited (if any)
- [ ] No secrets or credentials in code
- [ ] Commit messages follow format: `type(scope): description`

**Checkpoint**: Report all check results. If any fail, stop and report.

## Phase 3: Prepare Commits

1. Review staged changes: `git status` and `git diff`
2. Ensure logical commit structure
3. Commit message format: `type(scope): brief description`
   - Types: feat, fix, docs, refactor, test, chore
   - Max 72 characters
   - **No emojis**
   - **No Claude attribution**

## Phase 4: Create PR

1. Push branch to remote (if not already):

   ```bash
   git push -u origin <branch-name>
   ```

2. Create PR with `gh pr create`:

```bash
gh pr create --title "type(scope): description" --body "$(cat <<'EOF'
## Summary
[2-3 sentences: What and why]

## Changes
- [Key change 1]
- [Key change 2]
- [Key change 3]

## Testing
- [How it was tested]
- [Test results summary]

## Documentation
- Updated: yes/no
- [List docs changed if any]

## Checklist
- [ ] Tests pass
- [ ] Quality checks pass
- [ ] Docs updated
- [ ] Ready for review
EOF
)"
```

## Phase 5: User Review

Present PR summary to user:

- PR URL
- Summary of changes
- Test results
- Any concerns or notes

**Checkpoint**: User must approve before merge.

## Phase 6: Merge (with approval)

Only after explicit user approval:

1. Squash merge to main:

   ```bash
   gh pr merge --squash
   ```

2. Confirm merge completed

3. Clean up:

   ```bash
   git worktree remove <path>  # if using worktree
   git branch -d <branch>      # delete local branch
   ```

---

**Start now with Phase 1: Run all checks.**
