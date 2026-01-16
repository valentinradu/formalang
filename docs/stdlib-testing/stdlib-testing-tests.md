# Stdlib Testing - Test Plan

## Components

### Tier 1: Shape Primitives

| Component | Status | Tests |
|-----------|--------|-------|
| `Rect` | Covered | solid, rounded corners |
| `Circle` | Covered | solid fill |
| `Ellipse` | Covered | solid fill |
| `Polygon` | **TODO** | triangle, hexagon, rotated |
| `Line` | **TODO** | horizontal, vertical, diagonal |
| `ShapeUnion` | **TODO** | two circles |
| `ShapeIntersection` | **TODO** | rect + circle |
| `ShapeSubtraction` | **TODO** | rect - circle |
| `Contour` | **TODO** | triangle path, bezier curve |
| `LineTo` | **TODO** | straight segment |
| `Arc` | **TODO** | arc segment |
| `QuadBezier` | **TODO** | quadratic curve |
| `CubicBezier` | **TODO** | cubic curve |

### Tier 2: Fill Types

| Component | Status | Tests |
|-----------|--------|-------|
| `fill::Solid` | Covered | via shapes |
| `fill::Linear` | **TODO** | 0deg, 45deg, 90deg, 135deg |
| `fill::Radial` | **TODO** | centered, offset center |
| `fill::Angular` | **TODO** | 0deg start, 90deg start |
| `fill::Pattern` | **TODO** | repeat, repeatX, repeatY |
| `fill::MultiLinear` | **TODO** | 3-stop gradient |
| `fill::relative::*` | **TODO** | relative coordinate fills |

### Tier 3: Layout Containers

| Component | Status | Tests |
|-----------|--------|-------|
| `VStack` | **TODO** | 3 rects, spacing, alignment |
| `HStack` | **TODO** | 3 rects, spacing, alignment |
| `ZStack` | **TODO** | overlapping shapes |
| `Frame` | **TODO** | centered child |
| `Grid` | **TODO** | 2x2 layout |
| `Spacer` | **TODO** | in HStack |
| `Scroll` | **TODO** | scrollable content |

### Tier 4: Visual Effects

| Component | Status | Tests |
|-----------|--------|-------|
| `Opacity` | **TODO** | 0.5 opacity rect |
| `Grayscale` | **TODO** | colored rect |
| `Saturation` | **TODO** | 0.5 saturation |
| `Brightness` | **TODO** | 1.5x brightness |
| `Contrast` | **TODO** | 1.5x contrast |
| `HueRotation` | **TODO** | 90deg rotation |
| `Blur` | **TODO** | 4px blur |
| `Shadow` | **TODO** | drop shadow |
| `Blended` | **TODO** | multiply, screen, overlay |
| `Mask` | **TODO** | alpha mask |
| `ClipShape` | **TODO** | clip to circle |
| `Clipped` | **TODO** | clip to bounds |

### Tier 5: Content Components

| Component | Status | Tests |
|-----------|--------|-------|
| `Label` | **TODO** | text rendering |
| `Paragraph` | **TODO** | multi-line text |
| `Image` | **TODO** | image source |

### Tier 6: Transforms (Modifiers)

| Component | Status | Tests |
|-----------|--------|-------|
| `Modifier` | **TODO** | transform operations |
| Scale | **TODO** | 2x scale |
| Rotate | **TODO** | 45deg rotation |
| Translate | **TODO** | offset position |

### Tier 7: Animation (Static)

| Component | Status | Tests |
|-----------|--------|-------|
| `Easing` functions | **TODO** | linear, easeIn, easeOut curves |
| `Transition` transforms | **TODO** | opacity/scale/slide at 0.5 progress |

### Tier 8: Composition

| Component | Status | Tests |
|-----------|--------|-------|
| Nested stacks | **TODO** | VStack in HStack |
| Gradient shapes | **TODO** | Rect with linear gradient |
| Effect chains | **TODO** | Opacity + Grayscale |
| Complex UI | **TODO** | Button-like composition |

---

## Behaviors

### Shape Primitives

#### Shape Stroke Tests
- `rect_stroke_only`: no fill, 2px stroke
- `rect_stroke_and_fill`: both fill and stroke
- `circle_stroke_only`: no fill, 2px stroke
- `ellipse_stroke_thick`: 4px stroke width

#### Polygon Tests
- `polygon_triangle_solid`: 3-sided, no rotation
- `polygon_hexagon_solid`: 6-sided, no rotation
- `polygon_pentagon_rotated`: 5-sided, 36deg rotation
- `polygon_with_stroke`: hexagon with stroke, no fill

#### Line Tests
- `line_horizontal`: 0deg, 2px stroke
- `line_vertical`: 90deg, 2px stroke
- `line_diagonal`: 45deg, 2px stroke
- `line_thick`: 4px stroke width

#### Boolean Operations
- `union_two_circles`: overlapping circles merged
- `intersection_rect_circle`: visible only in overlap
- `subtraction_rect_minus_circle`: rect with hole

#### Contour/Path Tests
- `contour_triangle`: 3 LineTo segments, closed
- `contour_open_path`: unclosed stroke-only path
- `contour_quad_bezier`: curved path with QuadBezier
- `contour_cubic_bezier`: curved path with CubicBezier
- `contour_arc`: path with Arc segment

### Fill Types

#### Linear Gradient Tests
- `linear_horizontal`: angle=0, red to blue
- `linear_vertical`: angle=90, red to blue
- `linear_diagonal`: angle=45, red to blue
- `linear_reverse`: angle=180, red to blue

