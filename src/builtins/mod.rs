//! Builtin functions for `FormaLang`
//!
//! This module provides definitions for builtin functions available in `FormaLang`,
//! including math functions, vector operations, and utility functions.
//!
//! These functions map directly to WGSL builtin functions for GPU execution.

use crate::ast::PrimitiveType;
use std::collections::HashMap;

/// A builtin function definition.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct BuiltinFunction {
    /// Function name
    pub name: &'static str,
    /// Parameter types (supports overloading via multiple signatures)
    pub signatures: Vec<FunctionSignature>,
    /// Category for documentation
    pub category: BuiltinCategory,
    /// Brief description
    pub description: &'static str,
}

/// A function signature (parameter types -> return type).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<ParamType>,
    /// Return type
    pub return_type: ReturnType,
}

/// Parameter type for builtin functions.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamType {
    /// A specific primitive type
    Primitive(PrimitiveType),
    /// Any scalar type (f32, i32, u32)
    AnyScalar,
    /// Any float type (f32)
    AnyFloat,
    /// Any integer type (i32, u32)
    AnyInt,
    /// Any vector type (vec2, vec3, vec4)
    AnyVec,
    /// Any float vector type (vec2, vec3, vec4 with f32)
    AnyFloatVec,
    /// Any matrix type
    AnyMat,
    /// Same type as another parameter (for generic functions)
    SameAs(usize),
}

/// Return type for builtin functions.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnType {
    /// A specific primitive type
    Primitive(PrimitiveType),
    /// Same type as a parameter
    SameAsParam(usize),
    /// Scalar type extracted from vector/matrix
    ScalarOf(usize),
    /// Boolean result (for comparison functions)
    Bool,
}

/// Category of builtin functions.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinCategory {
    /// Math functions (sin, cos, sqrt, etc.)
    Math,
    /// Vector operations (dot, cross, normalize, etc.)
    Vector,
    /// Matrix operations
    Matrix,
    /// Comparison and logical functions
    Comparison,
    /// Interpolation functions
    Interpolation,
    /// Type conversion functions
    Conversion,
}

// Shorthand for creating param/return types
const fn p(t: PrimitiveType) -> ParamType {
    ParamType::Primitive(t)
}

const fn r(t: PrimitiveType) -> ReturnType {
    ReturnType::Primitive(t)
}

const fn same_param(i: usize) -> ReturnType {
    ReturnType::SameAsParam(i)
}

const fn same_as(i: usize) -> ParamType {
    ParamType::SameAs(i)
}

