# GPU Shader Compilation Plan

## Goal

FormaLang compiles to `.fvc` (FormaVera Compiled) containing shaders + scene data. A separate runtime library executes it.

```
source.fv → Compiler → scene.fvc → Runtime → pixels + events
```

---

## Output Format (.fvc)

Single file containing:
- Compiled shaders (one or more formats)
- Scene data (flattened nodes)
- String table
- Metadata

### Target Flags

```bash
formalang compile scene.fv -o scene.fvc                # Default: WGSL
formalang compile scene.fv -o scene.fvc --fat          # All formats
formalang compile scene.fv -o scene.fvc --msl          # Metal Shading Language
formalang compile scene.fv -o scene.fvc --spirv        # SPIR-V
formalang compile scene.fv -o scene.fvc --dxil         # DirectX IL
formalang compile scene.fv -o scene.fvc --msl --spirv  # Combinable
```

### Shader Formats

| Flag | Format |
|------|--------|
| `--wgsl` (default) | WGSL |
| `--msl` | Metal Shading Language |
| `--spirv` | SPIR-V |
| `--dxil` | DirectX IL |
| `--fat` | All above |

---

## Language

### Types

```formalang
// Scalars
f32, i32, u32, bool

// Vectors
vec2, vec3, vec4
ivec2, ivec3, ivec4
uvec2, uvec3, uvec4

// Matrices
mat2, mat3, mat4

// Arrays (fixed capacity, explicit)
[T; N]

// Optional
T?

// Structs, Enums, Traits
```

### Functions in Impl Blocks

```formalang
impl Rect {
    // Field defaults
    fill: nil,

    // Functions
    fn layout_propose(self, proposed: Size) -> Size {
        Size(
            width: self.width.resolve(proposed.width),
            height: self.height.resolve(proposed.height)
        )
    }

    fn render(self, uv: vec2) -> Color {
        self.fill.sample(uv)
    }
}
```

### Builtins (Categorized Modules)

```formalang
use builtin::math::*
use builtin::vector::*

// builtin::math
sin(x), cos(x), tan(x)
abs(x), min(a, b), max(a, b), clamp(x, lo, hi)
lerp(a, b, t), step(edge, x), smoothstep(a, b, x)
sqrt(x), pow(x, y), exp(x), log(x)
floor(x), ceil(x), fract(x)

// builtin::vector
dot(a, b), cross(a, b), normalize(v), length(v)
reflect(v, n), refract(v, n, eta)
```

---

## Type Mapping

| FormaLang | GPU Buffer |
|-----------|------------|
| `f32` | `f32` |
| `i32` | `i32` |
| `u32` | `u32` |
| `bool` | `u32` (0/1) |
| `String` | `u32` (handle) |
| `[T; N]` | `array<T, N>` |
| `T?` | `u32` + `T` |
| `enum` | `u32` + data |
| `struct` | flattened fields |

---

## Compiler Responsibilities

The compiler is **layout/render agnostic**. It does not know about layout passes, rendering, or hit testing.

### What Compiler Does

1. Compile all `fn` in impl blocks to WGSL functions
2. Generate dispatch for functions with multiple implementations:
   ```wgsl
   fn dispatch_layout_propose(idx: u32, proposed: vec2<f32>) -> vec2<f32> {
       switch (nodes[idx].kind) {
           case KIND_RECT: { return Rect_layout_propose(idx, proposed); }
           case KIND_VSTACK: { return VStack_layout_propose(idx, proposed); }
       }
   }
   ```
3. Flatten scene tree to node array with depth indices
4. Map struct fields to buffer offsets
5. Transpile WGSL to target formats (via naga)
6. Bundle into .fvc

### What Compiler Does NOT Know

- What `layout_propose` means
- How many layout passes exist
- What order functions are called
- That hit testing exists

---

## Runtime Responsibilities

Separate project: `formalang-runtime`

### What Runtime Does

1. Load .fvc file
2. Create GPU buffers from scene data
3. Execute the layout protocol:
   - For each depth level (top-down): call `dispatch_layout_propose`
   - For each depth level (bottom-up): call `dispatch_layout_report`
   - For each depth level (top-down): call `dispatch_layout_position`
4. Execute rendering: call `dispatch_render`
5. Execute hit testing: call `dispatch_hit_test`
6. Return events to host

### Protocol Defined By

- **Stdlib**: Defines function signatures (layout_propose, render, etc.)
- **Runtime**: Knows the call order and dispatch pattern

---

## Stdlib Pattern

Stdlib defines data + behavior. Compiler just compiles it.