#### Radial Gradient Tests
- `radial_centered`: center=(0.5, 0.5), white to black
- `radial_offset`: center=(0.3, 0.3), white to black

#### Angular Gradient Tests
- `angular_basic`: 0deg start, red to blue
- `angular_rotated`: 90deg start, red to blue

#### Pattern Tests
- `pattern_repeat`: checkerboard effect
- `pattern_repeat_x`: horizontal stripes
- `pattern_repeat_y`: vertical stripes
- `pattern_nested_gradient`: pattern with linear gradient source

#### Relative Fill Tests
- `relative_linear`: same as linear but in relative coords
- `relative_radial`: same as radial but in relative coords

### Layout Containers

#### VStack Tests
- `vstack_basic`: 3 rects, default spacing
- `vstack_spaced`: 3 rects, 10px spacing
- `vstack_aligned_leading`: alignment=leading
- `vstack_aligned_trailing`: alignment=trailing
- `vstack_distribution_space_between`: even distribution

#### HStack Tests
- `hstack_basic`: 3 rects, default spacing
- `hstack_spaced`: 3 rects, 10px spacing
- `hstack_aligned_top`: alignment=top
- `hstack_aligned_bottom`: alignment=bottom

#### ZStack Tests
- `zstack_basic`: two overlapping rects
- `zstack_aligned_corner`: bottomTrailing alignment

#### Frame Tests
- `frame_centered`: child smaller than frame
- `frame_aligned`: topLeading alignment

#### Grid Tests
- `grid_2x2`: 4 items in 2 columns

#### Spacer Tests
- `spacer_in_hstack`: Spacer pushes content to edges
- `spacer_min_length`: Spacer with minLength constraint

#### Scroll Tests
- `scroll_vertical`: Vertical scroll container
- `scroll_clipped`: Content clipped at viewport

### Visual Effects

#### Opacity Tests
- `opacity_half`: 0.5 opacity on red rect
- `opacity_quarter`: 0.25 opacity

#### Filter Tests
- `grayscale_full`: 1.0 grayscale on colored rect
- `grayscale_partial`: 0.5 grayscale
- `saturation_desaturate`: 0.0 saturation
- `saturation_oversaturate`: 2.0 saturation
- `brightness_increase`: 1.5x brightness
- `brightness_decrease`: 0.5x brightness
- `contrast_increase`: 1.5x contrast
- `contrast_decrease`: 0.5x contrast
- `hue_rotation_90`: 90deg hue shift
- `hue_rotation_180`: 180deg hue shift

#### Blur/Shadow Tests
- `blur_small`: 2px blur radius
- `blur_large`: 8px blur radius
- `shadow_basic`: default shadow params
- `shadow_offset`: x=4, y=4 offset

#### Blend Mode Tests
- `blend_multiply`: multiply mode
- `blend_screen`: screen mode
- `blend_overlay`: overlay mode
- `blend_difference`: difference mode

#### Mask/Clip Tests
- `mask_alpha`: rect masked by gradient
- `clip_shape_circle`: rect clipped to circle
- `clipped_overflow`: content clipped at bounds

### Content Components

#### Label Tests
- `label_basic`: simple text
- `label_with_fill`: colored text

#### Image Tests
- `image_basic`: placeholder image

### Transforms (Modifiers)

#### Transform Tests
- `transform_scale_2x`: 2x uniform scale
- `transform_rotate_45`: 45deg rotation
- `transform_translate`: 10px offset
- `transform_combined`: scale + rotate

### Animation (Static Snapshots)

#### Easing Curve Tests
- `easing_linear_0.5`: linear at t=0.5
- `easing_ease_in_0.5`: easeIn at t=0.5
- `easing_ease_out_0.5`: easeOut at t=0.5
- `easing_ease_in_out_0.5`: easeInOut at t=0.5

#### Transition Tests
- `transition_opacity_0.5`: opacity transition at 50%
- `transition_scale_0.5`: scale transition at 50%

### Composition

- `nested_stacks`: VStack containing HStack
- `gradient_button`: Rect with linear gradient, rounded corners
- `effect_chain`: Rect with opacity + grayscale
- `card_component`: ZStack with background + content

---

## Edge Cases

### Shapes
- `rect_zero_corner_radius`: cornerRadius=0 (sharp)
- `rect_max_corner_radius`: cornerRadius=min(w,h)/2 (pill)
- `circle_tiny`: 4px diameter (AA test)
- `polygon_many_sides`: 12+ sides (near-circle)
- `rect_zero_dimension`: width=0 or height=0

### Fills
- `gradient_same_colors`: from=to (solid result)
- `gradient_angle_360`: wraps to 0
- `radial_center_edge`: center at (0,0)

### Layout
- `vstack_empty`: no children
- `vstack_single`: one child
- `hstack_overflow`: children exceed container
- `frame_smaller_child`: child doesn't fill frame
- `nested_padding_overflow`: nested containers with padding overflow

### Effects
- `opacity_zero`: fully transparent
- `opacity_one`: no change
- `grayscale_on_grayscale`: no-op test

---

## Coverage

### Critical Paths

1. Shape SDF rendering accuracy
2. Fill sampling at UV boundaries
3. Layout positioning calculations
4. Effect compositing order

### Utilities

- Wrapper generator for each shape type
- Comparison tolerance configuration
- Diff image generation for failures
