//! WGSL Wrapper Generator
//!
//! Generates WGSL shaders by compiling FormaLang stdlib shapes.
//! This ensures the snapshot tests validate actual FormaLang compilation output.

use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::{compile_with_analyzer_and_resolver, ir::lower_to_ir};
use std::path::PathBuf;

/// Shape-specific field configuration.
///
/// Each variant contains fill, stroke, and shape-specific parameters.
/// - fill_rgba: Optional fill color [r, g, b, a] in 0.0-1.0 range
/// - stroke_rgba: Optional stroke color [r, g, b, a] in 0.0-1.0 range
/// - stroke_width: Stroke width in pixels (default 1.0)
#[derive(Debug, Clone)]
pub enum ShapeFields {
    /// Rectangle with optional corner radius.
    Rect {
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        corner_radius: f32,
        stroke_width: f32,
    },
    /// Circle shape.
    Circle {
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        stroke_width: f32,
    },
    /// Ellipse shape.
    Ellipse {
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        stroke_width: f32,
    },
    /// Regular polygon with n sides.
    Polygon {
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        sides: u32,
        rotation: f32,
        stroke_width: f32,
    },
    /// Line segment from one point to another.
    Line {
        stroke_rgba: [f32; 4],
        from: (f32, f32),
        to: (f32, f32),
        stroke_width: f32,
    },
    /// Custom path defined by segments.
    Contour {
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        segments: Vec<PathSegment>,
        closed: bool,
        stroke_width: f32,
    },
    /// Union of two shapes (boolean OR).
    ShapeUnion {
        shapes: Vec<ShapeFields>,
        fill_rgba: Option<[f32; 4]>,
    },
    /// Intersection of two shapes (boolean AND).
    ShapeIntersection {
        shapes: Vec<ShapeFields>,
        fill_rgba: Option<[f32; 4]>,
    },
    /// Subtraction of shapes (base - subtract).
    ShapeSubtraction {
        base: Box<ShapeFields>,
        subtract: Box<ShapeFields>,
        fill_rgba: Option<[f32; 4]>,
    },
}

/// Path segment for Contour shapes.
#[derive(Debug, Clone)]
pub enum PathSegment {
    /// Straight line to target point.
    LineTo { to: (f32, f32) },
    /// Circular arc to target point.
    Arc {
        to: (f32, f32),
        radius: f32,
        clockwise: bool,
        large_arc: bool,
    },
    /// Quadratic Bezier curve with one control point.
    QuadBezier { to: (f32, f32), control: (f32, f32) },
    /// Cubic Bezier curve with two control points.
    CubicBezier {
        to: (f32, f32),
        control1: (f32, f32),
        control2: (f32, f32),
    },
}

// =============================================================================
// FILL TYPES
// =============================================================================

/// Pattern repeat mode for pattern fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternRepeat {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
}

/// Fill specification for shapes.
#[derive(Debug, Clone)]
pub enum FillSpec {
    /// Solid color fill.
    Solid { rgba: [f32; 4] },
    /// Linear gradient from one color to another.
    Linear {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    },
    /// Radial gradient from center outward.
    Radial {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        center_x: f32,
        center_y: f32,
    },
    /// Angular/conic gradient around a center point.
    Angular {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    },
    /// Tiled pattern using another fill as source.
    Pattern {
        source: Box<FillSpec>,
        width: f32,
        height: f32,
        repeat: PatternRepeat,
    },
    /// Multi-stop linear gradient.
    MultiLinear {
        stops: Vec<([f32; 4], f32)>,
        angle: f32,
    },
    /// Relative linear gradient (0-1 coordinate space).
    RelativeLinear {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    },
    /// Relative radial gradient (0-1 coordinate space).
    RelativeRadial {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        center_x: f32,
        center_y: f32,
    },
    /// Relative solid fill (0-1 coordinate space).
    RelativeSolid {
        rgba: [f32; 4],
    },
    /// Relative angular gradient (0-1 coordinate space).
    RelativeAngular {
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    },
}

/// Specification for rendering a fill on a shape.
#[derive(Debug, Clone)]
pub struct FillRenderSpec {
    pub shape: ShapeFields,
    pub fill: FillSpec,
    pub size: (f32, f32),
}

impl FillRenderSpec {
    pub fn new(shape: ShapeFields, fill: FillSpec, size: (f32, f32)) -> Self {
        Self { shape, fill, size }
    }

    /// Create a rect with linear gradient fill.
    pub fn rect_linear_gradient(
        size: (f32, f32),
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    ) -> Self {
        Self {
            shape: ShapeFields::Rect {
                fill_rgba: None,
                stroke_rgba: None,
                corner_radius: 0.0,
                stroke_width: 1.0,
            },
            fill: FillSpec::Linear {
                from_rgba,
                to_rgba,
                angle,
            },
            size,
        }
    }

    /// Create a rect with radial gradient fill.
    pub fn rect_radial_gradient(
        size: (f32, f32),
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        center_x: f32,
        center_y: f32,
    ) -> Self {
        Self {
            shape: ShapeFields::Rect {
                fill_rgba: None,
                stroke_rgba: None,
                corner_radius: 0.0,
                stroke_width: 1.0,
            },
            fill: FillSpec::Radial {
                from_rgba,
                to_rgba,
                center_x,
                center_y,
            },
            size,
        }
    }

    /// Create a rect with angular gradient fill.
    pub fn rect_angular_gradient(
        size: (f32, f32),
        from_rgba: [f32; 4],
        to_rgba: [f32; 4],
        angle: f32,
    ) -> Self {
        Self {
            shape: ShapeFields::Rect {
                fill_rgba: None,
                stroke_rgba: None,
                corner_radius: 0.0,
                stroke_width: 1.0,
            },
            fill: FillSpec::Angular {
                from_rgba,
                to_rgba,
                angle,
            },
            size,
        }
    }

    /// Format an RGBA color for FormaLang source (0-255 range for RGB, 0-1 for alpha).
    fn format_color(rgba: &[f32; 4]) -> String {
        format!(
            ".rgba(r: {:.1}, g: {:.1}, b: {:.1}, a: {:.6})",
            rgba[0] * 255.0,
            rgba[1] * 255.0,
            rgba[2] * 255.0,
            rgba[3]
        )
    }

