# Shader Snapshots Spec

## Requirements

### Purpose

Validate FormaLang stdlib visual output by rendering WGSL shaders to images and comparing against golden files. Catches regressions in shape rendering, fill sampling, and GPU codegen.

### Behavior

**Inputs:**
- FormaLang source using stdlib components (shapes, fills)
- Render dimensions (width, height in pixels)
- Golden image path

**Outputs:**
- Pass/fail comparison result
- Diff image on failure (highlights pixel differences)
- Actual rendered image on failure

**Edge cases:**
- Missing golden file: create it (update mode)
- GPU unavailable: fail gracefully with clear error
- Cross-platform precision: tolerance-based comparison

### Constraints

**Excluded scope:**
- Layout testing (layout_propose/report/position) - separate feature
- Animation testing - requires temporal comparison
- Interactive controls - requires event simulation

**Initial scope:** Core shapes (Rect, Circle, Ellipse) with Solid fill only.

### Acceptance

1. `cargo test shader_snapshots` runs without GPU display
2. Intentional shader change causes test failure with diff image
3. `SNAPSHOT_UPDATE=1` regenerates golden files
4. CI passes on GitHub Actions with Mesa software renderer

## Design

### Approach

The WGSL generator produces library code (structs, methods) but not GPU entry points.
We need a **wrapper shader** that provides `@vertex`/`@fragment` entry points calling
the component's `render(uv, size)` method.

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────┐
│ FormaLang src   │────▶│ compile_to_ir│────▶│ generate_   │
│ (stdlib shapes) │     │              │     │ wgsl        │
└─────────────────┘     └──────────────┘     └─────────────┘
                                                    │
                                                    ▼
                                             ┌─────────────┐
                                             │ + wrapper   │
                                             │ entry points│
                                             └─────────────┘
                                                    │
                                                    ▼
┌─────────────────┐     ┌──────────────┐     ┌─────────────┐
│ Compare against │◀────│ Read texture │◀────│ wgpu render │
│ golden PNG      │     │ to CPU       │     │ (headless)  │
└─────────────────┘     └──────────────┘     └─────────────┘
```

**Wrapper shader template:**
```wgsl
// Generated library code from FormaLang
{generated_wgsl}

// Test harness entry points
struct Uniforms {
    size: vec2<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    // Fullscreen triangle
    let x = f32((idx & 1u) << 2u) - 1.0;
    let y = f32((idx & 2u) << 1u) - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pos.xy / uniforms.size;
    let component = {component_instance};  // e.g., Rect with defaults
    let color = {struct_name}_render(component, uv, uniforms.size);
    return vec4<f32>(color.r, color.g, color.b, color.a);
}
```

**Components:**
- `ShaderTestHarness`: wgpu headless context, render pipeline
- `WrapperGenerator`: generates entry points for component under test
- `SnapshotComparator`: tolerance-based image comparison
- `shader_snapshot!` macro: ergonomic test definition

### Dependencies

| Crate | Version | Purpose | License |
|-------|---------|---------|---------|
| wgpu | 27.0 | Headless GPU rendering | MIT/Apache-2.0 |
| pollster | 0.4 | Block on async in tests | MIT/Apache-2.0 |
| image | 0.25 | PNG encode/decode | MIT/Apache-2.0 |

All dependencies are well-maintained (>1M downloads, recent releases).

### Interfaces

```rust
// harness.rs
pub struct ShaderTestHarness { /* wgpu handles */ }

impl ShaderTestHarness {
    pub async fn new(width: u32, height: u32) -> Result<Self, HarnessError>;
    pub async fn render(&self, spec: &ShapeRenderSpec) -> Result<Vec<u8>, RenderError>;
}

// wrapper.rs
/// Specification for a shape to render.
/// Uses WGSL naming convention: methods become `{StructName}_{method}`.
pub struct ShapeRenderSpec {
    pub struct_name: String,           // e.g., "Rect" -> calls Rect_render()
    pub size: (f32, f32),              // render dimensions
    pub fields: ShapeFields,           // shape-specific configuration
}

pub enum ShapeFields {
    Rect { fill_rgba: [f32; 4], corner_radius: f32 },
    Circle { fill_rgba: [f32; 4] },
    Ellipse { fill_rgba: [f32; 4] },
}

impl ShapeRenderSpec {
    /// Generate complete WGSL with entry points.
    /// Includes: library code + Solid fill struct + shape instance + entry points.
    pub fn generate_wgsl(&self, library_wgsl: &str) -> String;
}

// comparator.rs
pub struct SnapshotComparator {
    pub tolerance: f32,        // 0.0-1.0 per channel
    pub max_diff_percent: f32, // max % differing pixels
}

impl SnapshotComparator {
    pub fn compare(&self, actual: &[u8], expected: &[u8], w: u32, h: u32) -> CompareResult;
    pub fn save_diff(&self, actual: &[u8], expected: &[u8], path: &Path) -> io::Result<()>;
}

pub enum CompareResult {
    Match,
    Mismatch { diff_count: u32, diff_percent: f32 },
    SizeMismatch { actual: (u32, u32), expected: (u32, u32) },
}

// macros.rs
macro_rules! shader_snapshot {
    ($name:ident, $source:expr, size: ($w:expr, $h:expr)) => { ... };
}
```

### Failure Modes

| Failure | Handling |
|---------|----------|
| wgpu init fails | Return `HarnessError::GpuUnavailable` with adapter info |
| WGSL compile fails | Return `RenderError::ShaderCompile(naga error)` |
| Golden missing | Create if `SNAPSHOT_UPDATE=1`, else fail with path |
| Comparison fails | Save diff + actual to `tests/shader_snapshots/failed/{test_name}_*.png` |

### Risks

| Risk | Mitigation |
|------|------------|
| GPU differences across platforms | Tolerance-based comparison (2% per channel, 0.1% pixels) |
| CI lacks GPU | Mesa software renderer (llvmpipe) |
| wgpu/naga version conflicts | Pin versions, test upgrade path |
| Large golden files in git | PNG compression, consider git-lfs if >10MB total |