```formalang
pub struct Rect: Shape {
    width: Dimension,
    height: Dimension,
    fill: Fill?,
    mount body: Never
}

impl Rect {
    fill: nil,

    fn layout_propose(self, proposed: Size) -> Size {
        Size(
            width: self.width.resolve(proposed.width),
            height: self.height.resolve(proposed.height)
        )
    }

    fn layout_report(self) -> Size {
        Size(width: self.actual_w, height: self.actual_h)
    }

    fn layout_position(self, bounds: Rect) { }

    fn render(self, uv: vec2) -> Color {
        if self.fill != nil {
            self.fill.sample(uv)
        } else {
            Color::transparent
        }
    }
}

impl fill::Linear {
    fn sample(self, uv: vec2) -> Color {
        let t = builtin::vector::rotate(uv, self.angle).x
        builtin::math::lerp(self.from, self.to, t)
    }
}
```

---

## Trait Dispatch

Inline switch on type tag:

```formalang
self.fill.sample(uv)  // fill: Fill (trait)
```

Compiles to:

```wgsl
fn dispatch_Fill_sample(kind: u32, data: ptr<...>, uv: vec2<f32>) -> vec4<f32> {
    switch (kind) {
        case FILL_SOLID: { return Solid_sample(data, uv); }
        case FILL_LINEAR: { return Linear_sample(data, uv); }
        case FILL_RADIAL: { return Radial_sample(data, uv); }
    }
}
```

---

## Layout Algorithm

Iterative by depth. Runtime dispatches, not compiler.

```
Pass 1: Propose (top-down)
  for depth in 0..max_depth:
    runtime calls dispatch_layout_propose for all nodes at depth

Pass 2: Report (bottom-up)
  for depth in max_depth..0:
    runtime calls dispatch_layout_report for all nodes at depth

Pass 3: Position (top-down)
  for depth in 0..max_depth:
    runtime calls dispatch_layout_position for all nodes at depth
```

---

## Scene Format (inside .fvc)

```
Header:
  magic: u32
  version: u32
  node_count: u32
  string_count: u32
  shader_count: u32

Shader Table:
  (format: u8, offset: u32, size: u32)*

Shaders:
  WGSL bytes (if included)
  SPIR-V bytes (if included)
  MSL bytes (if included)
  DXIL bytes (if included)

Node Array:
  kind: u16
  flags: u16
  parent: u32
  first_child: u32
  next_sibling: u32
  depth: u16
  child_count: u16
  element_id: u32
  proposed_w: f32
  proposed_h: f32
  actual_w: f32
  actual_h: f32
  pos_x: f32
  pos_y: f32
  data: [u8; DATA_SIZE]

String Table:
  (length: u32, utf8_bytes: [u8])*
```

---

## Runtime vs Compile Time

### No Recompilation (Buffer Updates)

- Window resize
- Pointer position
- Button state
- Frame time
- Node field values
- Text content
- Add/remove nodes within capacity

### Recompilation (Source Changes)

| Change | Rebuild |
|--------|---------|
| Function body | Shaders only |
| New struct/enum | Shaders + scene |
| Field type change | Everything |
| New trait impl | Shaders only |

---

## Development: Watch Mode

```bash
formalang watch scene.fv -o ./output/
```

- Monitors source files
- Detects shader-only vs full rebuild
- Incremental compilation
- Outputs updated .fvc

---

## Implementation Steps

### 1. Language Extensions
- [ ] `fn` in impl blocks
- [ ] vec/mat types
- [ ] f32/i32/u32/bool
- [ ] builtin::math, builtin::vector modules

### 2. IR Extensions
- [ ] IrFunction
- [ ] Method resolution
- [ ] Dispatch generation

### 3. Scene Compiler
- [ ] Tree flattening with depth index
- [ ] Binary format writer
- [ ] Field → offset mapping

### 4. WGSL Codegen
- [ ] Expression → WGSL
- [ ] Function → WGSL
- [ ] Dispatch function generation

### 5. Shader Transpilation
- [ ] Integrate naga
- [ ] WGSL → SPIR-V
- [ ] WGSL → MSL
- [ ] WGSL → DXIL

### 6. .fvc Format
- [ ] Bundle shaders + scene
- [ ] Target flags (--fat, --ios, etc.)
- [ ] Reader/writer library

### 7. CLI
- [ ] `formalang compile`
- [ ] `formalang watch`
- [ ] Target flags

### 8. Stdlib
- [ ] Layout functions (propose, report, position)
- [ ] Render functions
- [ ] Fill sampling
- [ ] Container layouts (VStack, HStack, etc.)

---

## Project Structure

```
formalang/                    # Compiler
├── src/
│   ├── gpu/
│   │   ├── validate.rs
│   │   ├── flatten.rs
│   │   ├── scene.rs
│   │   ├── wgsl.rs
│   │   └── fvc.rs
│   └── ...

formalang-runtime/            # Separate project
├── src/
│   ├── lib.rs
│   ├── loader.rs             # .fvc parsing
│   ├── executor.rs           # GPU dispatch
│   └── protocol.rs           # Layout/render protocol
```

---

## Text Handling

String → handle. Runtime provides glyph data.

1. Compiler: strings → string table in .fvc
2. Runtime: shapes strings (HarfBuzz) → glyph buffer
3. GPU: samples SDF atlas at glyph positions
