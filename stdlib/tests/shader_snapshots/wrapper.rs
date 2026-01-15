//! WGSL Wrapper Generator
//!
//! Generates WGSL shaders by compiling FormaLang stdlib shapes.
//! This ensures the snapshot tests validate actual FormaLang compilation output.

use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::{compile_with_analyzer_and_resolver, ir::lower_to_ir};
use std::path::PathBuf;

/// Shape-specific field configuration.
#[derive(Debug, Clone)]
pub enum ShapeFields {
    /// Rectangle with optional corner radius.
    Rect {
        fill_rgba: [f32; 4],
        corner_radius: f32,
    },
    /// Circle shape.
    Circle { fill_rgba: [f32; 4] },
    /// Ellipse shape.
    Ellipse { fill_rgba: [f32; 4] },
}

/// Specification for rendering a shape.
#[derive(Debug, Clone)]
pub struct ShapeRenderSpec {
    /// Shape struct name (e.g., "Rect", "Circle", "Ellipse").
    pub struct_name: String,
    /// Render dimensions in pixels.
    pub size: (f32, f32),
    /// Shape-specific field configuration.
    pub fields: ShapeFields,
}

impl ShapeRenderSpec {
    /// Create a Rect render spec.
    pub fn rect(width: f32, height: f32, fill_rgba: [f32; 4], corner_radius: f32) -> Self {
        Self {
            struct_name: "Rect".to_string(),
            size: (width, height),
            fields: ShapeFields::Rect {
                fill_rgba,
                corner_radius,
            },
        }
    }