    /// Generate FormaLang source for this fill spec.
    fn generate_formalang_source(&self) -> String {
        let common_imports = r#"use stdlib::shapes::*
use stdlib::color::*
use stdlib::dimension::*
use stdlib::gpu::*
use stdlib::traits::*
use stdlib::fill::*
use stdlib::contour::*
"#;
        let fill_expr = match &self.fill {
            FillSpec::Solid { rgba } => {
                format!(".solid(color: {})", Self::format_color(rgba))
            }
            FillSpec::Linear {
                from_rgba,
                to_rgba,
                angle,
            } => {
                format!(
                    ".linear(from: {}, to: {}, angle: {:.1})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    angle
                )
            }
            FillSpec::Radial {
                from_rgba,
                to_rgba,
                center_x,
                center_y,
            } => {
                format!(
                    ".radial(from: {}, to: {}, centerX: {:.2}, centerY: {:.2})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    center_x,
                    center_y
                )
            }
            FillSpec::Angular {
                from_rgba,
                to_rgba,
                angle,
            } => {
                format!(
                    ".angular(from: {}, to: {}, angle: {:.1})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    angle
                )
            }
            FillSpec::Pattern {
                source,
                width,
                height,
                repeat,
            } => {
                let source_expr = match source.as_ref() {
                    FillSpec::Solid { rgba } => {
                        format!(".solid(color: {})", Self::format_color(rgba))
                    }
                    FillSpec::Linear {
                        from_rgba,
                        to_rgba,
                        angle,
                    } => {
                        format!(
                            ".linear(from: {}, to: {}, angle: {:.1})",
                            Self::format_color(from_rgba),
                            Self::format_color(to_rgba),
                            angle
                        )
                    }
                    _ => ".solid(color: .rgba(r: 128.0, g: 128.0, b: 128.0, a: 1.0))".to_string(),
                };
                let repeat_str = match repeat {
                    PatternRepeat::Repeat => ".repeat",
                    PatternRepeat::RepeatX => ".repeatX",
                    PatternRepeat::RepeatY => ".repeatY",
                    PatternRepeat::NoRepeat => ".noRepeat",
                };
                format!(
                    ".pattern(source: {}, width: {:.1}, height: {:.1}, repeat: {})",
                    source_expr, width, height, repeat_str
                )
            }
            FillSpec::MultiLinear { stops, angle } => {
                let stops_str: Vec<String> = stops
                    .iter()
                    .map(|(rgba, pos)| {
                        format!(
                            "ColorStop(color: {}, position: {:.2})",
                            Self::format_color(rgba),
                            pos
                        )
                    })
                    .collect();
                format!(
                    ".multilinear(stops: [{}], angle: {:.1})",
                    stops_str.join(", "),
                    angle
                )
            }
            FillSpec::RelativeLinear {
                from_rgba,
                to_rgba,
                angle,
            } => {
                format!(
                    "fill::relative::Linear(from: {}, to: {}, angle: {:.1})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    angle
                )
            }
            FillSpec::RelativeRadial {
                from_rgba,
                to_rgba,
                center_x,
                center_y,
            } => {
                format!(
                    "fill::relative::Radial(from: {}, to: {}, centerX: {:.2}, centerY: {:.2})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    center_x,
                    center_y
                )
            }
            FillSpec::RelativeSolid { rgba } => {
                format!(
                    "fill::relative::Solid(color: {})",
                    Self::format_color(rgba)
                )
            }
            FillSpec::RelativeAngular {
                from_rgba,
                to_rgba,
                angle,
            } => {
                format!(
                    "fill::relative::Angular(from: {}, to: {}, angle: {:.1})",
                    Self::format_color(from_rgba),
                    Self::format_color(to_rgba),
                    angle
                )
            }
        };

        match &self.shape {
            ShapeFields::Rect { corner_radius, .. } => {
                format!(
                    r#"{common_imports}
let test_shape = Rect(
    cornerRadius: {corner_radius:.6},
    fill: {fill}
)
"#,
                    common_imports = common_imports,
                    corner_radius = corner_radius,
                    fill = fill_expr
                )
            }
            ShapeFields::Circle { .. } => {
                format!(
                    r#"{common_imports}
let test_shape = Circle(
    fill: {fill}
)
"#,
                    common_imports = common_imports,
                    fill = fill_expr
                )
            }
            _ => {
                format!(
                    r#"{common_imports}
let test_shape = Rect(
    cornerRadius: 0.0,
    fill: {fill}
)
"#,
                    common_imports = common_imports,
                    fill = fill_expr
                )
            }
        }
    }

    fn compile_to_wgsl(&self) -> Result<String, String> {
        let source = self.generate_formalang_source();
        let resolver = FileSystemResolver::new(PathBuf::from("."));
        let (ast, analyzer) = compile_with_analyzer_and_resolver(&source, resolver)
            .map_err(|e| format!("Compilation failed: {:?}", e))?;
        let ir_module = lower_to_ir(&ast, analyzer.symbols())
            .map_err(|e| format!("IR lowering failed: {:?}", e))?;
        let wgsl = formalang::codegen::generate_wgsl_with_imports(
            &ir_module,
            analyzer.imported_ir_modules(),
        );
        Ok(wgsl)
    }

    pub fn generate_wgsl(&self) -> String {
        let compiled_wgsl = self
            .compile_to_wgsl()
            .expect("FormaLang compilation should succeed");

        let (width, height) = self.size;

        let entry_points = format!(
            r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
    let color = Rect_render(test_shape, uv, vec2<f32>({:.1}, {:.1}));
    return Color4_to_vec4(color);
}}
"#,
            width, height
        );

        format!("{}\n\n{}", compiled_wgsl, entry_points)
    }
}

// =============================================================================
// SHAPE RENDER SPEC
// =============================================================================

/// Specification for rendering a shape.
#[derive(Debug, Clone)]
pub struct ShapeRenderSpec {
    pub struct_name: String,
    pub size: (f32, f32),
    pub fields: ShapeFields,
}