/// Registry of all builtin functions.
#[derive(Debug)]
pub struct BuiltinRegistry {
    functions: HashMap<&'static str, BuiltinFunction>,
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinRegistry {
    /// Create a new registry with all builtin functions.
    #[must_use] 
    pub fn new() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
        };
        registry.register_math_functions();
        registry.register_vector_functions();
        registry.register_matrix_functions();
        registry.register_comparison_functions();
        registry.register_interpolation_functions();
        registry
    }

    /// Look up a builtin function by name.
    #[must_use] 
    pub fn get(&self, name: &str) -> Option<&BuiltinFunction> {
        self.functions.get(name)
    }

    /// Check if a function is a builtin.
    #[must_use] 
    pub fn is_builtin(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get all builtin functions.
    pub fn all(&self) -> impl Iterator<Item = &BuiltinFunction> {
        self.functions.values()
    }

    /// Get all functions in a specific category.
    pub fn by_category(&self, category: BuiltinCategory) -> impl Iterator<Item = &BuiltinFunction> {
        self.functions
            .values()
            .filter(move |f| f.category == category)
    }

    /// Resolve the return type of a builtin function given argument types.
    /// Returns the type name as a string, or None if the function doesn't exist
    /// or argument types don't match any signature.
    #[must_use] 
    pub fn resolve_return_type(&self, name: &str, arg_types: &[String]) -> Option<String> {
        let func = self.get(name)?;

        // Use the first signature as the primary one
        func.signatures.first().and_then(|sig| match &sig.return_type {
            ReturnType::Primitive(prim) => Some(Self::primitive_to_string(*prim)),
            ReturnType::SameAsParam(n) => {
                // Return same type as the nth argument
                arg_types
                    .get(*n)
                    .cloned()
                    .or_else(|| Some("Number".to_string()))
            }
            ReturnType::ScalarOf(n) => {
                // Extract scalar type from vector argument
                arg_types
                    .get(*n)
                    .map(|ty| Self::scalar_of_type(ty))
                    .or_else(|| Some("Number".to_string()))
            }
            ReturnType::Bool => Some("Boolean".to_string()),
        })
    }

    /// Extract the scalar component type from a vector type
    fn scalar_of_type(_ty: &str) -> String {
        // All vector and matrix types map to Number in FormaLang
        "Number".to_string()
    }

    /// Convert a primitive type to its `FormaLang` type name
    fn primitive_to_string(prim: PrimitiveType) -> String {
        match prim {
            // GPU scalar types that map to Number
            PrimitiveType::F32 | PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::Number => {
                "Number".to_string()
            }
            PrimitiveType::Bool | PrimitiveType::Boolean => "Boolean".to_string(),
            // GPU vector types (float)
            PrimitiveType::Vec2 => "vec2".to_string(),
            PrimitiveType::Vec3 => "vec3".to_string(),
            PrimitiveType::Vec4 => "vec4".to_string(),
            // GPU vector types (signed int)
            PrimitiveType::IVec2 => "ivec2".to_string(),
            PrimitiveType::IVec3 => "ivec3".to_string(),
            PrimitiveType::IVec4 => "ivec4".to_string(),
            // GPU vector types (unsigned int)
            PrimitiveType::UVec2 => "uvec2".to_string(),
            PrimitiveType::UVec3 => "uvec3".to_string(),
            PrimitiveType::UVec4 => "uvec4".to_string(),
            // GPU matrix types
            PrimitiveType::Mat2 => "mat2".to_string(),
            PrimitiveType::Mat3 => "mat3".to_string(),
            PrimitiveType::Mat4 => "mat4".to_string(),
            // FormaLang-specific types
            PrimitiveType::String => "String".to_string(),
            PrimitiveType::Path => "Path".to_string(),
            PrimitiveType::Regex => "Regex".to_string(),
            PrimitiveType::Never => "Never".to_string(),
        }
    }

    /// Get the global singleton registry instance
    pub fn global() -> &'static Self {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<BuiltinRegistry> = OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    fn register(&mut self, func: BuiltinFunction) {
        self.functions.insert(func.name, func);
    }

    #[expect(clippy::too_many_lines, reason = "large match expression — splitting would reduce clarity")]
    fn register_math_functions(&mut self) {
        use PrimitiveType::{F32, I32, U32};

        // Trigonometric functions
        self.register(BuiltinFunction {
            name: "sin",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Sine of angle in radians",
        });

        self.register(BuiltinFunction {
            name: "cos",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Cosine of angle in radians",
        });

        self.register(BuiltinFunction {
            name: "tan",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Tangent of angle in radians",
        });

        self.register(BuiltinFunction {
            name: "asin",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Arc sine",
        });

        self.register(BuiltinFunction {
            name: "acos",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Arc cosine",
        });

        self.register(BuiltinFunction {
            name: "atan",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Arc tangent",
        });

        self.register(BuiltinFunction {
            name: "atan2",
            signatures: vec![FunctionSignature {
                params: vec![p(F32), p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Arc tangent of y/x",
        });

        // Exponential and logarithmic
        self.register(BuiltinFunction {
            name: "exp",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Natural exponential (e^x)",
        });

        self.register(BuiltinFunction {
            name: "exp2",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Base-2 exponential (2^x)",
        });

        self.register(BuiltinFunction {
            name: "log",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Natural logarithm",
        });

        self.register(BuiltinFunction {
            name: "log2",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Base-2 logarithm",
        });

        self.register(BuiltinFunction {
            name: "pow",
            signatures: vec![FunctionSignature {
                params: vec![p(F32), p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Power function (x^y)",
        });

        self.register(BuiltinFunction {
            name: "sqrt",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Square root",
        });

        self.register(BuiltinFunction {
            name: "inverseSqrt",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Inverse square root (1/sqrt(x))",
        });

        // Rounding functions
        self.register(BuiltinFunction {
            name: "abs",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32)],
                    return_type: r(I32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Absolute value",
        });

        self.register(BuiltinFunction {
            name: "sign",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32)],
                    return_type: r(I32),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Sign of value (-1, 0, or 1)",
        });

        self.register(BuiltinFunction {
            name: "floor",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Floor (round toward negative infinity)",
        });

        self.register(BuiltinFunction {
            name: "ceil",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Ceiling (round toward positive infinity)",
        });

        self.register(BuiltinFunction {
            name: "round",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Round to nearest integer",
        });

        self.register(BuiltinFunction {
            name: "trunc",
            signatures: vec![FunctionSignature {
                params: vec![p(F32)],
                return_type: r(F32),
            }],
            category: BuiltinCategory::Math,
            description: "Truncate toward zero",
        });

        self.register(BuiltinFunction {
            name: "fract",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Fractional part (x - floor(x))",
        });

        // Min/max/clamp
        self.register(BuiltinFunction {
            name: "min",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32), p(I32)],
                    return_type: r(I32),
                },
                FunctionSignature {
                    params: vec![p(U32), p(U32)],
                    return_type: r(U32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Minimum of two values",
        });

        self.register(BuiltinFunction {
            name: "max",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32), p(I32)],
                    return_type: r(I32),
                },
                FunctionSignature {
                    params: vec![p(U32), p(U32)],
                    return_type: r(U32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Maximum of two values",
        });

        self.register(BuiltinFunction {
            name: "clamp",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32), p(I32), p(I32)],
                    return_type: r(I32),
                },
                FunctionSignature {
                    params: vec![p(U32), p(U32), p(U32)],
                    return_type: r(U32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0), same_as(0)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Clamp value between min and max",
        });

        self.register(BuiltinFunction {
            name: "saturate",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Math,
            description: "Clamp to [0, 1] range",
        });
    }

    #[expect(clippy::too_many_lines, reason = "large match expression — splitting would reduce clarity")]
    fn register_vector_functions(&mut self) {
        use PrimitiveType::{Vec2, F32, Vec3, Vec4};

        self.register(BuiltinFunction {
            name: "length",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec3)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec4)],
                    return_type: r(F32),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Vector length (magnitude)",
        });

        self.register(BuiltinFunction {
            name: "distance",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec4), p(Vec4)],
                    return_type: r(F32),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Distance between two points",
        });

        self.register(BuiltinFunction {
            name: "normalize",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2)],
                    return_type: r(Vec2),
                },
                FunctionSignature {
                    params: vec![p(Vec3)],
                    return_type: r(Vec3),
                },
                FunctionSignature {
                    params: vec![p(Vec4)],
                    return_type: r(Vec4),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Normalize vector to unit length",
        });

        self.register(BuiltinFunction {
            name: "dot",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec4), p(Vec4)],
                    return_type: r(F32),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Dot product of two vectors",
        });

        self.register(BuiltinFunction {
            name: "cross",
            signatures: vec![FunctionSignature {
                params: vec![p(Vec3), p(Vec3)],
                return_type: r(Vec3),
            }],
            category: BuiltinCategory::Vector,
            description: "Cross product of two 3D vectors",
        });

        self.register(BuiltinFunction {
            name: "reflect",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2)],
                    return_type: r(Vec2),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3)],
                    return_type: r(Vec3),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Reflect incident vector about normal",
        });

        self.register(BuiltinFunction {
            name: "refract",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2), p(F32)],
                    return_type: r(Vec2),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3), p(F32)],
                    return_type: r(Vec3),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Refract incident vector through surface",
        });

        self.register(BuiltinFunction {
            name: "faceForward",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2), p(Vec2)],
                    return_type: r(Vec2),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3), p(Vec3)],
                    return_type: r(Vec3),
                },
            ],
            category: BuiltinCategory::Vector,
            description: "Flip normal to face forward",
        });
    }

    fn register_matrix_functions(&mut self) {
        use PrimitiveType::{Mat2, Mat3, Mat4, F32};

        self.register(BuiltinFunction {
            name: "transpose",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Mat2)],
                    return_type: r(Mat2),
                },
                FunctionSignature {
                    params: vec![p(Mat3)],
                    return_type: r(Mat3),
                },
                FunctionSignature {
                    params: vec![p(Mat4)],
                    return_type: r(Mat4),
                },
            ],
            category: BuiltinCategory::Matrix,
            description: "Matrix transpose",
        });

        self.register(BuiltinFunction {
            name: "determinant",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Mat2)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Mat3)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Mat4)],
                    return_type: r(F32),
                },
            ],
            category: BuiltinCategory::Matrix,
            description: "Matrix determinant",
        });
    }

    fn register_comparison_functions(&mut self) {
        use PrimitiveType::{Vec2, Vec3, Vec4, F32, Bool, I32, U32};

        self.register(BuiltinFunction {
            name: "all",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2)],
                    return_type: ReturnType::Bool,
                },
                FunctionSignature {
                    params: vec![p(Vec3)],
                    return_type: ReturnType::Bool,
                },
                FunctionSignature {
                    params: vec![p(Vec4)],
                    return_type: ReturnType::Bool,
                },
            ],
            category: BuiltinCategory::Comparison,
            description: "True if all components are true",
        });

        self.register(BuiltinFunction {
            name: "any",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(Vec2)],
                    return_type: ReturnType::Bool,
                },
                FunctionSignature {
                    params: vec![p(Vec3)],
                    return_type: ReturnType::Bool,
                },
                FunctionSignature {
                    params: vec![p(Vec4)],
                    return_type: ReturnType::Bool,
                },
            ],
            category: BuiltinCategory::Comparison,
            description: "True if any component is true",
        });

        self.register(BuiltinFunction {
            name: "select",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32), p(Bool)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(I32), p(I32), p(Bool)],
                    return_type: r(I32),
                },
                FunctionSignature {
                    params: vec![p(U32), p(U32), p(Bool)],
                    return_type: r(U32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0), p(Bool)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Comparison,
            description: "Select between two values based on condition",
        });
    }

    fn register_interpolation_functions(&mut self) {
        use PrimitiveType::{F32, Vec2, Vec3, Vec4};

        self.register(BuiltinFunction {
            name: "mix",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![p(Vec2), p(Vec2), p(F32)],
                    return_type: r(Vec2),
                },
                FunctionSignature {
                    params: vec![p(Vec3), p(Vec3), p(F32)],
                    return_type: r(Vec3),
                },
                FunctionSignature {
                    params: vec![p(Vec4), p(Vec4), p(F32)],
                    return_type: r(Vec4),
                },
            ],
            category: BuiltinCategory::Interpolation,
            description: "Linear interpolation between two values",
        });

        self.register(BuiltinFunction {
            name: "step",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Interpolation,
            description: "Step function (0 if x < edge, else 1)",
        });

        self.register(BuiltinFunction {
            name: "smoothstep",
            signatures: vec![
                FunctionSignature {
                    params: vec![p(F32), p(F32), p(F32)],
                    return_type: r(F32),
                },
                FunctionSignature {
                    params: vec![ParamType::AnyFloatVec, same_as(0), same_as(0)],
                    return_type: same_param(0),
                },
            ],
            category: BuiltinCategory::Interpolation,
            description: "Smooth Hermite interpolation",
        });
    }
}

