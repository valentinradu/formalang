# FormaLang Standard Library Reference

Comprehensive list of all standard library entities with their properties and types.

**Status**: 📋 Design specification (not yet implemented)
**Source**: STDLIB_DESIGN.fv

---

## Table of Contents

- [Marker Traits](#marker-traits)
- [Behavior Traits](#behavior-traits)
- [Category Traits](#category-traits)
- [Layout Components](#layout-components)
- [Content Components](#content-components)
- [Control Components](#control-components)
- [Shape Components](#shape-components)
- [Contour Segments](#contour-segments)
- [Shape Operations](#shape-operations)
- [Layout Constraints](#layout-constraints)
- [Visual Modifiers](#visual-modifiers)
- [Modifier System](#modifier-system)
- [Data Structures](#data-structures)
- [Enums](#enums)
- [Utility Components](#utility-components)

---

## Marker Traits

### View (trait)
- **Type**: `pub trait View {}`
- **Purpose**: Base trait for all visual components
- **Properties**: None (marker trait)

### Shape (trait)
- **Type**: `pub trait Shape {}`
- **Purpose**: Base trait for geometric primitives
- **Properties**: None (marker trait)

### ContourSegment (trait)
- **Type**: `pub trait ContourSegment {}`
- **Purpose**: Base trait for path drawing primitives
- **Properties**: None (marker trait)

---

## Behavior Traits

### Layerable (trait)
- **Type**: `pub trait Layerable`
- **Purpose**: Components that support background and foreground layers
- **Properties**:
  - `background` (mount): View - Background layer
  - `foreground` (mount): View - Foreground layer

---

## Category Traits

### Container (trait)
- **Type**: `pub trait Container: View + Layerable`
- **Inherits**: View + Layerable
- **Properties**:
  - `items` (mount): [View] - Array of child views

### Control (trait)
- **Type**: `pub trait Control: View + Layerable`
- **Inherits**: View + Layerable
- **Properties**:
  - `label` (mount): View - Label/content for the control

### Content (trait)
- **Type**: `pub trait Content: View + Layerable`
- **Inherits**: View + Layerable
- **Properties**: None (composition trait)

---

## Layout Components

### VStack (struct)
- **Type**: `pub struct VStack: Container`
- **Implements**: Container (which extends View + Layerable)
- **Purpose**: Vertical stack layout
- **Properties**:
  - `padding`: Padding - Padding around content (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around stack (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color for descendants (default: nil)
  - `spacing`: Dimension - Space between items (default: .px(value: 0))
  - `alignment`: alignment::Horizontal - Horizontal alignment (default: .center)
  - `distribution`: distribution::Vertical - Vertical distribution (default: .top)
  - `background` (mount): View - Background layer (default: Empty())
  - `items` (mount): [View] - Child views
  - `foreground` (mount): View - Foreground layer (default: Empty())

### HStack (struct)
- **Type**: `pub struct HStack: Container`
- **Implements**: Container (which extends View + Layerable)
- **Purpose**: Horizontal stack layout
- **Properties**:
  - `padding`: Padding - Padding around content (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around stack (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color for descendants (default: nil)
  - `spacing`: Dimension - Space between items (default: .px(value: 0))
  - `alignment`: alignment::Vertical - Vertical alignment (default: .center)
  - `distribution`: distribution::Horizontal - Horizontal distribution (default: .leading)
  - `background` (mount): View - Background layer (default: Empty())
  - `items` (mount): [View] - Child views
  - `foreground` (mount): View - Foreground layer (default: Empty())

### ZStack (struct)
- **Type**: `pub struct ZStack: View + Layerable`
- **Implements**: View + Layerable
- **Purpose**: Layered stack (z-axis)
- **Properties**:
  - `padding`: Padding - Padding around content (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around stack (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color for descendants (default: nil)
  - `spacing`: Dimension - Space between layers (default: .px(value: 0))
  - `alignment`: alignment::Center - Center-point alignment (default: .center)
  - `background` (mount): View - Background layer (default: Empty())
  - `layers` (mount): [View] - Layered views
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Grid (struct)
- **Type**: `pub struct Grid: Container`
- **Implements**: Container (which extends View + Layerable)
- **Purpose**: Grid layout with rows and columns
- **Properties**:
  - `padding`: Padding - Padding around content (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around grid (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color for descendants (default: nil)
  - `spacing`: Dimension - Space between items (default: .px(value: 0))
  - `columns`: [GridItem] - Column definitions
  - `rows`: [GridItem] - Row definitions
  - `alignmentH`: alignment::Horizontal - Horizontal alignment (default: .center)
  - `alignmentV`: alignment::Vertical - Vertical alignment (default: .center)
  - `background` (mount): View - Background layer (default: Empty())
  - `items` (mount): [View] - Grid items
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Scroll (struct)
- **Type**: `pub struct Scroll: View + Layerable`
- **Implements**: View + Layerable
- **Purpose**: Scrollable container
- **Properties**:
  - `axis`: Axis - Scroll direction (default: .vertical)
  - `showsIndicators`: Boolean - Show scroll indicators (default: true)
  - `bounces`: Boolean - Bounce at edges (default: true)
  - `isPagingEnabled`: Boolean - Snap to pages (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `content` (mount): View - Scrollable content
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Spacer (struct)
- **Type**: `pub struct Spacer: View`
- **Implements**: View
- **Purpose**: Flexible spacing element
- **Properties**:
  - `minLength`: Dimension - Minimum length (default: .auto)

---

## Content Components

### Label (struct)
- **Type**: `pub struct Label: Content`
- **Implements**: Content (which extends View + Layerable)
- **Purpose**: Single-line text display
- **Properties**:
  - `padding`: Padding - Padding around text (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around label (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `content`: String - Text content
  - `style`: LabelStyle? - Text styling (default: nil)
  - `truncationMode`: TruncationMode - How to truncate (default: .tail)
  - `fill`: Fill? - Text fill color/gradient (default: nil)
  - `background` (mount): View - Background layer (default: Empty())
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Paragraph (struct)
- **Type**: `pub struct Paragraph: Content`
- **Implements**: Content (which extends View + Layerable)
- **Purpose**: Multi-line text display
- **Properties**:
  - `padding`: Padding - Padding around text (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around paragraph (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `source`: String - Text content
  - `lineLimit`: Number? - Maximum lines (default: nil)
  - `truncationMode`: TruncationMode - How to truncate (default: .tail)
  - `fill`: Fill? - Text fill color/gradient (default: nil)
  - `background` (mount): View - Background layer (default: Empty())
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Image (struct)
- **Type**: `pub struct Image: Content`
- **Implements**: Content (which extends View + Layerable)
- **Purpose**: Image display
- **Properties**:
  - `padding`: Padding - Padding around image (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around image (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `source`: Path - Image file path
  - `renderingMode`: RenderingMode - Rendering style (default: .original)
  - `background` (mount): View - Background layer (default: Empty())
  - `foreground` (mount): View - Foreground layer (default: Empty())

---

## Control Components

### Button (struct)
- **Type**: `pub struct Button: Control`
- **Implements**: Control (which extends View + Layerable)
- **Purpose**: Clickable button control
- **Properties**:
  - `padding`: Padding - Padding around button (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around button (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `label` (mount): View - Button label/content
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Input (struct)
- **Type**: `pub struct Input: Control`
- **Implements**: Control (which extends View + Layerable)
- **Purpose**: Text input field
- **Properties**:
  - `padding`: Padding - Padding around input (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around input (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `caret` (mount): View - Text cursor (default: Empty())
  - `placeholder` (mount): View - Placeholder text (default: Empty())
  - `value` (mount): View - Input value display (default: Empty())
  - `label` (mount): View - Input label
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Checkbox (struct)
- **Type**: `pub struct Checkbox: Control`
- **Implements**: Control (which extends View + Layerable)
- **Purpose**: Checkbox control
- **Properties**:
  - `padding`: Padding - Padding around checkbox (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around checkbox (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `checked`: Boolean - Checked state (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `checkmark` (mount): View - Checkmark indicator (default: Empty())
  - `label` (mount): View - Checkbox label
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Toggle (struct)
- **Type**: `pub struct Toggle: Control`
- **Implements**: Control (which extends View + Layerable)
- **Purpose**: Toggle switch control
- **Properties**:
  - `padding`: Padding - Padding around toggle (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around toggle (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `checked`: Boolean - Toggle state (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `thumb` (mount): View - Toggle thumb/handle (default: Empty())
  - `label` (mount): View - Toggle label
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Slider (struct)
- **Type**: `pub struct Slider: View + Layerable`
- **Implements**: View + Layerable
- **Purpose**: Slider control for numeric values
- **Properties**:
  - `padding`: Padding - Padding around slider (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around slider (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `value`: Number - Current value (default: 0)
  - `min`: Number - Minimum value (default: 0)
  - `max`: Number - Maximum value (default: 100)
  - `step`: Number - Step increment (default: 1)
  - `background` (mount): View - Background layer (default: Empty())
  - `track` (mount): View - Slider track (default: Empty())
  - `thumb` (mount): View - Slider thumb/handle (default: Empty())
  - `foreground` (mount): View - Foreground layer (default: Empty())

### Dropdown (struct)
- **Type**: `pub struct Dropdown: Control`
- **Implements**: Control (which extends View + Layerable)
- **Purpose**: Dropdown/select control
- **Properties**:
  - `padding`: Padding - Padding around dropdown (default: .around(value: .px(value: 0)))
  - `margin`: Margin - Margin around dropdown (default: .around(value: .px(value: 0)))
  - `tint`: Color? - Tint color (default: nil)
  - `disabled`: Boolean - Disabled state (default: false)
  - `background` (mount): View - Background layer (default: Empty())
  - `input` (mount): View - Selected value display (default: Empty())
  - `placeholder` (mount): View - Placeholder text (default: Empty())
  - `indicator` (mount): View - Dropdown indicator (default: Empty())
  - `panel` (mount): View - Options panel (default: Empty())
  - `options` (mount): [View] - Dropdown options
  - `label` (mount): View - Dropdown label
  - `foreground` (mount): View - Foreground layer (default: Empty())

---

## Shape Components

### Rect (struct)
- **Type**: `pub struct Rect: Shape`
- **Implements**: Shape
- **Purpose**: Rectangle shape
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `width`: Dimension - Rectangle width
  - `height`: Dimension - Rectangle height
  - `cornerRadius`: Dimension - Corner radius (default: .px(value: 0))
  - `fill`: Fill? - Fill color/gradient (default: nil)
  - `stroke`: Fill? - Stroke color/gradient (default: nil)

### Circle (struct)
- **Type**: `pub struct Circle: Shape`
- **Implements**: Shape
- **Purpose**: Circle shape
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `radius`: Dimension - Circle radius
  - `fill`: Fill? - Fill color/gradient (default: nil)
  - `stroke`: Fill? - Stroke color/gradient (default: nil)

### Ellipse (struct)
- **Type**: `pub struct Ellipse: Shape`
- **Implements**: Shape
- **Purpose**: Ellipse shape
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `width`: Dimension - Ellipse width
  - `height`: Dimension - Ellipse height
  - `fill`: Fill? - Fill color/gradient (default: nil)
  - `stroke`: Fill? - Stroke color/gradient (default: nil)

### Contour (struct)
- **Type**: `pub struct Contour: Shape`
- **Implements**: Shape
- **Purpose**: Custom vector path shape
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `start`: Point - Starting point
  - `closed`: Boolean - Close the path (default: false)
  - `segments` (mount): [ContourSegment] - Path segments
  - `fill`: Fill? - Fill color/gradient (default: nil)
  - `stroke`: Fill? - Stroke color/gradient (default: nil)

---

## Contour Segments

### Move (struct)
- **Type**: `pub struct Move: ContourSegment`
- **Implements**: ContourSegment
- **Purpose**: Move to a point without drawing
- **Properties**:
  - `to`: Point - Target point

### LineTo (struct)
- **Type**: `pub struct LineTo: ContourSegment`
- **Implements**: ContourSegment
- **Purpose**: Draw a straight line
- **Properties**:
  - `to`: Point - End point

### Arc (struct)
- **Type**: `pub struct Arc: ContourSegment`
- **Implements**: ContourSegment
- **Purpose**: Draw an arc
- **Properties**:
  - `to`: Point - End point
  - `radius`: Number - Arc radius
  - `clockwise`: Boolean - Direction
  - `largeArc`: Boolean - Use large arc

### QuadBezier (struct)
- **Type**: `pub struct QuadBezier: ContourSegment`
- **Implements**: ContourSegment
- **Purpose**: Draw a quadratic Bézier curve
- **Properties**:
  - `to`: Point - End point
  - `control`: Point - Control point

### CubicBezier (struct)
- **Type**: `pub struct CubicBezier: ContourSegment`
- **Implements**: ContourSegment
- **Purpose**: Draw a cubic Bézier curve
- **Properties**:
  - `to`: Point - End point
  - `control1`: Point - First control point
  - `control2`: Point - Second control point

---

## Shape Operations

### ShapeUnion (struct)
- **Type**: `pub struct ShapeUnion: Shape`
- **Implements**: Shape
- **Purpose**: Union of multiple shapes
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `shapes` (mount): [Shape] - Shapes to union

### ShapeIntersection (struct)
- **Type**: `pub struct ShapeIntersection: Shape`
- **Implements**: Shape
- **Purpose**: Intersection of multiple shapes
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `shapes` (mount): [Shape] - Shapes to intersect

### ShapeSubtraction (struct)
- **Type**: `pub struct ShapeSubtraction: Shape`
- **Implements**: Shape
- **Purpose**: Subtract shapes from a base shape
- **Properties**:
  - `tint`: Color? - Tint color (default: nil)
  - `base` (mount): Shape - Base shape
  - `subtract` (mount): [Shape] - Shapes to subtract

---

## Layout Constraints

### Frame (struct)
- **Type**: `pub struct Frame: View`
- **Implements**: View
- **Purpose**: Set explicit size constraints
- **Properties**:
  - `width`: Dimension - Fixed/flexible width (default: .auto)
  - `height`: Dimension - Fixed/flexible height (default: .auto)
  - `alignment`: alignment::Center - Content alignment (default: .center)
  - `content` (mount): View - Framed content

### SizeConstraint (struct)
- **Type**: `pub struct SizeConstraint: View`
- **Implements**: View
- **Purpose**: Set minimum/maximum size constraints
- **Properties**:
  - `minWidth`: Dimension - Minimum width (default: .auto)
  - `maxWidth`: Dimension - Maximum width (default: .auto)
  - `minHeight`: Dimension - Minimum height (default: .auto)
  - `maxHeight`: Dimension - Maximum height (default: .auto)
  - `content` (mount): View - Constrained content

### FixedSize (struct)
- **Type**: `pub struct FixedSize: View`
- **Implements**: View
- **Purpose**: Fix size to content's ideal size
- **Properties**:
  - `horizontal`: Boolean - Fix horizontal size (default: false)
  - `vertical`: Boolean - Fix vertical size (default: false)
  - `content` (mount): View - Fixed content

### AspectRatio (struct)
- **Type**: `pub struct AspectRatio: View`
- **Implements**: View
- **Purpose**: Maintain aspect ratio
- **Properties**:
  - `ratio`: Number - Aspect ratio (width/height)
  - `contentMode`: ContentMode - Fit or fill (default: .fit)
  - `content` (mount): View - Content with aspect ratio

### LayoutPriority (struct)
- **Type**: `pub struct LayoutPriority: View`
- **Implements**: View
- **Purpose**: Set layout priority
- **Properties**:
  - `value`: Number - Priority value (default: 0)
  - `content` (mount): View - Prioritized content

---

## Visual Modifiers

### Offset (struct)
- **Type**: `pub struct Offset: View`
- **Implements**: View
- **Purpose**: Offset position
- **Properties**:
  - `x`: Dimension - Horizontal offset (default: .px(value: 0))
  - `y`: Dimension - Vertical offset (default: .px(value: 0))
  - `content` (mount): View - Offset content

### Clipped (struct)
- **Type**: `pub struct Clipped: View`
- **Implements**: View
- **Purpose**: Clip content to bounds
- **Properties**:
  - `content` (mount): View - Clipped content

### ClipShape (struct)
- **Type**: `pub struct ClipShape: View`
- **Implements**: View
- **Purpose**: Clip content to a shape
- **Properties**:
  - `shape` (mount): Shape - Clipping shape
  - `content` (mount): View - Clipped content

### CornerRadius (struct)
- **Type**: `pub struct CornerRadius: View`
- **Implements**: View
- **Purpose**: Apply corner radius
- **Properties**:
  - `radius`: Dimension - Corner radius
  - `content` (mount): View - Content with rounded corners

---

## Modifier System

### Modifier (struct)
- **Type**: `pub struct Modifier: View`
- **Implements**: View
- **Purpose**: Apply transform/effect operations
- **Properties**:
  - `transforms`: [ModifierOp] - List of operations to apply
  - `mask` (mount): Shape - Optional mask shape
  - `content` (mount): View - Content to modify

---

## Data Structures

### Size (struct)
- **Type**: `pub struct Size`
- **Purpose**: Represent 2D dimensions
- **Properties**:
  - `width`: Dimension - Width dimension
  - `height`: Dimension - Height dimension

### Point (struct)
- **Type**: `pub struct Point`
- **Purpose**: Represent 2D coordinates
- **Properties**:
  - `x`: Number - X coordinate
  - `y`: Number - Y coordinate

### ControlPoint (struct)
- **Type**: `pub struct ControlPoint`
- **Purpose**: FFD (Free Form Deformation) control point
- **Properties**:
  - `gridX`: Number - Grid X position
  - `gridY`: Number - Grid Y position
  - `offset`: Point - Offset from grid position

### LabelStyle (struct)
- **Type**: `pub struct LabelStyle`
- **Purpose**: Text styling for labels
- **Properties**:
  - `font`: Font - Font definition
  - `size`: Dimension - Text size
  - `letterSpacing`: Dimension - Letter spacing
  - `weight`: Weight - Font weight

### Font (struct)
- **Type**: `pub struct Font`
- **Purpose**: Complete font specification
- **Properties**:
  - `family`: String - Font family name
  - `weight`: Weight - Font weight
  - `style`: FontStyle - Font style
  - `stretch`: FontStretch - Font stretch

---

## Enums

### Dimension (enum)
- **Type**: `pub enum Dimension`
- **Purpose**: Flexible or fixed dimensions
- **Variants**:
  - `auto` - Flexible/automatic sizing
  - `px(value: Number)` - Fixed pixel value

### Padding (enum)
- **Type**: `pub enum Padding`
- **Purpose**: Padding specification
- **Variants**:
  - `around(value: Dimension)` - All sides
  - `horizontal(value: Dimension)` - Left and right
  - `vertical(value: Dimension)` - Top and bottom
  - `sides(top: Dimension, right: Dimension, bottom: Dimension, left: Dimension)` - Individual sides

### Margin (enum)
- **Type**: `pub enum Margin`
- **Purpose**: Margin specification
- **Variants**:
  - `around(value: Dimension)` - All sides
  - `horizontal(value: Dimension)` - Left and right
  - `vertical(value: Dimension)` - Top and bottom
  - `sides(top: Dimension, right: Dimension, bottom: Dimension, left: Dimension)` - Individual sides

### Color (enum)
- **Type**: `pub enum Color`
- **Purpose**: Color representation
- **Variants**:
  - `rgb(r: Number, g: Number, b: Number)` - RGB color (0-255) or adjustments (-255 to +255)
  - `hsla(h: Number, s: Number, l: Number, a: Number)` - HSLA color or adjustments
  - `hex(value: String)` - Hex color (e.g., "#FF0000")

### Fill (enum)
- **Type**: `pub enum Fill`
- **Purpose**: Fill/gradient types
- **Variants**:
  - `solid(color: Color)` - Solid color
  - `relativeSolid(color: Color)` - Relative to tint
  - `linearGradient(from: Color, to: Color, angle: Dimension)` - Linear gradient
  - `relativeLinearGradient(from: Color, to: Color, angle: Dimension)` - Relative linear gradient
  - `radialGradient(from: Color, to: Color, centerX: Number, centerY: Number)` - Radial gradient
  - `relativeRadialGradient(from: Color, to: Color, centerX: Number, centerY: Number)` - Relative radial gradient
  - `angularGradient(from: Color, to: Color, angle: Dimension)` - Angular gradient
  - `relativeAngularGradient(from: Color, to: Color, angle: Dimension)` - Relative angular gradient
  - `pattern(source: Fill, size: Size?, repeat: PatternRepeat)` - Pattern fill

### alignment::Horizontal (enum)
- **Type**: `pub enum Horizontal` (in alignment module)
- **Purpose**: Horizontal alignment for VStack
- **Variants**:
  - `leading` - Left alignment
  - `center` - Center alignment
  - `trailing` - Right alignment

### alignment::Vertical (enum)
- **Type**: `pub enum Vertical` (in alignment module)
- **Purpose**: Vertical alignment for HStack
- **Variants**:
  - `top` - Top alignment
  - `center` - Center alignment
  - `bottom` - Bottom alignment

### alignment::Center (enum)
- **Type**: `pub enum Center` (in alignment module)
- **Purpose**: 9-point alignment for ZStack/Frame
- **Variants**:
  - `topLeading`, `top`, `topTrailing`
  - `leading`, `center`, `trailing`
  - `bottomLeading`, `bottom`, `bottomTrailing`

### distribution::Vertical (enum)
- **Type**: `pub enum Vertical` (in distribution module)
- **Purpose**: Vertical distribution
- **Variants**:
  - `top`, `center`, `bottom`
  - `spaceBetween`, `spaceAround`, `spaceEvenly`

### distribution::Horizontal (enum)
- **Type**: `pub enum Horizontal` (in distribution module)
- **Purpose**: Horizontal distribution
- **Variants**:
  - `leading`, `center`, `trailing`
  - `spaceBetween`, `spaceAround`, `spaceEvenly`

### Axis (enum)
- **Type**: `pub enum Axis`
- **Purpose**: Axis direction
- **Variants**:
  - `horizontal` - Horizontal axis
  - `vertical` - Vertical axis

### ContentMode (enum)
- **Type**: `pub enum ContentMode`
- **Purpose**: Aspect ratio content mode
- **Variants**:
  - `fit` - Fit within bounds
  - `fill` - Fill bounds

### PatternRepeat (enum)
- **Type**: `pub enum PatternRepeat`
- **Purpose**: Pattern repeat modes
- **Variants**:
  - `repeat`, `repeatX`, `repeatY`, `noRepeat`

### Weight (enum)
- **Type**: `pub enum Weight`
- **Purpose**: Font weight
- **Variants**:
  - `thin`, `light`, `regular`, `medium`
  - `semibold`, `bold`, `heavy`, `black`

### FontStyle (enum)
- **Type**: `pub enum FontStyle`
- **Purpose**: Font style
- **Variants**:
  - `normal`, `italic`, `oblique`

### FontStretch (enum)
- **Type**: `pub enum FontStretch`
- **Purpose**: Font stretch
- **Variants**:
  - `ultraCondensed`, `extraCondensed`, `condensed`, `semiCondensed`
  - `normal`
  - `semiExpanded`, `expanded`, `extraExpanded`, `ultraExpanded`

### TruncationMode (enum)
- **Type**: `pub enum TruncationMode`
- **Purpose**: Text truncation
- **Variants**:
  - `head` - Truncate at start
  - `middle` - Truncate in middle
  - `tail` - Truncate at end

### RenderingMode (enum)
- **Type**: `pub enum RenderingMode`
- **Purpose**: Image rendering mode
- **Variants**:
  - `original` - Original colors
  - `template` - Use as template (apply tint)

### GridItem (enum)
- **Type**: `pub enum GridItem`
- **Purpose**: Grid row/column definition
- **Variants**:
  - `flexible(min: Dimension, max: Dimension)` - Flexible sizing
  - `adaptive(min: Dimension, max: Dimension)` - Adaptive sizing
  - `fixed(size: Dimension)` - Fixed size

### TransformType (enum)
- **Type**: `pub enum TransformType`
- **Purpose**: Transform operations
- **Variants**:
  - `rotation(angle: Dimension)`
  - `scale(x: Number, y: Number)`
  - `skew(x: Dimension, y: Dimension)`
  - `translation(x: Dimension, y: Dimension)`

### WaveAxis (enum)
- **Type**: `pub enum WaveAxis`
- **Purpose**: Wave modifier axis
- **Variants**:
  - `tangent` - Perpendicular to path
  - `normal` - Along path direction
  - `radial` - From center point

### PointType (enum)
- **Type**: `pub enum PointType`
- **Purpose**: Zig-zag point type
- **Variants**:
  - `smooth`, `corner`, `zigzag`

### SubdivideMethod (enum)
- **Type**: `pub enum SubdivideMethod`
- **Purpose**: Path subdivision method
- **Variants**:
  - `linear`, `smooth`, `adaptive`

### JoinType (enum)
- **Type**: `pub enum JoinType`
- **Purpose**: Path join type
- **Variants**:
  - `miter`, `round`, `bevel`

### MirrorAxis (enum)
- **Type**: `pub enum MirrorAxis`
- **Purpose**: Mirror axis
- **Variants**:
  - `horizontal`, `vertical`
  - `custom(angle: Dimension)`

### StrokeCap (enum)
- **Type**: `pub enum StrokeCap`
- **Purpose**: Stroke cap style
- **Variants**:
  - `butt`, `round`, `square`

### StrokeJoin (enum)
- **Type**: `pub enum StrokeJoin`
- **Purpose**: Stroke join style
- **Variants**:
  - `miter`, `round`, `bevel`

### ModifierOp (enum)
- **Type**: `pub enum ModifierOp`
- **Purpose**: Modifier operations
- **Categories**:

  **Path Modifiers** (apply to ContourSegment/Shape):
  - `wave(amplitude: Number, frequency: Number, phase: Dimension, axis: WaveAxis)`
  - `noise(intensity: Number, scale: Number, seed: Number, octaves: Number)`
  - `ffd(gridX: Number, gridY: Number, controlPoints: [ControlPoint])`
  - `twist(angle: Dimension, center: Point, falloff: Number)`
  - `zigZag(amplitude: Number, frequency: Number, pointType: PointType)`
  - `pucker(amount: Number)`
  - `smooth(tension: Number, iterations: Number)`
  - `subdivide(segments: Number, method: SubdivideMethod)`
  - `simplify(tolerance: Number)`
  - `inflate(amount: Number, segments: Number?)`
  - `offset(distance: Number, joinType: JoinType)`
  - `pathDeform(path: Shape, axis: Axis, stretch: Boolean, rotation: Number)`
  - `envelope(upperBound: Shape, lowerBound: Shape, axis: Axis, strength: Number)`
  - `trimPath(start: Number, end: Number, offset: Number)`
  - `stroke(width: Number, cap: StrokeCap, join: StrokeJoin)`

  **Transform Modifiers** (apply to all View):
  - `scale(x: Number, y: Number, pivot: Point)`
  - `rotate(angle: Dimension, pivot: Point)`
  - `translate(offset: Point)`
  - `skew(angleX: Dimension, angleY: Dimension)`

  **Effect Modifiers** (apply to all View):
  - `blur(radius: Dimension)`
  - `shadow(color: Color, radius: Dimension, x: Dimension, y: Dimension)`
  - `opacity(value: Number)`

  **Arrange Modifiers** (apply to arrays/collections):
  - `array(count: Number, offset: Point, spacing: Dimension)`
  - `radialArray(count: Number, angle: Dimension, radius: Number, radiusMin: Number?, radiusMax: Number?)`
  - `mirror(axis: MirrorAxis, distance: Dimension, keepOriginal: Boolean)`
  - `repeat(copies: Number, offsetPoint: Point, offsetScale: Number, offsetRotation: Dimension, offsetOpacity: Number)`
  - `randomize(positionAmount: Number?, rotationAmount: Dimension?, scaleAmount: Number?, seed: Number)`
  - `waveArrange(amplitude: Number, frequency: Number, axis: Axis)`
  - `ffdArrange(gridX: Number, gridY: Number, controlPoints: [ControlPoint])`

---

## Utility Components

### Empty (struct)
- **Type**: `pub struct Empty: View`
- **Implements**: View
- **Purpose**: Empty placeholder view
- **Properties**: None

---

## Summary Statistics

### Entity Counts by Type

**Structs**: 44 total
- Layout components: 6 (VStack, HStack, ZStack, Grid, Scroll, Spacer)
- Content components: 3 (Label, Paragraph, Image)
- Control components: 6 (Button, Input, Checkbox, Toggle, Slider, Dropdown)
- Shape components: 4 (Rect, Circle, Ellipse, Contour)
- Contour segments: 5 (Move, LineTo, Arc, QuadBezier, CubicBezier)
- Shape operations: 3 (ShapeUnion, ShapeIntersection, ShapeSubtraction)
- Layout constraints: 5 (Frame, SizeConstraint, FixedSize, AspectRatio, LayoutPriority)
- Visual modifiers: 4 (Offset, Clipped, ClipShape, CornerRadius)
- Modifier system: 1 (Modifier)
- Utility components: 1 (Empty)
- Data structures: 5 (Size, Point, ControlPoint, LabelStyle, Font)

**Traits**: 7 total
- Marker traits: 3 (View, Shape, ContourSegment)
- Behavior traits: 1 (Layerable)
- Category traits: 3 (Container, Control, Content)

**Enums**: 26 total
- Core types: 5 (Dimension, Padding, Margin, Color, Fill)
- Alignment types: 3 (alignment::Horizontal, alignment::Vertical, alignment::Center)
- Distribution types: 2 (distribution::Vertical, distribution::Horizontal)
- Styling types: 8 (Weight, FontStyle, FontStretch, TruncationMode, RenderingMode, Axis, ContentMode, PatternRepeat)
- Modifier types: 8 (GridItem, TransformType, WaveAxis, PointType, SubdivideMethod, JoinType, MirrorAxis, StrokeCap, StrokeJoin, ModifierOp)

### Trait Implementation Summary

**Components implementing View only**:
- Spacer, Frame, SizeConstraint, FixedSize, AspectRatio, LayoutPriority, Offset, Clipped, ClipShape, CornerRadius, Modifier, Empty

**Components implementing View + Layerable**:
- ZStack, Scroll, Slider

**Components implementing Container** (extends View + Layerable):
- VStack, HStack, Grid

**Components implementing Control** (extends View + Layerable):
- Button, Input, Checkbox, Toggle, Dropdown

**Components implementing Content** (extends View + Layerable):
- Label, Paragraph, Image

**Components implementing Shape**:
- Rect, Circle, Ellipse, Contour, ShapeUnion, ShapeIntersection, ShapeSubtraction

**Components implementing ContourSegment**:
- Move, LineTo, Arc, QuadBezier, CubicBezier

**Data structures** (no trait implementation):
- Size, Point, ControlPoint, LabelStyle, Font

### ModifierOp Variant Summary

**Total Modifier Operations**: 32
- Path modifiers: 15 (wave, noise, ffd, twist, zigZag, pucker, smooth, subdivide, simplify, inflate, offset, pathDeform, envelope, trimPath, stroke)
- Transform modifiers: 4 (scale, rotate, translate, skew)
- Effect modifiers: 3 (blur, shadow, opacity)
- Arrange modifiers: 7 (array, radialArray, mirror, repeat, randomize, waveArrange, ffdArrange)

---

## Design Principles

1. **Trait Composition**: Components use trait composition (`View + Layerable + Container`) for flexible type hierarchies
2. **Sensible Defaults**: All optional fields have defaults (0, false, nil, Empty())
3. **Mount Points**: Components use mount fields for composition (`mount background: View`)
4. **Relative Colors**: Fill supports both absolute and relative (tint-based) colors
5. **Modifier System**: Centralized Modifier component applies transforms/effects via ModifierOp enum
6. **Layerable Pattern**: Components with background/foreground implement Layerable trait
7. **No Nesting Rules**: Controls cannot nest other Controls; Scroll cannot nest another Scroll

---

**See Also**:
- [STDLIB_DESIGN.fv](../STDLIB_DESIGN.fv) - Full design specification with syntax
- [FEATURES.md](./FEATURES.md) - Implemented language features