impl ShapeRenderSpec {
    /// Create a Rect render spec with solid fill.
    pub fn rect(width: f32, height: f32, fill_rgba: [f32; 4], corner_radius: f32) -> Self {
        Self {
            struct_name: "Rect".to_string(),
            size: (width, height),
            fields: ShapeFields::Rect {
                fill_rgba: Some(fill_rgba),
                stroke_rgba: None,
                corner_radius,
                stroke_width: 1.0,
            },
        }
    }

    /// Create a Rect render spec with stroke only.
    pub fn rect_stroke(
        width: f32,
        height: f32,
        stroke_rgba: [f32; 4],
        corner_radius: f32,
        stroke_width: f32,
    ) -> Self {
        Self {
            struct_name: "Rect".to_string(),
            size: (width, height),
            fields: ShapeFields::Rect {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                corner_radius,
                stroke_width,
            },
        }
    }

    /// Create a Rect render spec with both fill and stroke.
    pub fn rect_with_stroke(
        width: f32,
        height: f32,
        fill_rgba: [f32; 4],
        stroke_rgba: [f32; 4],
        corner_radius: f32,
        stroke_width: f32,
    ) -> Self {
        Self {
            struct_name: "Rect".to_string(),
            size: (width, height),
            fields: ShapeFields::Rect {
                fill_rgba: Some(fill_rgba),
                stroke_rgba: Some(stroke_rgba),
                corner_radius,
                stroke_width,
            },
        }
    }

