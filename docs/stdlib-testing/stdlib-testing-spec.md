# Stdlib Testing Specification

## Purpose

Comprehensive visual regression testing for the FormaLang standard library. The current test suite covers only basic shapes (Rect, Circle, Ellipse) with solid fills. This spec defines progressive expansion to cover all stdlib components with increasing complexity.

## Current State

- **Covered**: `Rect`, `Circle`, `Ellipse` with `fill::Solid`
- **Golden images**: 4 snapshots in `stdlib/tests/shader_snapshots/golden/shapes/`
- **Infrastructure**: GPU harness, WGSL wrapper generator, PNG comparator

## Behavior

### Inputs

- FormaLang source using stdlib components
- Render dimensions (width, height)
- Shape/fill configurations

### Outputs

- WGSL shaders compiled from FormaLang
- Rendered PNG snapshots
- Pass/fail comparison against golden images

### Edge Cases

- Zero-dimension shapes
- Extremely small shapes (anti-aliasing boundary)
- Maximum corner radius (fully rounded)
- Gradient angles at 0, 90, 180, 270 degrees
- Nested containers with padding overflow
- Empty container bodies

## Constraints

- Tests must run headlessly (CI-compatible)
- GPU may be unavailable; tests should skip gracefully
- Snapshot comparison tolerance: 2% pixel threshold, 5 units per-channel tolerance
- No animation testing (static snapshots only)
- No interactive control testing (layout/render only)

## Acceptance Criteria

1. All shape primitives have snapshot tests
2. All fill types have snapshot tests
3. Layout containers have composition tests
4. Effects have visual verification tests
5. Tests run in CI with GPU fallback handling
6. `SNAPSHOT_UPDATE=1` regenerates golden images

---

## Design

### Approach

Progressive test expansion in tiers:

1. **Tier 1 - Shapes**: Shape primitives (Rect, Circle, Ellipse, Polygon, Line, Contour, Boolean ops)
2. **Tier 2 - Fills**: Gradient types and patterns (Solid, Linear, Radial, Angular, Pattern, MultiLinear)
3. **Tier 3 - Layout**: Stack containers (VStack, HStack, ZStack, Frame, Grid, Spacer, Scroll)
4. **Tier 4 - Effects**: Visual effects (Opacity, filters, Blur, Shadow, Blended, Mask, Clip)
5. **Tier 5 - Content**: Text and image components (Label, Paragraph, Image)
6. **Tier 6 - Transforms**: Modifier operations (scale, rotate, translate)
7. **Tier 7 - Animation**: Static easing/transition snapshots
8. **Tier 8 - Composition**: Complex nested structures

### Dependencies

- `wgpu` - GPU rendering (existing)
- `pollster` - Async runtime (existing)
- `png` - Image I/O (existing)
- No new dependencies required

### Interfaces

```rust
// Extend ShapeRenderSpec for new shapes
pub enum ShapeFields {
    Rect { fill_rgba, stroke_rgba, corner_radius, stroke_width },
    Circle { fill_rgba, stroke_rgba, stroke_width },
    Ellipse { fill_rgba, stroke_rgba, stroke_width },
    // NEW
    Polygon { fill_rgba, stroke_rgba, sides, rotation, stroke_width },
    Line { stroke_rgba, from: (f32, f32), to: (f32, f32), stroke_width },
    Contour { fill_rgba, stroke_rgba, segments: Vec<PathSegment>, closed, stroke_width },
}

pub enum PathSegment {
    LineTo { to: (f32, f32) },
    Arc { to: (f32, f32), radius, clockwise, large_arc },
    QuadBezier { to: (f32, f32), control: (f32, f32) },
    CubicBezier { to: (f32, f32), control1: (f32, f32), control2: (f32, f32) },
}

// New spec types for fills
pub struct FillRenderSpec {
    pub shape: ShapeFields,
    pub fill: FillSpec,
    pub size: (f32, f32),
}

pub enum FillSpec {
    Solid { rgba },
    Linear { from_rgba, to_rgba, angle },
    Radial { from_rgba, to_rgba, center_x, center_y },
    Angular { from_rgba, to_rgba, angle },
    Pattern { source: Box<FillSpec>, width, height, repeat: PatternRepeat },
    MultiLinear { stops: Vec<(rgba, f32)>, angle },
}

// Relative coordinate variants (same structure, different semantic)
pub mod relative {
    pub use super::FillSpec;  // Same API, compiled differently
}

// Layout composition spec
pub struct LayoutRenderSpec {
    pub container: ContainerType,
    pub children: Vec<ShapeRenderSpec>,
    pub size: (f32, f32),
}
```

### Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| GPU unavailable | Skip test with warning, don't fail CI |
| Shader compile error | Fail test with WGSL error message |
| Golden missing | Fail with path to actual image |
| Pixel mismatch | Fail with diff image and percentage |
| Size mismatch | Fail with dimension comparison |

### Risks

| Risk | Mitigation |
|------|------------|
| GPU driver differences | Tolerance threshold in comparator |
| Anti-aliasing variance | 1-2 pixel tolerance at edges |
| Float precision | Round to fixed decimal in WGSL generation |
| Large golden file size | PNG compression, selective test cases |
