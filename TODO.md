# FormaLang Shader Infrastructure TODO

Remaining work to get all stdlib snapshot tests passing.

## Test Status Summary

- **62 tests passing**
- **67 tests ignored** (infrastructure needed)
- **0 tests failing**

## Completed Fixes

### Codegen Fixes (Latest Session)
- **Method call mangling in unrolled loops** - `resolve_renamed_array_element_type()` extracts actual element type from renamed variables
- **Never type inference** - Added to suspicious type detection for if-expressions in match arms
- **Struct constructor type inference** - PascalCase + named args heuristic identifies struct instantiation
- **Debug output cleanup** - Removed debug `eprintln!` statements

### Previous Sessions
- Pattern fills, MultiLinear fills, deterministic WGSL codegen
- Inline module glob imports, WGSL struct name escaping
- External enum type lookup, closure support
- Hoisting correctness (closure, loop body, condition, block statement, match arm)

## Remaining Work

### 1. Wrapper Type Implementations (Test Infrastructure)

Stubs in `stdlib/tests/shader_snapshots/wrapper.rs` need implementation:

| Task | Location | Blocks |
|------|----------|--------|
| `ShapeRenderSpec::polygon` | wrapper.rs:673 | polygon tests |
| `ShapeRenderSpec::line` | wrapper.rs:689 | line tests |
| `ShapeRenderSpec::contour` | wrapper.rs:707 | contour tests |
| `generate_formalang_source` for Polygon/Line/Contour | wrapper.rs:790-796 | above tests |
| `generate_formalang_source` for ShapeUnion/Intersection/Subtraction | wrapper.rs:799-805 | boolean tests |
| `FillRenderSpec` methods | wrapper.rs:198 | fill tests |
| `LayoutRenderSpec` methods | wrapper.rs:325 | layout tests |
| `EffectRenderSpec` methods | wrapper.rs:427 | effect tests |
| `ContentRenderSpec` methods | wrapper.rs:479 | content tests |
| `TransformRenderSpec` methods | wrapper.rs:527 | transform tests |
| `AnimationSnapshotSpec` methods | wrapper.rs:588 | animation tests |

### 2. Codegen Fixes (Compiler)

| Issue | Tests Blocked | Location |
|-------|---------------|----------|
| Direct Fill struct instantiation (wrap in FillData) | 4 relative fill tests | `src/codegen/wgsl.rs` struct field assignment |
| Boolean shape composite rendering (combine child SDFs) | 3 boolean shape tests | `src/codegen/wgsl.rs` shape render generation |
| Transitive module import for enum lookups | stdlib validation | `src/codegen/wgsl.rs` `gen_field_load_expr_external` |

### 3. New Infrastructure Required

| Infrastructure | Tests Blocked | Scope |
|----------------|---------------|-------|
| Multi-pass rendering pipeline | 23 effect tests (blur, shadow, masks) | New `harness.rs` capability |
| Layout constraint solver | 18 layout tests (HStack, VStack, etc.) | New layout engine |
| Time parameter support | 6 animation tests | Harness extension |
| Font rasterization | 2 text tests | External dependency |
| Texture loading | 1 image test | External dependency |

## Priority Order

1. **Wrapper implementations** - Unblocks many shape/fill tests with no compiler changes
2. **Direct Fill struct instantiation** - Enables relative fill tests
3. **Boolean shape composite** - Enables boolean shape tests
4. **Transform modifiers** - Enables transform tests
5. **Multi-pass rendering** - Enables effect tests (largest batch)
6. **Layout engine** - Enables layout tests (requires significant new code)

## Test Locations

- `stdlib/tests/shader_snapshots/tests/shapes.rs` - Shape primitives
- `stdlib/tests/shader_snapshots/tests/fills.rs` - Fill types (stubbed)
- `stdlib/tests/shader_snapshots/tests/edge_cases.rs` - Boundary conditions (7 done, rest blocked)
- `stdlib/tests/shader_snapshots/tests/layout.rs` - Layout containers (blocked)
- `stdlib/tests/shader_snapshots/tests/effects.rs` - Visual effects (blocked)
- `stdlib/tests/shader_snapshots/tests/animation.rs` - Animation (blocked)
- `stdlib/tests/shader_snapshots/tests/content.rs` - Text/Image (blocked)
- `stdlib/tests/shader_snapshots/tests/modifiers.rs` - Transforms (blocked)
- `stdlib/tests/shader_snapshots/tests/composition.rs` - Complex nesting (blocked)