    /// Create a Circle render spec with solid fill.
    pub fn circle(diameter: f32, fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Circle".to_string(),
            size: (diameter, diameter),
            fields: ShapeFields::Circle {
                fill_rgba: Some(fill_rgba),
                stroke_rgba: None,
                stroke_width: 1.0,
            },
        }
    }

    /// Create a Circle render spec with stroke only.
    pub fn circle_stroke(diameter: f32, stroke_rgba: [f32; 4], stroke_width: f32) -> Self {
        Self {
            struct_name: "Circle".to_string(),
            size: (diameter, diameter),
            fields: ShapeFields::Circle {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                stroke_width,
            },
        }
    }

    /// Create an Ellipse render spec with solid fill.
    pub fn ellipse(width: f32, height: f32, fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Ellipse".to_string(),
            size: (width, height),
            fields: ShapeFields::Ellipse {
                fill_rgba: Some(fill_rgba),
                stroke_rgba: None,
                stroke_width: 1.0,
            },
        }
    }

    /// Create an Ellipse render spec with stroke only.
    pub fn ellipse_stroke(width: f32, height: f32, stroke_rgba: [f32; 4], stroke_width: f32) -> Self {
        Self {
            struct_name: "Ellipse".to_string(),
            size: (width, height),
            fields: ShapeFields::Ellipse {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                stroke_width,
            },
        }
    }

    /// Create a Polygon render spec.
    pub fn polygon(
        size: (f32, f32),
        fill_rgba: Option<[f32; 4]>,
        sides: u32,
        rotation: f32,
    ) -> Self {
        Self {
            struct_name: "Polygon".to_string(),
            size,
            fields: ShapeFields::Polygon {
                fill_rgba,
                stroke_rgba: None,
                sides,
                rotation,
                stroke_width: 1.0,
            },
        }
    }

    /// Create a Polygon render spec with stroke.
    pub fn polygon_with_stroke(
        size: (f32, f32),
        fill_rgba: Option<[f32; 4]>,
        stroke_rgba: Option<[f32; 4]>,
        sides: u32,
        rotation: f32,
        stroke_width: f32,
    ) -> Self {
        Self {
            struct_name: "Polygon".to_string(),
            size,
            fields: ShapeFields::Polygon {
                fill_rgba,
                stroke_rgba,
                sides,
                rotation,
                stroke_width,
            },
        }
    }

    /// Create a Line render spec.
    pub fn line(
        from: (f32, f32),
        to: (f32, f32),
        stroke_rgba: [f32; 4],
        stroke_width: f32,
    ) -> Self {
        let width = (to.0 - from.0).abs().max(stroke_width * 2.0) + 20.0;
        let height = (to.1 - from.1).abs().max(stroke_width * 2.0) + 20.0;
        Self {
            struct_name: "Line".to_string(),
            size: (width.max(100.0), height.max(100.0)),
            fields: ShapeFields::Line {
                stroke_rgba,
                from,
                to,
                stroke_width,
            },
        }
    }

    /// Create a Contour render spec for a triangle (stroke only).
    pub fn contour_triangle(size: (f32, f32), stroke_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Contour".to_string(),
            size,
            fields: ShapeFields::Contour {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                segments: vec![
                    PathSegment::LineTo { to: (90.0, 10.0) },
                    PathSegment::LineTo { to: (50.0, 90.0) },
                ],
                closed: true,
                stroke_width: 3.0,
            },
        }
    }

    /// Create a Contour render spec with open stroke.
    pub fn contour_open_stroke(size: (f32, f32), stroke_rgba: [f32; 4], stroke_width: f32) -> Self {
        Self {
            struct_name: "Contour".to_string(),
            size,
            fields: ShapeFields::Contour {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                segments: vec![
                    PathSegment::LineTo { to: (50.0, 10.0) },
                    PathSegment::LineTo { to: (90.0, 50.0) },
                    PathSegment::LineTo { to: (50.0, 90.0) },
                ],
                closed: false,
                stroke_width,
            },
        }
    }

    /// Create a Contour render spec with quadratic Bezier (stroke only).
    pub fn contour_quad_bezier(size: (f32, f32), stroke_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Contour".to_string(),
            size,
            fields: ShapeFields::Contour {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                segments: vec![
                    PathSegment::QuadBezier {
                        to: (90.0, 50.0),
                        control: (50.0, 10.0),
                    },
                    PathSegment::LineTo { to: (50.0, 90.0) },
                ],
                closed: true,
                stroke_width: 3.0,
            },
        }
    }

    /// Create a Contour render spec with cubic Bezier (stroke only).
    pub fn contour_cubic_bezier(size: (f32, f32), stroke_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Contour".to_string(),
            size,
            fields: ShapeFields::Contour {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                segments: vec![
                    PathSegment::CubicBezier {
                        to: (90.0, 90.0),
                        control1: (30.0, 10.0),
                        control2: (70.0, 90.0),
                    },
                    PathSegment::LineTo { to: (10.0, 50.0) },
                ],
                closed: true,
                stroke_width: 3.0,
            },
        }
    }

    /// Create a Contour render spec with arc (stroke only).
    pub fn contour_arc(size: (f32, f32), stroke_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Contour".to_string(),
            size,
            fields: ShapeFields::Contour {
                fill_rgba: None,
                stroke_rgba: Some(stroke_rgba),
                segments: vec![
                    PathSegment::Arc {
                        to: (70.0, 10.0),
                        radius: 50.0,
                        clockwise: false,
                        large_arc: false,
                    },
                    PathSegment::LineTo { to: (90.0, 90.0) },
                    PathSegment::LineTo { to: (10.0, 90.0) },
                ],
                closed: true,
                stroke_width: 3.0,
            },
        }
    }

    /// Create a union of two circles.
    pub fn shape_union_circles(size: (f32, f32), fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "ShapeUnion".to_string(),
            size,
            fields: ShapeFields::ShapeUnion {
                shapes: vec![
                    ShapeFields::Circle {
                        fill_rgba: Some(fill_rgba),
                        stroke_rgba: None,
                        stroke_width: 1.0,
                    },
                    ShapeFields::Circle {
                        fill_rgba: Some(fill_rgba),
                        stroke_rgba: None,
                        stroke_width: 1.0,
                    },
                ],
                fill_rgba: Some(fill_rgba),
            },
        }
    }

    /// Create an intersection of rect and circle.
    pub fn shape_intersection_rect_circle(size: (f32, f32), fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "ShapeIntersection".to_string(),
            size,
            fields: ShapeFields::ShapeIntersection {
                shapes: vec![
                    ShapeFields::Rect {
                        fill_rgba: Some(fill_rgba),
                        stroke_rgba: None,
                        corner_radius: 0.0,
                        stroke_width: 1.0,
                    },
                    ShapeFields::Circle {
                        fill_rgba: Some(fill_rgba),
                        stroke_rgba: None,
                        stroke_width: 1.0,
                    },
                ],
                fill_rgba: Some(fill_rgba),
            },
        }
    }

    /// Create a subtraction of circle from rect.
    pub fn shape_subtraction_rect_minus_circle(size: (f32, f32), fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "ShapeSubtraction".to_string(),
            size,
            fields: ShapeFields::ShapeSubtraction {
                base: Box::new(ShapeFields::Rect {
                    fill_rgba: Some(fill_rgba),
                    stroke_rgba: None,
                    corner_radius: 0.0,
                    stroke_width: 1.0,
                }),
                subtract: Box::new(ShapeFields::Circle {
                    fill_rgba: None,
                    stroke_rgba: None,
                    stroke_width: 1.0,
                }),
                fill_rgba: Some(fill_rgba),
            },
        }
    }

    /// Format an RGBA color for FormaLang source.
    fn format_color(rgba: &[f32; 4]) -> String {
        format!(
            ".rgba(r: {:.1}, g: {:.1}, b: {:.1}, a: {:.6})",
            rgba[0] * 255.0,
            rgba[1] * 255.0,
            rgba[2] * 255.0,
            rgba[3]
        )
    }

    /// Format an optional fill.
    fn format_optional_fill(rgba: Option<&[f32; 4]>) -> String {
        match rgba {
            Some(c) => format!(".solid(color: {})", Self::format_color(c)),
            None => "nil".to_string(),
        }
    }

    /// Format an optional stroke.
    fn format_optional_stroke(rgba: Option<&[f32; 4]>) -> String {
        match rgba {
            Some(c) => format!(".solid(color: {})", Self::format_color(c)),
            None => "nil".to_string(),
        }
    }

    /// Generate FormaLang source code for this shape.
    pub fn generate_formalang_source(&self) -> String {
        let common_imports = r#"use stdlib::shapes::*
use stdlib::color::*
use stdlib::dimension::*
use stdlib::gpu::*
use stdlib::traits::*
use stdlib::fill::*
use stdlib::contour::*
"#;
        match &self.fields {
            ShapeFields::Rect {
                fill_rgba,
                stroke_rgba,
                corner_radius,
                stroke_width,
            } => {
                let fill_line = match fill_rgba {
                    Some(c) => format!("    fill: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let stroke_line = match stroke_rgba {
                    Some(c) => format!("    stroke: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                format!(
                    r#"{common_imports}
let test_shape = Rect(
    cornerRadius: {corner_radius:.6},
{fill_line}
{stroke_line}
    strokeWidth: {stroke_width:.1}
)
"#,
                    common_imports = common_imports,
                    corner_radius = corner_radius,
                    fill_line = fill_line,
                    stroke_line = stroke_line,
                    stroke_width = stroke_width
                )
            }
            ShapeFields::Circle {
                fill_rgba,
                stroke_rgba,
                stroke_width,
            } => {
                let fill_line = match fill_rgba {
                    Some(c) => format!("    fill: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let stroke_line = match stroke_rgba {
                    Some(c) => format!("    stroke: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                format!(
                    r#"{common_imports}
let test_shape = Circle(
{fill_line}
{stroke_line}
    strokeWidth: {stroke_width:.1}
)
"#,
                    common_imports = common_imports,
                    fill_line = fill_line,
                    stroke_line = stroke_line,
                    stroke_width = stroke_width
                )
            }
            ShapeFields::Ellipse {
                fill_rgba,
                stroke_rgba,
                stroke_width,
            } => {
                let fill_line = match fill_rgba {
                    Some(c) => format!("    fill: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let stroke_line = match stroke_rgba {
                    Some(c) => format!("    stroke: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                format!(
                    r#"{common_imports}
let test_shape = Ellipse(
{fill_line}
{stroke_line}
    strokeWidth: {stroke_width:.1}
)
"#,
                    common_imports = common_imports,
                    fill_line = fill_line,
                    stroke_line = stroke_line,
                    stroke_width = stroke_width
                )
            }
            ShapeFields::Polygon {
                fill_rgba,
                stroke_rgba,
                sides,
                rotation,
                stroke_width,
            } => {
                let fill_line = match fill_rgba {
                    Some(c) => format!("    fill: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let stroke_line = match stroke_rgba {
                    Some(c) => format!("    stroke: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                format!(
                    r#"{common_imports}
let test_shape = Polygon(
    sides: {sides}u,
    rotation: {rotation:.1},
{fill_line}
{stroke_line}
    strokeWidth: {stroke_width:.1}
)
"#,
                    common_imports = common_imports,
                    sides = sides,
                    rotation = rotation,
                    fill_line = fill_line,
                    stroke_line = stroke_line,
                    stroke_width = stroke_width
                )
            }
            ShapeFields::Line {
                stroke_rgba,
                from,
                to,
                stroke_width,
            } => {
                let stroke = Self::format_color(stroke_rgba);
                format!(
                    r#"{common_imports}
let test_shape = Line(
    from: Point(x: {:.1}, y: {:.1}),
    to: Point(x: {:.1}, y: {:.1}),
    stroke: .solid(color: {stroke}),
    strokeWidth: {stroke_width:.1}
)
"#,
                    from.0,
                    from.1,
                    to.0,
                    to.1,
                    stroke = stroke,
                    stroke_width = stroke_width,
                    common_imports = common_imports
                )
            }
            ShapeFields::Contour {
                fill_rgba,
                stroke_rgba,
                segments,
                closed,
                stroke_width,
            } => {
                let fill_line = match fill_rgba {
                    Some(c) => format!("    fill: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let stroke_line = match stroke_rgba {
                    Some(c) => format!("    stroke: .solid(color: {}),", Self::format_color(c)),
                    None => String::new(),
                };
                let segments_str: Vec<String> = segments
                    .iter()
                    .map(|seg| match seg {
                        PathSegment::LineTo { to } => {
                            format!("        LineTo(to: Point(x: {:.1}, y: {:.1}))", to.0, to.1)
                        }
                        PathSegment::Arc {
                            to,
                            radius,
                            clockwise,
                            large_arc,
                        } => {
                            format!(
                                "        Arc(to: Point(x: {:.1}, y: {:.1}), radius: {:.1}, clockwise: {}, largeArc: {})",
                                to.0, to.1, radius, clockwise, large_arc
                            )
                        }
                        PathSegment::QuadBezier { to, control } => {
                            format!(
                                "        QuadBezier(to: Point(x: {:.1}, y: {:.1}), control: Point(x: {:.1}, y: {:.1}))",
                                to.0, to.1, control.0, control.1
                            )
                        }
                        PathSegment::CubicBezier {
                            to,
                            control1,
                            control2,
                        } => {
                            format!(
                                "        CubicBezier(to: Point(x: {:.1}, y: {:.1}), control1: Point(x: {:.1}, y: {:.1}), control2: Point(x: {:.1}, y: {:.1}))",
                                to.0, to.1, control1.0, control1.1, control2.0, control2.1
                            )
                        }
                    })
                    .collect();
                format!(
                    r#"{common_imports}
let test_shape = Contour(
    start: Point(x: 10.0, y: 10.0),
    closed: {closed},
{fill_line}
{stroke_line}
    strokeWidth: {stroke_width:.1}
) {{
    segments: {{
{segments}
    }}
}}
"#,
                    common_imports = common_imports,
                    segments = segments_str.join("\n"),
                    closed = closed,
                    fill_line = fill_line,
                    stroke_line = stroke_line,
                    stroke_width = stroke_width
                )
            }
            ShapeFields::ShapeUnion { fill_rgba, .. }
            | ShapeFields::ShapeIntersection { fill_rgba, .. }
            | ShapeFields::ShapeSubtraction { fill_rgba, .. } => {
                // Boolean shapes not yet implemented
                let _fill = Self::format_optional_fill(fill_rgba.as_ref());
                format!(
                    r#"{common_imports}
let test_shape = Rect(cornerRadius: 0.0, fill: .solid(color: .rgba(r: 128.0, g: 128.0, b: 128.0, a: 1.0)))
"#,
                    common_imports = common_imports
                )
            }
        }
    }

    /// Compile FormaLang source to WGSL.
    pub fn compile_to_wgsl(&self) -> Result<String, String> {
        let source = self.generate_formalang_source();

        let resolver = FileSystemResolver::new(PathBuf::from("."));

        let (ast, analyzer) = compile_with_analyzer_and_resolver(&source, resolver)
            .map_err(|e| format!("Compilation failed: {:?}", e))?;

        let ir_module = lower_to_ir(&ast, analyzer.symbols())
            .map_err(|e| format!("IR lowering failed: {:?}", e))?;

        let wgsl = formalang::codegen::generate_wgsl_with_imports(
            &ir_module,
            analyzer.imported_ir_modules(),
        );

        Ok(wgsl)
    }

    /// Generate complete WGSL shader with entry points.
    pub fn generate_wgsl(&self) -> String {
        let compiled_wgsl = self
            .compile_to_wgsl()
            .expect("FormaLang compilation should succeed");

        let entry_points = self.generate_entry_points();

        format!("{}\n\n{}", compiled_wgsl, entry_points)
    }

    /// Generate vertex/fragment entry points that use the compiled shape functions.
    fn generate_entry_points(&self) -> String {
        let shape_name = &self.struct_name;
        let (width, height) = self.size;

        match &self.fields {
            ShapeFields::Circle { .. } => {
                let radius = width / 2.0;
                format!(
                    r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
    let color = Circle_render(test_shape, uv, {:.1});
    return Color4_to_vec4(color);
}}
"#,
                    radius
                )
            }
            ShapeFields::Line { from, to, .. } => {
                format!(
                    r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
    let size = vec2<f32>({:.1}, {:.1});
    let color = Line_render(test_shape, uv, size);
    return Color4_to_vec4(color);
}}
"#,
                    width, height
                )
            }
            ShapeFields::Contour {
                segments,
                closed,
                fill_rgba,
                ..
            } => {
                // Generate SDF computation for each segment
                let mut sdf_code = String::new();
                sdf_code.push_str(&format!(
                    "    let size = vec2<f32>({:.1}, {:.1});\n",
                    width, height
                ));
                sdf_code.push_str("    let point = uv * size;\n");
                sdf_code.push_str("    let start = vec2<f32>(10.0, 10.0);\n\n");
                sdf_code.push_str("    // Compute segment SDFs\n");

                // For filled closed contours, we need inside/outside detection
                let needs_inside_test = *closed && fill_rgba.is_some();

                let mut prev_end = "start".to_string();
                for (i, seg) in segments.iter().enumerate() {
                    let seg_var = format!("seg{}", i);
                    let sdf_var = format!("sdf{}", i);
                    let end_var = format!("end{}", i);
                    match seg {
                        PathSegment::LineTo { to } => {
                            // WGSL struct constructors use positional arguments
                            sdf_code.push_str(&format!(
                                "    let {} = LineTo(Point({:.1}, {:.1}));\n",
                                seg_var, to.0, to.1
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = LineTo_sdf({}, point, {});\n",
                                sdf_var, seg_var, prev_end
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = vec2<f32>({:.1}, {:.1});\n",
                                end_var, to.0, to.1
                            ));
                        }
                        PathSegment::Arc {
                            to,
                            radius,
                            clockwise,
                            large_arc,
                        } => {
                            sdf_code.push_str(&format!(
                                "    let {} = Arc(Point({:.1}, {:.1}), {:.1}, {}, {});\n",
                                seg_var, to.0, to.1, radius, clockwise, large_arc
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = Arc_sdf({}, point, {});\n",
                                sdf_var, seg_var, prev_end
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = vec2<f32>({:.1}, {:.1});\n",
                                end_var, to.0, to.1
                            ));
                        }
                        PathSegment::QuadBezier { to, control } => {
                            sdf_code.push_str(&format!(
                                "    let {} = QuadBezier(Point({:.1}, {:.1}), Point({:.1}, {:.1}));\n",
                                seg_var, to.0, to.1, control.0, control.1
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = QuadBezier_sdf({}, point, {});\n",
                                sdf_var, seg_var, prev_end
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = vec2<f32>({:.1}, {:.1});\n",
                                end_var, to.0, to.1
                            ));
                        }
                        PathSegment::CubicBezier {
                            to,
                            control1,
                            control2,
                        } => {
                            sdf_code.push_str(&format!(
                                "    let {} = CubicBezier(Point({:.1}, {:.1}), Point({:.1}, {:.1}), Point({:.1}, {:.1}));\n",
                                seg_var, to.0, to.1, control1.0, control1.1, control2.0, control2.1
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = CubicBezier_sdf({}, point, {});\n",
                                sdf_var, seg_var, prev_end
                            ));
                            sdf_code.push_str(&format!(
                                "    let {} = vec2<f32>({:.1}, {:.1});\n",
                                end_var, to.0, to.1
                            ));
                        }
                    }
                    prev_end = end_var;
                }

                // Add closing segment if needed
                let last_end = if segments.is_empty() {
                    "start".to_string()
                } else {
                    format!("end{}", segments.len() - 1)
                };
                sdf_code.push_str(
                    "    let close_seg = LineTo(Point(10.0, 10.0));\n"
                );
                sdf_code.push_str(&format!(
                    "    let sdf_close = LineTo_sdf(close_seg, point, {});\n\n",
                    last_end
                ));

                // Compute minimum SDF
                sdf_code.push_str("    // Find minimum distance\n");
                sdf_code.push_str("    var min_dist = sdf0;\n");
                for i in 1..segments.len() {
                    sdf_code.push_str(&format!("    min_dist = min(min_dist, sdf{});\n", i));
                }
                sdf_code.push_str("    min_dist = min(min_dist, sdf_close);\n\n");

                // For filled closed contours, add inside/outside detection using crossing number
                if needs_inside_test {
                    // Collect all vertices for the crossing number test
                    // For curves, we sample multiple points to approximate the path
                    let mut vertices = vec![(10.0_f32, 10.0_f32)]; // start
                    let mut current_pos = (10.0_f32, 10.0_f32);

                    for seg in segments.iter() {
                        match seg {
                            PathSegment::LineTo { to } => {
                                vertices.push(*to);
                                current_pos = *to;
                            }
                            PathSegment::Arc { to, radius, clockwise, large_arc } => {
                                // Sample arc at multiple points
                                let samples = 16;
                                let (x0, y0) = current_pos;
                                let (x1, y1) = *to;
                                let r = *radius;

                                // Find arc center (simplified - assumes circular arc)
                                let dx = x1 - x0;
                                let dy = y1 - y0;
                                let d = (dx * dx + dy * dy).sqrt();
                                let h = (r * r - (d / 2.0) * (d / 2.0)).max(0.0).sqrt();
                                let mx = (x0 + x1) / 2.0;
                                let my = (y0 + y1) / 2.0;
                                let sign = if *clockwise != *large_arc { 1.0 } else { -1.0 };
                                let cx = mx + sign * h * (-dy) / d;
                                let cy = my + sign * h * dx / d;

                                let start_angle = (y0 - cy).atan2(x0 - cx);
                                let end_angle = (y1 - cy).atan2(x1 - cx);

                                for i in 1..=samples {
                                    let t = i as f32 / samples as f32;
                                    let angle = start_angle + t * (end_angle - start_angle);
                                    let px = cx + r * angle.cos();
                                    let py = cy + r * angle.sin();
                                    vertices.push((px, py));
                                }
                                current_pos = *to;
                            }
                            PathSegment::QuadBezier { to, control } => {
                                // Sample quadratic bezier at multiple points
                                let samples = 8;
                                let p0 = current_pos;
                                let p1 = *control;
                                let p2 = *to;

                                for i in 1..=samples {
                                    let t = i as f32 / samples as f32;
                                    let omt = 1.0 - t;
                                    let px = omt * omt * p0.0 + 2.0 * omt * t * p1.0 + t * t * p2.0;
                                    let py = omt * omt * p0.1 + 2.0 * omt * t * p1.1 + t * t * p2.1;
                                    vertices.push((px, py));
                                }
                                current_pos = *to;
                            }
                            PathSegment::CubicBezier { to, control1, control2 } => {
                                // Sample cubic bezier at multiple points
                                let samples = 12;
                                let p0 = current_pos;
                                let p1 = *control1;
                                let p2 = *control2;
                                let p3 = *to;

                                for i in 1..=samples {
                                    let t = i as f32 / samples as f32;
                                    let omt = 1.0 - t;
                                    let omt2 = omt * omt;
                                    let omt3 = omt2 * omt;
                                    let t2 = t * t;
                                    let t3 = t2 * t;
                                    let px = omt3 * p0.0 + 3.0 * omt2 * t * p1.0 + 3.0 * omt * t2 * p2.0 + t3 * p3.0;
                                    let py = omt3 * p0.1 + 3.0 * omt2 * t * p1.1 + 3.0 * omt * t2 * p2.1 + t3 * p3.1;
                                    vertices.push((px, py));
                                }
                                current_pos = *to;
                            }
                        }
                    }

                    sdf_code.push_str("    // Inside/outside test using crossing number algorithm\n");
                    sdf_code.push_str("    var crossings: i32 = 0;\n");

                    // Generate crossing test for each edge
                    for i in 0..vertices.len() {
                        let (x1, y1) = vertices[i];
                        let (x2, y2) = vertices[(i + 1) % vertices.len()];
                        sdf_code.push_str(&format!(
                            "    // Edge {} -> {}\n    {{\n",
                            i,
                            (i + 1) % vertices.len()
                        ));
                        sdf_code.push_str(&format!(
                            "        let v1 = vec2<f32>({:.1}, {:.1});\n",
                            x1, y1
                        ));
                        sdf_code.push_str(&format!(
                            "        let v2 = vec2<f32>({:.1}, {:.1});\n",
                            x2, y2
                        ));
                        // Ray casting: check if horizontal ray from point crosses edge
                        sdf_code.push_str(
                            "        let y_in_range = (v1.y <= point.y && point.y < v2.y) || (v2.y <= point.y && point.y < v1.y);\n",
                        );
                        sdf_code.push_str(
                            "        if (y_in_range) {\n",
                        );
                        sdf_code.push_str(
                            "            let x_intersect = v1.x + (point.y - v1.y) / (v2.y - v1.y) * (v2.x - v1.x);\n",
                        );
                        sdf_code.push_str(
                            "            if (point.x < x_intersect) {\n                crossings = crossings + 1;\n            }\n",
                        );
                        sdf_code.push_str("        }\n    }\n");
                    }

                    sdf_code.push_str(
                        "    // If odd crossings, point is inside (negate distance)\n",
                    );
                    sdf_code.push_str(
                        "    let signed_dist = select(min_dist, -min_dist, (crossings % 2) == 1);\n\n",
                    );
                    sdf_code.push_str(
                        "    let color = Contour_render(test_shape, uv, size, signed_dist);\n",
                    );
                } else {
                    sdf_code.push_str(
                        "    let color = Contour_render(test_shape, uv, size, min_dist);\n",
                    );
                }

                format!(
                    r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
{sdf_code}
    return Color4_to_vec4(color);
}}
"#,
                    sdf_code = sdf_code
                )
            }
            ShapeFields::Polygon { .. } => {
                // Polygon::render takes (self, uv: vec2, resolved_radius: f32)
                let radius = width / 2.0;
                format!(
                    r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
    let color = Polygon_render(test_shape, uv, {:.1});
    return Color4_to_vec4(color);
}}
"#,
                    radius
                )
            }
            _ => {
                // Default: Rect, Ellipse
                format!(
                    r#"
struct VertexOutput {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {{
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{
    let uv = in.uv;
    let color = {}_render(test_shape, uv, vec2<f32>({:.1}, {:.1}));
    return Color4_to_vec4(color);
}}
"#,
                    shape_name, width, height
                )
            }
        }
    }
}

// =============================================================================
// STUB TYPES FOR OTHER TEST FILES
// =============================================================================

/// Animation easing type (stub).
#[derive(Debug, Clone, Copy)]
pub enum EasingType {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// Animated property (stub).
#[derive(Debug, Clone)]
pub enum AnimatedProperty {
    Opacity,
    Scale,
}

/// Animation snapshot spec (stub).
#[derive(Debug, Clone)]
pub struct AnimationSnapshotSpec;

impl AnimationSnapshotSpec {
    pub fn new(_easing: EasingType, _t: f32, _property: AnimatedProperty, _size: (f32, f32)) -> Self {
        Self
    }
}

/// Container type for layouts (stub).
#[derive(Debug, Clone)]
pub enum ContainerType {
    HStack,
    VStack,
    ZStack,
    Frame,
}

/// Layout alignment types (stubs).
#[derive(Debug, Clone, Copy)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
pub enum HorizontalAlignment {
    Leading,
    Center,
    Trailing,
}

#[derive(Debug, Clone, Copy)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy)]
pub enum CenterAlignment {
    Center,
}

/// Layout render spec (stub).
#[derive(Debug, Clone)]
pub struct LayoutRenderSpec;

impl LayoutRenderSpec {
    pub fn new(_container: ContainerType, _children: Vec<ShapeRenderSpec>, _size: (f32, f32)) -> Self {
        Self
    }
}

/// Effect types (stubs).
#[derive(Debug, Clone)]
pub enum EffectSpec {
    Blur { radius: f32 },
    Opacity { value: f32 },
}

#[derive(Debug, Clone, Copy)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
}

/// Effect render spec (stub).
#[derive(Debug, Clone)]
pub struct EffectRenderSpec;

impl EffectRenderSpec {
    pub fn new(_shape: ShapeRenderSpec, _effects: Vec<EffectSpec>, _size: (f32, f32)) -> Self {
        Self
    }
}

/// Content types (stubs).
#[derive(Debug, Clone)]
pub enum ContentSpec {
    Label { text: String },
    Image { path: String },
}

/// Content render spec (stub).
#[derive(Debug, Clone)]
pub struct ContentRenderSpec;

impl ContentRenderSpec {
    pub fn new(_content: ContentSpec, _size: (f32, f32)) -> Self {
        Self
    }
}

/// Transform types (stubs).
#[derive(Debug, Clone)]
pub enum TransformSpec {
    Translate { x: f32, y: f32 },
    Rotate { angle: f32 },
    Scale { x: f32, y: f32 },
}

/// Transform render spec (stub).
#[derive(Debug, Clone)]
pub struct TransformRenderSpec;

impl TransformRenderSpec {
    pub fn new(_shape: ShapeRenderSpec, _transforms: Vec<TransformSpec>, _size: (f32, f32)) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_formalang_source() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let source = spec.generate_formalang_source();
        assert!(source.contains("use stdlib::shapes::*"));
        assert!(source.contains("test_shape = Rect"));
    }

    #[test]
    fn test_circle_formalang_source() {
        let spec = ShapeRenderSpec::circle(100.0, [0.0, 1.0, 0.0, 1.0]);
        let source = spec.generate_formalang_source();
        assert!(source.contains("test_shape = Circle"));
    }

    #[test]
    fn test_ellipse_formalang_source() {
        let spec = ShapeRenderSpec::ellipse(120.0, 80.0, [0.0, 0.0, 1.0, 1.0]);
        let source = spec.generate_formalang_source();
        assert!(source.contains("test_shape = Ellipse"));
    }

    #[test]
    fn test_rect_formalang_compiles() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let result = spec.compile_to_wgsl();
        assert!(result.is_ok(), "Should compile: {:?}", result.err());
    }

    #[test]
    fn test_circle_formalang_compiles() {
        let spec = ShapeRenderSpec::circle(100.0, [0.0, 1.0, 0.0, 1.0]);
        let result = spec.compile_to_wgsl();
        assert!(result.is_ok(), "Should compile: {:?}", result.err());
    }

    #[test]
    fn test_ellipse_formalang_compiles() {
        let spec = ShapeRenderSpec::ellipse(120.0, 80.0, [0.0, 0.0, 1.0, 1.0]);
        let result = spec.compile_to_wgsl();
        assert!(result.is_ok(), "Should compile: {:?}", result.err());
    }

    #[test]
    fn test_generate_wgsl_includes_entry_points() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let wgsl = spec.generate_wgsl();
        assert!(wgsl.contains("@vertex"));
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("Rect_render"));
    }

    #[test]
    fn test_pattern_wgsl_output() {
        let spec = FillRenderSpec::new(
            ShapeFields::Rect {
                fill_rgba: None,
                stroke_rgba: None,
                corner_radius: 0.0,
                stroke_width: 1.0,
            },
            FillSpec::Pattern {
                source: Box::new(FillSpec::Linear {
                    from_rgba: [1.0, 0.0, 0.0, 1.0],
                    to_rgba: [0.0, 0.0, 1.0, 1.0],
                    angle: 45.0,
                }),
                width: 4.0,
                height: 4.0,
                repeat: PatternRepeat::Repeat,
            },
            (100.0, 100.0),
        );
        let source = spec.generate_formalang_source();
        println!("=== FormaLang Source ===\n{}", source);

        let wgsl = spec.compile_to_wgsl();
        assert!(wgsl.is_ok(), "Should compile: {:?}", wgsl.err());

        let wgsl_code = wgsl.unwrap();
        println!("\n=== Pattern-related WGSL ===");
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("Pattern") || line.contains("pattern") || line.contains("Fill_sample") {
                println!("{:4}: {}", i + 1, line);
            }
        }

        // Print fill_Pattern_sample if it exists
        let mut in_func = false;
        let mut depth = 0;
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("fn fill_Pattern_sample") {
                in_func = true;
                depth = 0;
                println!("\n=== fill_Pattern_sample ===");
            }
            if in_func {
                println!("{:4}: {}", i + 1, line);
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && line.contains('}') {
                    in_func = false;
                }
            }
        }

        // Print type tag constants
        println!("\n=== Type Tag Constants ===");
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("FILL_TAG_") && line.contains("const") {
                println!("{:4}: {}", i + 1, line);
            }
        }

        // Print fill_Pattern struct
        in_func = false;
        depth = 0;
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("struct fill_Pattern {") {
                in_func = true;
                depth = 0;
                println!("\n=== fill_Pattern struct ===");
            }
            if in_func {
                println!("{:4}: {}", i + 1, line);
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && line.contains('}') {
                    in_func = false;
                }
            }
        }

        // Print Fill_sample function
        in_func = false;
        depth = 0;
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("fn Fill_sample(") {
                in_func = true;
                depth = 0;
                println!("\n=== Fill_sample ===");
            }
            if in_func {
                println!("{:4}: {}", i + 1, line);
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && line.contains('}') {
                    in_func = false;
                }
            }
        }

        // Print extract_nested_fill_data function
        in_func = false;
        depth = 0;
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("fn extract_nested_fill_data") {
                in_func = true;
                depth = 0;
                println!("\n=== extract_nested_fill_data ===");
            }
            if in_func {
                println!("{:4}: {}", i + 1, line);
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && line.contains('}') {
                    in_func = false;
                }
            }
        }

        // Print FillData struct
        in_func = false;
        depth = 0;
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("struct FillData {") {
                in_func = true;
                depth = 0;
                println!("\n=== FillData struct ===");
            }
            if in_func {
                println!("{:4}: {}", i + 1, line);
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && line.contains('}') {
                    in_func = false;
                }
            }
        }

        // Print test_shape initialization
        println!("\n=== test_shape initialization ===");
        for (i, line) in wgsl_code.lines().enumerate() {
            if line.contains("test_shape") {
                println!("{:4}: {}", i + 1, line);
            }
        }
    }

    #[test]
    fn test_contour_wgsl_output() {
        let spec = ShapeRenderSpec::contour_triangle((100.0, 100.0), [1.0, 0.0, 0.0, 1.0]);
        let source = spec.generate_formalang_source();
        assert!(source.contains("Contour"));
        assert!(source.contains("LineTo"));
        let wgsl = spec.compile_to_wgsl();
        assert!(wgsl.is_ok(), "Should compile: {:?}", wgsl.err());

        let wgsl_code = wgsl.unwrap();
        let lines: Vec<&str> = wgsl_code.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("fn Contour_render") {
                println!("--- Contour_render ---");
                for j in i..std::cmp::min(i + 25, lines.len()) {
                    println!("{}", lines[j]);
                }
            }
        }

        let full_wgsl = spec.generate_wgsl();
        println!("\n=== Entry points ===");
        if let Some(idx) = full_wgsl.find("@fragment") {
            println!(
                "{}",
                &full_wgsl[idx..idx.min(full_wgsl.len() - 1).saturating_add(600)]
            );
        }

        assert!(full_wgsl.contains("Contour_render"));
        assert!(full_wgsl.contains("LineTo_sdf"));
    }
}
