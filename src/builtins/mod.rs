//! Builtin functions for FormaLang
//!
//! This module provides definitions for builtin functions available in FormaLang,
//! including math functions, vector operations, and utility functions.
//!
//! These functions map directly to WGSL builtin functions for GPU execution.

use crate::ast::PrimitiveType;
use std::collections::HashMap;

/// A builtin function definition.
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
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<ParamType>,
    /// Return type
    pub return_type: ReturnType,
}

/// Parameter type for builtin functions.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
fn p(t: PrimitiveType) -> ParamType {
    ParamType::Primitive(t)
}

fn r(t: PrimitiveType) -> ReturnType {
    ReturnType::Primitive(t)
}

fn same_param(i: usize) -> ReturnType {
    ReturnType::SameAsParam(i)
}

fn same_as(i: usize) -> ParamType {
    ParamType::SameAs(i)
}

/// Registry of all builtin functions.
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
    pub fn get(&self, name: &str) -> Option<&BuiltinFunction> {
        self.functions.get(name)
    }

    /// Check if a function is a builtin.
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

    fn register(&mut self, func: BuiltinFunction) {
        self.functions.insert(func.name, func);
    }

    fn register_math_functions(&mut self) {
        use PrimitiveType::*;

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

    fn register_vector_functions(&mut self) {
        use PrimitiveType::*;

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
        use PrimitiveType::*;

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
        use PrimitiveType::*;

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
        use PrimitiveType::*;

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
pub fn resolve_method_type(
    receiver_type: PrimitiveType,
    method_name: &str,
) -> Option<PrimitiveType> {
    let registry = builtins();
    let func = registry.get(method_name)?;

    // Find a signature where the first parameter matches the receiver type
    for sig in &func.signatures {
        if sig.params.is_empty() {
            continue;
        }

        let first_param = &sig.params[0];
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
                        PrimitiveType::Vec2 | PrimitiveType::Vec3 | PrimitiveType::Vec4 => {
                            Some(PrimitiveType::F32)
                        }
                        PrimitiveType::IVec2 | PrimitiveType::IVec3 | PrimitiveType::IVec4 => {
                            Some(PrimitiveType::I32)
                        }
                        PrimitiveType::UVec2 | PrimitiveType::UVec3 | PrimitiveType::UVec4 => {
                            Some(PrimitiveType::U32)
                        }
                        PrimitiveType::Mat2 | PrimitiveType::Mat3 | PrimitiveType::Mat4 => {
                            Some(PrimitiveType::F32)
                        }
                        _ => None,
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
    fn test_registry_has_math_functions() {
        let registry = BuiltinRegistry::new();
        assert!(registry.is_builtin("sin"));
        assert!(registry.is_builtin("cos"));
        assert!(registry.is_builtin("sqrt"));
        assert!(registry.is_builtin("pow"));
        assert!(registry.is_builtin("abs"));
    }

    #[test]
    fn test_registry_has_vector_functions() {
        let registry = BuiltinRegistry::new();
        assert!(registry.is_builtin("dot"));
        assert!(registry.is_builtin("cross"));
        assert!(registry.is_builtin("normalize"));
        assert!(registry.is_builtin("length"));
    }

    #[test]
    fn test_registry_has_interpolation_functions() {
        let registry = BuiltinRegistry::new();
        assert!(registry.is_builtin("mix"));
        assert!(registry.is_builtin("step"));
        assert!(registry.is_builtin("smoothstep"));
    }

    #[test]
    fn test_function_signatures() {
        let registry = BuiltinRegistry::new();
        let sqrt = registry.get("sqrt").unwrap();
        // sqrt has multiple signatures (scalar and vector)
        assert!(sqrt.signatures.len() >= 2);
    }

    #[test]
    fn test_by_category() {
        let registry = BuiltinRegistry::new();
        let math_funcs: Vec<_> = registry.by_category(BuiltinCategory::Math).collect();
        assert!(!math_funcs.is_empty());
        assert!(math_funcs.iter().any(|f| f.name == "sin"));
    }

    #[test]
    fn test_global_builtins() {
        assert!(builtins().is_builtin("sin"));
        assert!(!builtins().is_builtin("not_a_function"));
    }

    #[test]
    fn test_resolve_method_type_normalize() {
        // normalize(Vec3) -> Vec3
        assert_eq!(
            resolve_method_type(PrimitiveType::Vec3, "normalize"),
            Some(PrimitiveType::Vec3)
        );
        // normalize(Vec2) -> Vec2
        assert_eq!(
            resolve_method_type(PrimitiveType::Vec2, "normalize"),
            Some(PrimitiveType::Vec2)
        );
    }

    #[test]
    fn test_resolve_method_type_length() {
        // length(Vec3) -> F32
        assert_eq!(
            resolve_method_type(PrimitiveType::Vec3, "length"),
            Some(PrimitiveType::F32)
        );
        // length(Vec2) -> F32
        assert_eq!(
            resolve_method_type(PrimitiveType::Vec2, "length"),
            Some(PrimitiveType::F32)
        );
    }

    #[test]
    fn test_resolve_method_type_math() {
        // sin(F32) -> F32
        assert_eq!(
            resolve_method_type(PrimitiveType::F32, "sin"),
            Some(PrimitiveType::F32)
        );
        // sqrt(F32) -> F32
        assert_eq!(
            resolve_method_type(PrimitiveType::F32, "sqrt"),
            Some(PrimitiveType::F32)
        );
    }

    #[test]
    fn test_resolve_method_type_invalid() {
        // String is not a GPU type
        assert_eq!(
            resolve_method_type(PrimitiveType::String, "normalize"),
            None
        );
        // Non-existent method
        assert_eq!(
            resolve_method_type(PrimitiveType::Vec3, "not_a_method"),
            None
        );
    }

    #[test]
    fn test_resolve_method_type_matrix() {
        // transpose(Mat3) -> Mat3
        assert_eq!(
            resolve_method_type(PrimitiveType::Mat3, "transpose"),
            Some(PrimitiveType::Mat3)
        );
        // determinant(Mat4) -> F32
        assert_eq!(
            resolve_method_type(PrimitiveType::Mat4, "determinant"),
            Some(PrimitiveType::F32)
        );
    }
}