    /// Create a Circle render spec.
    pub fn circle(diameter: f32, fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Circle".to_string(),
            size: (diameter, diameter),
            fields: ShapeFields::Circle { fill_rgba },
        }
    }

    /// Create an Ellipse render spec.
    pub fn ellipse(width: f32, height: f32, fill_rgba: [f32; 4]) -> Self {
        Self {
            struct_name: "Ellipse".to_string(),
            size: (width, height),
            fields: ShapeFields::Ellipse { fill_rgba },
        }
    }

    /// Generate FormaLang source code for this shape.
    fn generate_formalang_source(&self) -> String {
        let common_imports = r#"use stdlib::shapes::*
use stdlib::color::*
use stdlib::dimension::*
use stdlib::gpu::*
use stdlib::traits::*
use stdlib::fill::*
"#;
        match &self.fields {
            ShapeFields::Rect {
                fill_rgba,
                corner_radius,
            } => {
                format!(
                    r#"{common_imports}
let test_shape = Rect(
    cornerRadius: {corner_radius:.6},
    fill: .solid(color: .rgba(r: {r:.1}, g: {g:.1}, b: {b:.1}, a: {a:.6}))
)
"#,
                    common_imports = common_imports,
                    corner_radius = corner_radius,
                    r = fill_rgba[0] * 255.0,
                    g = fill_rgba[1] * 255.0,
                    b = fill_rgba[2] * 255.0,
                    a = fill_rgba[3]
                )
            }
            ShapeFields::Circle { fill_rgba } => {
                format!(
                    r#"{common_imports}
let test_shape = Circle(
    fill: .solid(color: .rgba(r: {r:.1}, g: {g:.1}, b: {b:.1}, a: {a:.6}))
)
"#,
                    common_imports = common_imports,
                    r = fill_rgba[0] * 255.0,
                    g = fill_rgba[1] * 255.0,
                    b = fill_rgba[2] * 255.0,
                    a = fill_rgba[3]
                )
            }
            ShapeFields::Ellipse { fill_rgba } => {
                format!(
                    r#"{common_imports}
let test_shape = Ellipse(
    fill: .solid(color: .rgba(r: {r:.1}, g: {g:.1}, b: {b:.1}, a: {a:.6}))
)
"#,
                    common_imports = common_imports,
                    r = fill_rgba[0] * 255.0,
                    g = fill_rgba[1] * 255.0,
                    b = fill_rgba[2] * 255.0,
                    a = fill_rgba[3]
                )
            }
        }
    }

    /// Compile FormaLang source to WGSL.
    fn compile_to_wgsl(&self) -> Result<String, String> {
        let source = self.generate_formalang_source();

        // Use FileSystemResolver to find stdlib
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
    ///
    /// Compiles FormaLang stdlib shapes and wraps with vertex/fragment entry points.
    pub fn generate_wgsl(&self) -> String {
        // Compile FormaLang to WGSL
        let compiled_wgsl = self
            .compile_to_wgsl()
            .expect("FormaLang compilation should succeed");

        // Generate entry points that call the compiled shape functions
        let entry_points = self.generate_entry_points();

        format!("{}\n\n{}", compiled_wgsl, entry_points)
    }

    /// Generate vertex/fragment entry points that use the compiled shape functions.
    fn generate_entry_points(&self) -> String {
        let shape_name = &self.struct_name;
        let (width, height) = self.size;

        // Different shapes have different render function signatures:
        // - Rect_render(self, uv, size: vec2<f32>)
        // - Circle_render(self, uv, resolved_radius: f32)
        // - Ellipse_render(self, uv, size: vec2<f32>)
        let render_call = match shape_name.as_str() {
            "Circle" => {
                // Circle expects resolved_radius (half of diameter)
                let radius = width / 2.0;
                format!("{}_render(test_shape, uv, {:.1})", shape_name, radius)
            }
            _ => {
                // Rect, Ellipse expect size as vec2<f32>
                format!(
                    "{}_render(test_shape, uv, vec2<f32>({:.1}, {:.1}))",
                    shape_name, width, height
                )
            }
        };

        // The compiled WGSL has functions like Rect_render, Circle_render, etc.
        // We need to create a shape instance and call its render function.
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

    // Call the FormaLang-compiled render function
    let color = {render_call};
    return Color4_to_vec4(color);
}}
"#,
            render_call = render_call
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_formalang_source() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let source = spec.generate_formalang_source();
        assert!(
            source.contains("use stdlib::shapes::*"),
            "Should import shapes"
        );
        assert!(
            source.contains("use stdlib::color::*"),
            "Should import color"
        );
        assert!(
            source.contains("test_shape = Rect"),
            "Should create Rect instance"
        );
        assert!(
            source.contains("cornerRadius"),
            "Should contain cornerRadius"
        );
        assert!(source.contains("fill:"), "Should contain fill");
    }

    #[test]
    fn test_circle_formalang_source() {
        let spec = ShapeRenderSpec::circle(100.0, [0.0, 1.0, 0.0, 1.0]);
        let source = spec.generate_formalang_source();
        assert!(
            source.contains("use stdlib::shapes::*"),
            "Should import shapes"
        );
        assert!(
            source.contains("test_shape = Circle"),
            "Should create Circle instance"
        );
        assert!(source.contains("fill:"), "Should contain fill");
    }

    #[test]
    fn test_ellipse_formalang_source() {
        let spec = ShapeRenderSpec::ellipse(120.0, 80.0, [0.0, 0.0, 1.0, 1.0]);
        let source = spec.generate_formalang_source();
        assert!(
            source.contains("use stdlib::shapes::*"),
            "Should import shapes"
        );
        assert!(
            source.contains("test_shape = Ellipse"),
            "Should create Ellipse instance"
        );
        assert!(source.contains("fill:"), "Should contain fill");
    }

    #[test]
    fn test_rect_formalang_compiles() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let result = spec.compile_to_wgsl();
        assert!(
            result.is_ok(),
            "FormaLang should compile: {:?}",
            result.err()
        );
        let wgsl = result.unwrap();
        assert!(
            wgsl.contains("Rect_sdf"),
            "Should generate Rect_sdf function"
        );
        assert!(
            wgsl.contains("Rect_render"),
            "Should generate Rect_render function"
        );
    }

    #[test]
    fn test_circle_formalang_compiles() {
        let spec = ShapeRenderSpec::circle(100.0, [0.0, 1.0, 0.0, 1.0]);
        let result = spec.compile_to_wgsl();
        assert!(
            result.is_ok(),
            "FormaLang should compile: {:?}",
            result.err()
        );
        let wgsl = result.unwrap();
        assert!(
            wgsl.contains("Circle_sdf"),
            "Should generate Circle_sdf function"
        );
        assert!(
            wgsl.contains("Circle_render"),
            "Should generate Circle_render function"
        );
    }

    #[test]
    fn test_ellipse_formalang_compiles() {
        let spec = ShapeRenderSpec::ellipse(120.0, 80.0, [0.0, 0.0, 1.0, 1.0]);
        let result = spec.compile_to_wgsl();
        assert!(
            result.is_ok(),
            "FormaLang should compile: {:?}",
            result.err()
        );
        let wgsl = result.unwrap();
        assert!(
            wgsl.contains("Ellipse_sdf"),
            "Should generate Ellipse_sdf function"
        );
        assert!(
            wgsl.contains("Ellipse_render"),
            "Should generate Ellipse_render function"
        );
    }

    #[test]
    fn test_generate_wgsl_includes_entry_points() {
        let spec = ShapeRenderSpec::rect(100.0, 100.0, [1.0, 0.0, 0.0, 1.0], 0.0);
        let wgsl = spec.generate_wgsl();
        assert!(wgsl.contains("@vertex"), "Should have vertex entry point");
        assert!(
            wgsl.contains("@fragment"),
            "Should have fragment entry point"
        );
        assert!(wgsl.contains("vs_main"), "Should have vs_main function");
        assert!(wgsl.contains("fs_main"), "Should have fs_main function");
        assert!(wgsl.contains("Rect_render"), "Should call Rect_render");
    }
}
