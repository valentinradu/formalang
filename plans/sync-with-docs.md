# Plan: Sync FormaLang Compiler with Documentation

**Status**: In Progress
**Branch**: feature/sync-with-docs
**Created**: 2025-11-22

---

## Phase 1: Setup

- [x] Pull latest main and create worktree at /tmp/feature-sync-with-docs

## Phase 2: Remove Legacy Features

- [x] Remove Token::Mounting from lexer/token.rs
- [x] Remove Token::Context from lexer/token.rs (legacy context keyword)
- [x] Remove ContextExpr from ast/mod.rs
- [x] Remove ContextItem from ast/mod.rs
- [x] Remove associated parser logic
- [x] Remove associated semantic logic

Note: Token::Question was kept - it's used for optional type syntax (T?)

## Phase 3: Never Type

- [x] Write tests for Never type parsing
- [x] Add Never to PrimitiveType enum
- [x] Update lexer (Token::NeverType)
- [x] Update parser
- [x] Update semantic analyzer (type_to_string)

## Phase 4: Dictionary Types

- [ ] Write tests for `[K: V]` type parsing
- [ ] Write tests for dictionary literal parsing
- [ ] Write tests for dictionary access parsing
- [ ] Write tests for dictionary semantic validation
- [ ] Add Dictionary to Type enum
- [ ] Add DictLiteral to Expr enum
- [ ] Add DictAccess to Expr enum
- [ ] Implement parser rules
- [ ] Implement semantic validation

## Phase 5: Closure Types and Expressions

- [ ] Write tests for closure type parsing (`T -> U`, `() -> T`)
- [ ] Write tests for closure expression parsing
- [ ] Write tests for closure semantic validation
- [ ] Add Closure to Type enum
- [ ] Add ClosureExpr to Expr enum
- [ ] Implement parser rules
- [ ] Implement semantic validation

## Phase 6: Let Expressions (block-scoped)

- [ ] Write tests for let in blocks
- [ ] Write tests for let scoping rules
- [ ] Add LetExpr to Expr enum
- [ ] Update parser for block-local let
- [ ] Implement semantic scoping

## Phase 7: Destructuring Patterns

- [ ] Write tests for array destructuring
- [ ] Write tests for struct destructuring
- [ ] Write tests for enum destructuring
- [ ] Add ArrayPattern to Pattern enum
- [ ] Add StructPattern to Pattern enum
- [ ] Add EnumPattern to Pattern enum
- [ ] Update parser
- [ ] Update semantic validation

## Phase 8: Validation

- [ ] All tests pass (`cargo test`)
- [ ] 80% code coverage verified
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy` passes
- [ ] `markdownlint-cli2` passes
- [ ] `cspell` passes

## Phase 9: PR

- [ ] Commit with message: `feat(lang): sync compiler with documentation`
- [ ] Create PR
- [ ] User approval
- [ ] Merge to main
- [ ] Clean up worktree

---

## Notes

- 2025-11-22: Phase 2 complete. Removed legacy context system (ContextExpr, ContextItem, Token::Context, Token::Mounting). Kept provides/consumes system which is documented.
- Need to create Cargo.toml before we can build/test.
