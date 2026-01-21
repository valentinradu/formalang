# Stdlib Testing

## Purpose

Visual regression testing infrastructure for FormaLang standard library components. Enables GPU-based snapshot testing of shader output generated from FormaLang source code, ensuring WGSL codegen produces visually correct results.

## Architecture

```
FormaLang Source -> Compiler -> IR -> WGSL Codegen -> GPU Render -> PNG Snapshot
                                                                         |
                                                              Golden Comparison
```

**Components:**
- `stdlib/tests/shader_snapshots/harness.rs` - GPU rendering infrastructure (wgpu-based)
- `stdlib/tests/shader_snapshots/wrapper.rs` - FormaLang source generators for test specs
- `stdlib/tests/shader_snapshots/comparator.rs` - PNG comparison with tolerance
- `stdlib/tests/shader_snapshots/tests/` - Test modules by category (shapes, fills, effects, etc.)
- `stdlib/tests/shader_snapshots/golden/` - Reference images

**Flow:**
1. Test spec (e.g., `ShapeRenderSpec::rect(100.0, 100.0, RED, 5.0)`) generates FormaLang source
2. Compiler produces WGSL shader
3. GPU renders shader to pixel buffer
4. Comparator checks against golden image (2% pixel threshold, 5 units per-channel)

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| GPU rendering required | WGSL shaders need actual GPU execution for accurate results |
| Graceful GPU skip | CI may lack GPU; tests skip with warning rather than fail |
| `SNAPSHOT_UPDATE=1` workflow | Regenerate goldens explicitly, not automatically |
| Tiered test expansion | Start with shapes, expand to fills, layout, effects progressively |
| Ignored tests for missing infrastructure | Track intent without blocking CI; layout/effects need runtime not yet built |

## Usage

```bash
# Run snapshot tests (skips if no GPU)
cargo test --test shader_snapshots

# Update golden images
SNAPSHOT_UPDATE=1 cargo test --test shader_snapshots

# Run specific test
cargo test rect_solid_fill
```

**Writing new tests:**
```rust
let spec = ShapeRenderSpec::rect(100.0, 100.0, RED, 5.0);
run_shape_snapshot_test(&spec, "test_name");
```

## Testing

**Current coverage:**
- Shape primitives: Rect, Circle, Ellipse (solid fills)
- Edge cases: zero/max corner radius, tiny shapes, gradient edge cases
- 62 passing, 67 ignored (require unbuilt infrastructure)

**Test locations:**
- `stdlib/tests/shader_snapshots/tests/shapes.rs` - Shape primitives
- `stdlib/tests/shader_snapshots/tests/edge_cases.rs` - Boundary conditions
- `stdlib/tests/shader_snapshots/golden/` - Reference images

## Maintenance

**Known limitations:**
- Layout tests require runtime layout engine (not yet implemented)
- Effect tests require multi-pass rendering infrastructure (not yet implemented)
- Content tests (Label, Image) require font/texture loading

**Codegen fixes applied:**
- `resolve_renamed_array_element_type()` - Correct method call mangling in unrolled loops
- Never type inference - Proper type resolution for if-expressions in match arms
- Struct constructor heuristic - PascalCase + named args detection

**Future work tracked in manifest:**
- `docs/stdlib-testing/stdlib-testing-manifest.toml` - Full task list with status
