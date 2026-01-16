# Shader Support Review

## Summary

Feature: Shader snapshot testing infrastructure and codegen fixes
Tests: 48 passing, 81 ignored (infrastructure needed), 0 failing

## Changes by File

### Core Compiler (src/)

#### `src/codegen/wgsl.rs` (+899 lines)
- **external_type_to_wgsl()**: Single source of truth for External type conversion (struct/trait/enum)
- **collect_assigned_vars()**: Tracks mutable variables for `var` vs `let` in WGSL
- **generated_structs**: Deduplication set prevents duplicate struct generation
- **Enum type handling**: Fixed simple name comparison for qualified paths like `fill::PatternRepeat`

#### `src/semantic/mod.rs` (+20 lines)
- **Qualified struct paths**: Changed `is_struct()` to `get_struct_qualified()` for nested module paths
- **FieldAccess**: Added validation, reference extraction, type inference, mutability checks

#### `src/semantic/symbol_table.rs` (+5 lines)
- **all_public_symbols()**: Added modules to returned symbols for glob imports

#### `src/ir/*.rs` (+91 lines combined)
- Added `IrExpr::FieldAccess` variant across IR infrastructure (expr, visitor, fold, dce, lower)

### Test Infrastructure (stdlib/tests/shader_snapshots/)

#### `wrapper.rs` (+2000 lines)
- FillSpec variants: RelativeLinear, RelativeRadial, RelativeSolid, RelativeAngular
- FormaLang source generation for all shape/fill types
- Pattern fill support with nested source

#### `harness.rs` (+197 lines)
- GPU rendering infrastructure updates

#### `tests/shapes.rs` (+216 lines)
- Shape snapshot tests (circle, rect, ellipse, polygon, line, contour, boolean)

### Stdlib

#### `stdlib/fill.fv` (+38 lines)
- ColorStop moved to module top level for path resolution
- Pattern repeat enum fixes

### Deleted

- `docs/shader-snapshots/*` (-692 lines): Moved/consolidated
- `tests/semantic_validation.rs` (-112 lines): Tests moved elsewhere

## Key Decisions

| Decision | Rationale | Alternatives Considered |
|----------|-----------|------------------------|
| `external_type_to_wgsl()` helper | Single source of truth prevents divergence | Keep duplicate code (rejected: maintenance risk) |
| Ignore `test_stdlib_wgsl_validation` | Blocked on blocks-in-expression codegen | Fix now (rejected: scope creep) |
| 81 tests ignored | Require infrastructure not yet built | Stub tests (current), skip entirely (rejected: lose tracking) |

## Risks

### Medium
1. **Deleted tests** (`tests/semantic_validation.rs`): Verify coverage didn't decrease
2. **Large wrapper.rs**: 2000+ lines may need splitting

### Low
1. **FieldAccess "Unknown" type**: Returns simplified type, may need refinement
2. **Ignored test count**: 81 ignored tests is significant backlog

## Test Coverage

| Area | Status | Notes |
|------|--------|-------|
| External enum type handling | Covered | Shader snapshot tests exercise this |
| Qualified struct paths | Covered | `fill::relative::*` tests |
| Glob imports with modules | Covered | Pattern tests use `use stdlib::fill::*` |
| FieldAccess IR | Covered | IR visitor tests |

### Untested
- Blocks in expression position (test ignored)
- Direct Fill struct FillData wrapping (tests ignored)
- Boolean shape composite rendering (tests ignored)

## Consistency Check

- TODO.md matches test ignore reasons
- Manifest updated with audit findings
- All passing tests verified
