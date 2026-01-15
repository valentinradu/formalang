# Shader Support Feature Review

**Branch**: feat/shader-support
**Base**: main
**Stats**: ~20k lines added, 49 files changed, 4 commits

## Summary

This feature adds WGSL shader code generation to FormaLang, enabling compilation of
FormaLang stdlib shapes/fills to GPU shaders for rendering.

## Commits

1. `cb3df74` - fix(ir): populate fields for imported types
2. `d1f59b6` - feat(ir): add IrLet and resolve self/let references
3. `3640974` - feat(codegen): add WGSL shader code generation
4. `2a9e410` - feat: unify invocation syntax and enhance language features

## Changes by Category

### New Modules (Core Feature)

| File | Lines | Purpose |
|------|-------|---------|
| `src/codegen/wgsl.rs` | 6509 | WGSL code generation from IR |
| `src/codegen/dispatch.rs` | 637 | Trait dispatch for polymorphic calls |
| `src/codegen/flatten.rs` | 422 | IR flattening for WGSL |
| `src/codegen/monomorph.rs` | 443 | Generic monomorphization |
| `src/codegen/sourcemap.rs` | 257 | Source map generation |
| `src/codegen/transpile.rs` | 385 | WGSL validation via naga |
| `src/codegen/fvc.rs` | 474 | FVC binary format |
| `src/bin/fvc.rs` | 284 | FVC compiler CLI |
| `src/builtins/mod.rs` | 1132 | GPU builtin functions |
| `src/ir/dce.rs` | 642 | Dead code elimination |
| `src/ir/fold.rs` | 612 | Constant folding |

### Enhanced Modules

| File | Delta | Purpose |
|------|-------|---------|
| `src/ir/lower.rs` | +1368 | Self/let resolution, impl handling |
| `src/parser/mod.rs` | +965 | Unified invocation syntax |
| `src/semantic/mod.rs` | +1627 | Trait validation, type inference |

### Test Coverage

| File | Lines | Coverage |
|------|-------|----------|
| `tests/ir.rs` | +863 | IR lowering tests |
| `tests/parser_edge_cases.rs` | +556 | Parser disambiguation |
| `tests/semantic_validation.rs` | +407 | Semantic checks |
| `tests/shader_snapshots.rs` | new | Visual GPU rendering tests |

## Key Design Decisions

### 1. Enum Representation in WGSL

WGSL lacks native enums. Solution:
```wgsl
struct EnumName {
    discriminant: u32,
    data: array<f32, N>,  // N = max variant size
}
```

**Risk**: Padding calculation must be exact or shaders crash.
**Mitigation**: `type_size_in_f32()` with module-aware enum lookups.

### 2. Trait Dispatch

FormaLang traits compile to WGSL switch statements:
```wgsl
fn Fill_sample(data: FillData, uv: vec2<f32>) -> vec4<f32> {
    switch data.type_tag {
        case FillData_Solid: { return fill_Solid_sample(...); }
        case FillData_Linear: { return fill_Linear_sample(...); }
        ...
    }
}
```

**Risk**: Recursive implementors (Pattern has Fill field) cause WGSL cycles.
**Mitigation**: Loop-based dispatch for recursive cases.

### 3. Module-Local IDs

EnumId/StructId are local to their source module.
**Risk**: HashMap iteration non-determinism caused wrong lookups.
**Fixed**: Source module checked first in all lookups.

## Risk Areas

### High Confidence (well tested)
- Basic shapes: Rect, Circle, Ellipse
- Solid fills
- Enum data packing

### Medium Confidence (limited testing)
- Gradient fills (Linear, Radial)
- Pattern fills (recursive dispatch)
- Complex nested enums

### Lower Confidence (edge cases)
- Cross-module enum references
- Generic monomorphization edge cases
- Very large enum variants

## Test Coverage Summary

- **Unit tests**: 346+ tests pass
- **Shader snapshots**: 16 tests (4 shapes x rendering pipeline)
- **Visual verification**: Golden images for Rect, Circle, Ellipse

## Open Items

1. Pattern fill dispatch (recursive) generates complex WGSL - works but verbose
2. `type_str` unused variable warning at wgsl.rs:3534

## Recommendation

Ready for merge. All critical paths tested, audit findings addressed.
