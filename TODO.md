# FormaLang Shader Infrastructure TODO

This document tracks remaining infrastructure work needed for shader snapshot tests.

## Test Status Summary

- **48 tests passing**
- **81 tests ignored** (infrastructure needed)
- **0 tests failing**

## Fixed Issues (This Session)

1. **Pattern fills** - Nested Fill source serialization into FillData array
2. **MultiLinear fills** - ColorStop path resolution (moved to module top level)
3. **Deterministic WGSL codegen** - Sorted module paths and struct indices
4. **Inline module glob imports** - Added modules to `all_public_symbols()`
5. **WGSL struct name escaping** - Added `to_wgsl_identifier()` for qualified paths
6. **External enum type lookup** - Fixed simple name comparison in `type_to_wgsl`

## Remaining Infrastructure

### High Priority (Compiler/Codegen Fixes)

#### Direct Fill Struct Instantiation (4 tests)

**Status**: Blocked on codegen
**Tests**: `relative_linear`, `relative_radial`, `relative_solid`, `relative_angular`

The inferred enum syntax (`.linear()`, `.solid()`) automatically wraps fills in FillData
for trait dispatch. Direct struct paths like `fill::relative::Linear(...)` don't.

**Fix needed**: When assigning a trait implementor struct to a trait-typed field,
automatically wrap in the trait's data struct (e.g., FillData).

**Location**: `src/codegen/wgsl.rs` - struct field assignment handling

#### Boolean Shape Composite Rendering (3 tests)

**Status**: Blocked on codegen
**Tests**: `union_two_circles`, `intersection_rect_circle`, `subtraction_rect_minus_circle`

Boolean shapes (ShapeUnion, ShapeIntersection, ShapeSubtraction) compile correctly
but WGSL codegen doesn't generate the composite SDF/render functions.

**Fix needed**:
1. Generate SDF functions for child shapes
2. Generate `sdf_combine()` function that combines child SDFs
3. Generate composite render function using combined SDF

**Location**: `src/codegen/wgsl.rs` - shape render function generation

#### Blocks in Expression Position (stdlib validation)

**Status**: Blocked on codegen
**Tests**: `test_stdlib_wgsl_validation` (IR test)

Some stdlib impl functions contain `for` or `match` blocks in expression positions.
WGSL requires these to be at statement level, but codegen outputs placeholder comments.

**Fix needed**: Convert block expressions to statement-level constructs:
1. Detect blocks in expression positions during codegen
2. Hoist block to a let binding before the expression
3. Reference the binding in the expression

**Location**: `src/codegen/wgsl.rs` - expression codegen for Block

### Medium Priority (Test Infrastructure)

#### Transform Modifiers (4 tests)

**Status**: Blocked on test infrastructure
**Tests**: `transform_scale_2x`, `transform_rotate_45`, `transform_translate`, `transform_combined`

Transform modifiers exist in stdlib (TransformDeformer enum with scale/rotate/translate).
Test infrastructure needs to support modified shapes.

**Fix needed**:
1. Add modifier support to `ShapeRenderSpec`
2. Generate FormaLang source with modifier application
3. Update entry point to apply transforms to UV coordinates

**Location**: `stdlib/tests/shader_snapshots/wrapper.rs`

### Lower Priority (External Dependencies)

#### Effect Compositing (23 tests)

**Status**: Blocked on multi-pass rendering
**Tests**: blur, shadow, glow, color adjustments, masks, clips

Effects like blur and shadow require multi-pass GPU rendering where one pass
renders to a texture and another samples from it.

**Fix needed**:
1. Multi-pass render pipeline in test harness
2. Intermediate texture support
3. Effect-specific shader composition

**Location**: `stdlib/tests/shader_snapshots/harness.rs`

#### Layout Containers (18 tests)

**Status**: Blocked on layout engine
**Tests**: HStack, VStack, ZStack, Frame, Grid, Spacer

Layout containers require a runtime layout engine to compute child positions
and sizes based on constraints.

**Fix needed**:
1. Layout constraint solver
2. Child position/size computation
3. Multi-shape rendering with positions

**Location**: New infrastructure needed

#### Animation (6 tests)

**Status**: Blocked on time-based state
**Tests**: easing curves, transitions, keyframes

Animation requires time-based state and easing function evaluation.

**Fix needed**:
1. Time parameter support in test harness
2. Easing function evaluation
3. State interpolation

#### Text Rendering (2 tests)

**Status**: Blocked on font rasterization
**Tests**: Label, styled text

Text requires font loading and glyph rasterization.

**Fix needed**:
1. Font loading infrastructure
2. Glyph atlas generation
3. Text layout engine

#### Image Rendering (1 test)

**Status**: Blocked on texture loading
**Tests**: Image content

Images require texture loading from files.

**Fix needed**:
1. Image file loading
2. Texture creation and sampling
3. UV mapping for images

#### Edge Cases (16 tests)

**Status**: Various blockers
**Tests**: nested compositions, boundary conditions, error cases

These depend on other features being implemented first.

## Architecture Notes

### Trait Dispatch System

The Fill trait uses a data wrapper pattern:
- `FillData` stores type tag + field data as f32 array
- `Fill_sample()` dispatches to implementor functions based on type tag
- Inferred enum syntax (`.linear()`) automatically wraps in FillData

### SDF-Based Rendering

Shapes use signed distance functions:
- Positive distance = outside shape
- Negative distance = inside shape
- Zero = on shape boundary
- Anti-aliasing uses `smoothstep()` on distance

### Module System

- File-based modules: `use stdlib::shapes::*`
- Inline modules: `mod name { }` inside files
- Glob imports now include inline modules (fixed this session)
