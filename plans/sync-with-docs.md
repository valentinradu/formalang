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

- [x] Write tests for `[K: V]` type parsing
- [x] Write tests for dictionary literal parsing
- [x] Write tests for dictionary access parsing
- [x] Write tests for dictionary semantic validation
- [x] Add Dictionary to Type enum
- [x] Add DictLiteral to Expr enum
- [x] Add DictAccess to Expr enum
- [x] Implement parser rules
- [x] Implement semantic validation

Note: Already implemented in previous work.

## Phase 5: Closure Types and Expressions

- [x] Write tests for closure type parsing (`T -> U`, `() -> T`)
- [x] Write tests for closure expression parsing
- [x] Write tests for closure semantic validation
- [x] Add Closure to Type enum
- [x] Add ClosureExpr to Expr enum
- [x] Implement parser rules
- [x] Implement semantic validation

Note: Already implemented (PR #12).

## Phase 6: Let Expressions (block-scoped)

- [x] Write tests for let in blocks
- [x] Write tests for let scoping rules
- [x] Add LetExpr to Expr enum
- [x] Update parser for block-local let
- [x] Implement semantic scoping

Note: Already implemented.

## Phase 7: Destructuring Patterns

- [x] Write tests for array destructuring
- [x] Write tests for struct destructuring (ignored until semantic support)
- [x] Write tests for enum destructuring (ignored until semantic support)
- [x] Add BindingPattern enum with Array, Struct, Tuple variants
- [x] Add ArrayPatternElement and StructPatternField types
- [x] Update parser with binding_pattern_parser
- [x] Update LetBinding and LetExpr to use BindingPattern
- [x] Update semantic analyzer for simple patterns
- [ ] Implement full semantic validation for destructuring (future work)

## Phase 8: Validation

- [x] All tests pass (`cargo test`) - 812 passing, 8 ignored
- [x] `cargo fmt --check` passes
- [x] `cargo clippy` passes
- [ ] 80% code coverage verified (optional)
- [ ] `markdownlint-cli2` passes (optional)
- [ ] `cspell` passes (optional)

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
