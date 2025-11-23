# Plan: Bring Semantic Analyzer Up to Date

**Status**: In Progress
**Branch**: feature/semantic-update
**Created**: 2025-11-23

---

## Overview

Complete semantic validation for destructuring patterns. Currently, the parser
supports array, struct, and tuple destructuring patterns, but the semantic
analyzer only registers simple bindings. This plan adds full semantic support
to un-ignore 8 tests.

## Phase 1: Setup

- [x] Create worktree at /tmp/feature-semantic-update
- [x] Remove completed sync-with-docs.md plan
- [x] Create this plan file

## Phase 2: Core Semantic Support for All Destructuring Patterns

Added `collect_bindings_from_pattern` helper function that recursively extracts
all binding names from any pattern type (Simple, Array, Struct, Tuple).

Tasks:
- [x] Add `PatternBinding` struct and `collect_bindings_from_pattern` helper
- [x] Update `build_symbol_table` (both methods) to use helper
- [x] Update `infer_let_types` to use helper
- [x] Update `validate_expr` for LetExpr to use helper
- [x] Update `detect_circular_let_dependencies` to use helper
- [x] Update `is_let_mutable` helper to use pattern bindings
- [x] Update `get_let_type` helper to use pattern bindings

## Phase 3: Tests Un-ignored

The following 5 tests now pass:
- [x] `test_struct_destructuring_simple`
- [x] `test_struct_destructuring_with_rename`
- [x] `test_struct_destructuring_partial`
- [x] `test_enum_destructuring_simple`
- [x] `test_enum_destructuring_nested`

## Phase 4: Type Validation for Destructuring (Future Work)

These error tests remain ignored (type validation not implemented):
- [ ] `test_error_array_destructuring_type_mismatch`
- [ ] `test_error_struct_destructuring_type_mismatch`
- [ ] `test_error_struct_destructuring_missing_field`

Note: Implementing proper type validation for destructuring would require
inferring element types from the source type, which is more complex.

## Phase 5: Validation

- [x] All tests pass (`cargo test`) - 820 passing
- [x] 5 of 8 previously ignored tests now pass (3 remaining are error tests)
- [x] `cargo fmt --check` passes
- [x] `cargo clippy` passes
- [ ] Coverage remains above 80%

## Phase 6: PR

- [ ] Commit with message: `feat(semantic): add full destructuring pattern validation`
- [ ] Create PR
- [ ] User approval
- [ ] Merge to main
- [ ] Clean up worktree

---

## Notes

- Added `PatternBinding` struct to hold name and span from extracted bindings
- Added `collect_bindings_from_pattern` and `collect_bindings_recursive` functions
- All pattern types (Simple, Array, Struct, Tuple) now properly register bindings
- Struct patterns use alias if present, otherwise field name
- Array patterns handle Binding, Rest(Some), but skip Rest(None) and Wildcard
- Tuple patterns recurse into elements