/// Global builtin registry instance.
static BUILTINS: std::sync::OnceLock<BuiltinRegistry> = std::sync::OnceLock::new();

/// Get the global builtin registry.
pub fn builtins() -> &'static BuiltinRegistry {
    BUILTINS.get_or_init(BuiltinRegistry::new)
}

/// Resolve the return type of a method call on a primitive type.
///
/// This handles cases like `vec3.normalize()`, `vec3.length()`, etc.
/// Returns `None` if the method is not a builtin or doesn't match the receiver type.
#[must_use] 
pub fn resolve_method_type(
    receiver_type: PrimitiveType,
    method_name: &str,
) -> Option<PrimitiveType> {
    let registry = builtins();
    let func = registry.get(method_name)?;

    // Find a signature where the first parameter matches the receiver type
    for sig in &func.signatures {
        let Some(first_param) = sig.params.first() else {
            continue;
        };
        let matches = match first_param {
            ParamType::Primitive(p) => *p == receiver_type,
            ParamType::AnyFloatVec => matches!(
                receiver_type,
                PrimitiveType::Vec2 | PrimitiveType::Vec3 | PrimitiveType::Vec4
            ),
            ParamType::AnyVec => matches!(
                receiver_type,
                PrimitiveType::Vec2
                    | PrimitiveType::Vec3
                    | PrimitiveType::Vec4
                    | PrimitiveType::IVec2
                    | PrimitiveType::IVec3
                    | PrimitiveType::IVec4
                    | PrimitiveType::UVec2
                    | PrimitiveType::UVec3
                    | PrimitiveType::UVec4
            ),
            ParamType::AnyFloat => receiver_type == PrimitiveType::F32,
            ParamType::AnyScalar => matches!(
                receiver_type,
                PrimitiveType::F32 | PrimitiveType::I32 | PrimitiveType::U32
            ),
            ParamType::AnyMat => matches!(
                receiver_type,
                PrimitiveType::Mat2 | PrimitiveType::Mat3 | PrimitiveType::Mat4
            ),
            ParamType::AnyInt => {
                matches!(receiver_type, PrimitiveType::I32 | PrimitiveType::U32)
            }
            ParamType::SameAs(_) => false, // SameAs references another param, not applicable for receiver
        };

        if matches {
            // Found a matching signature, resolve the return type
            return match &sig.return_type {
                ReturnType::Primitive(p) => Some(*p),
                ReturnType::SameAsParam(0) => Some(receiver_type),
                ReturnType::SameAsParam(_) => None, // Other params not available in method call context
                ReturnType::ScalarOf(_) => {
                    // Extract scalar from vector/matrix
                    match receiver_type {
                        PrimitiveType::Vec2
                        | PrimitiveType::Vec3
                        | PrimitiveType::Vec4
                        | PrimitiveType::Mat2
                        | PrimitiveType::Mat3
                        | PrimitiveType::Mat4 => Some(PrimitiveType::F32),
                        PrimitiveType::IVec2 | PrimitiveType::IVec3 | PrimitiveType::IVec4 => {
                            Some(PrimitiveType::I32)
                        }
                        PrimitiveType::UVec2 | PrimitiveType::UVec3 | PrimitiveType::UVec4 => {
                            Some(PrimitiveType::U32)
                        }
                        PrimitiveType::String
                        | PrimitiveType::Number
                        | PrimitiveType::Boolean
                        | PrimitiveType::Path
                        | PrimitiveType::Regex
                        | PrimitiveType::Never
                        | PrimitiveType::F32
                        | PrimitiveType::I32
                        | PrimitiveType::U32
                        | PrimitiveType::Bool => None,
                    }
                }
                ReturnType::Bool => Some(PrimitiveType::Bool),
            };
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_math_functions() -> Result<(), Box<dyn std::error::Error>> {
        let registry = BuiltinRegistry::new();
        if !(registry.is_builtin("sin")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("cos")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("sqrt")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("pow")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("abs")) { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_registry_has_vector_functions() -> Result<(), Box<dyn std::error::Error>> {
        let registry = BuiltinRegistry::new();
        if !(registry.is_builtin("dot")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("cross")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("normalize")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("length")) { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_registry_has_interpolation_functions() -> Result<(), Box<dyn std::error::Error>> {
        let registry = BuiltinRegistry::new();
        if !(registry.is_builtin("mix")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("step")) { return Err("assertion failed".into()); }
        if !(registry.is_builtin("smoothstep")) { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_function_signatures() -> Result<(), Box<dyn std::error::Error>> {
        let registry = BuiltinRegistry::new();
        let sqrt = registry.get("sqrt").ok_or("sqrt not found in registry")?;
        // sqrt has multiple signatures (scalar and vector)
        if sqrt.signatures.len() < 2 {
            return Err(format!(
                "Expected at least 2 signatures for sqrt, got {}",
                sqrt.signatures.len()
            )
            .into());
        }
        Ok(())
    }

    #[test]
    fn test_by_category() -> Result<(), Box<dyn std::error::Error>> {
        let registry = BuiltinRegistry::new();
        let math_funcs: Vec<_> = registry.by_category(BuiltinCategory::Math).collect();
        if math_funcs.is_empty() { return Err("math_funcs should not be empty".into()); }
        if !(math_funcs.iter().any(|f| f.name == "sin")) { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_global_builtins() -> Result<(), Box<dyn std::error::Error>> {
        if !(builtins().is_builtin("sin")) { return Err("assertion failed".into()); }
        if builtins().is_builtin("not_a_function") { return Err("'not_a_function' should not be a builtin".into()); }
        Ok(())
    }

    #[test]
    fn test_resolve_method_type_normalize() -> Result<(), Box<dyn std::error::Error>> {
        // normalize(Vec3) -> Vec3
        let a = resolve_method_type(PrimitiveType::Vec3, "normalize");
        let b = Some(PrimitiveType::Vec3);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        // normalize(Vec2) -> Vec2
        let a = resolve_method_type(PrimitiveType::Vec2, "normalize");
        let b = Some(PrimitiveType::Vec2);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        Ok(())
    }

    #[test]
    fn test_resolve_method_type_length() -> Result<(), Box<dyn std::error::Error>> {
        // length(Vec3) -> F32
        let a = resolve_method_type(PrimitiveType::Vec3, "length");
        let b = Some(PrimitiveType::F32);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        // length(Vec2) -> F32
        let a = resolve_method_type(PrimitiveType::Vec2, "length");
        let b = Some(PrimitiveType::F32);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        Ok(())
    }

    #[test]
    fn test_resolve_method_type_math() -> Result<(), Box<dyn std::error::Error>> {
        // sin(F32) -> F32
        let a = resolve_method_type(PrimitiveType::F32, "sin");
        let b = Some(PrimitiveType::F32);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        // sqrt(F32) -> F32
        let a = resolve_method_type(PrimitiveType::F32, "sqrt");
        let b = Some(PrimitiveType::F32);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        Ok(())
    }

    #[test]
    fn test_resolve_method_type_invalid() -> Result<(), Box<dyn std::error::Error>> {
        // String is not a GPU type
        let a = resolve_method_type(PrimitiveType::String, "normalize");
        let b = None;
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        // Non-existent method
        let a = resolve_method_type(PrimitiveType::Vec3, "not_a_method");
        let b = None;
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        Ok(())
    }

    #[test]
    fn test_resolve_method_type_matrix() -> Result<(), Box<dyn std::error::Error>> {
        // transpose(Mat3) -> Mat3
        let a = resolve_method_type(PrimitiveType::Mat3, "transpose");
        let b = Some(PrimitiveType::Mat3);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        // determinant(Mat4) -> F32
        let a = resolve_method_type(PrimitiveType::Mat4, "determinant");
        let b = Some(PrimitiveType::F32);
        if a != b { return Err(format!("expected {b:?}, got {a:?}").into()); }
        Ok(())
    }
}
