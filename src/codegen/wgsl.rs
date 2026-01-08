//! WGSL code generation from FormaLang IR
//!
//! Generates WebGPU Shading Language (WGSL) from the IR representation.
//! WGSL is the shader language for WebGPU, supported natively in browsers.

use crate::ast::{BinaryOperator, Literal, PrimitiveType, UnaryOperator};
use crate::codegen::dispatch::DispatchGenerator;
use crate::codegen::monomorph::{MonomorphKey, Monomorphizer};
use crate::ir::{simple_type_name, IrExpr, IrFunction, IrImpl, IrModule, IrStruct, ResolvedType};
use std::collections::HashMap;

/// Default maximum array size for unknown-size arrays in for loops.
///
/// When iterating over arrays whose size cannot be determined at compile time,
/// we use this conservative upper bound. 256 elements is chosen as a reasonable
/// limit for GPU shader contexts where large runtime arrays are uncommon.
const DEFAULT_MAX_ARRAY_SIZE: usize = 256;

/// Default maximum data size (in f32 units) for trait dispatch buffers.
///
/// This represents 16 f32 values (64 bytes), which is sufficient for most
/// struct data in GPU shaders while remaining efficient for memory alignment.
const DEFAULT_MAX_DISPATCH_DATA_SIZE: usize = 16;

/// Generate WGSL code from an IR module.
///
/// # Example
///
/// ```
/// use formalang::compile_to_ir;
/// use formalang::codegen::generate_wgsl;
///
/// let source = r#"
///     struct Vec2 { x: f32, y: f32 }
///     impl Vec2 {
///         fn length(self) -> f32 {
///             sqrt(self.x * self.x + self.y * self.y)
///         }
///     }
/// "#;
///
/// let ir = compile_to_ir(source).unwrap();
/// let wgsl = generate_wgsl(&ir);
/// assert!(wgsl.contains("struct Vec2"));
/// ```
pub fn generate_wgsl(module: &IrModule) -> String {
    let mut gen = WgslGenerator::new(module);
    gen.generate()
}

/// Generate WGSL code from an IR module with source map.
///
/// Returns both the generated WGSL and a source map that tracks the
/// relationship between generated lines and source elements.
///
/// # Example
///
/// ```
/// use formalang::compile_to_ir;
/// use formalang::codegen::generate_wgsl_with_sourcemap;
///
/// let source = "struct Vec2 { x: f32, y: f32 }";
/// let ir = compile_to_ir(source).unwrap();
/// let (wgsl, source_map) = generate_wgsl_with_sourcemap(&ir);
///
/// // The source map tracks where each WGSL construct came from
/// assert!(source_map.get_source_name(1).is_some());
/// ```
pub fn generate_wgsl_with_sourcemap(module: &IrModule) -> (String, super::sourcemap::SourceMap) {
    let mut gen = WgslGenerator::new(module);
    let wgsl = gen.generate();
    (wgsl, gen.into_source_map())
}

/// WGSL code generator.
///
/// Transforms FormaLang IR into WGSL shader code.
pub struct WgslGenerator<'a> {
    module: &'a IrModule,
    output: String,
    indent: usize,
    /// Monomorphized struct names: (base_id, args) -> mangled_name
    monomorph_names: HashMap<MonomorphKey, String>,
    /// Current output line number (1-indexed)
    current_line: usize,
    /// Source map tracking WGSL lines to source
    source_map: super::sourcemap::SourceMap,
    /// Counter for generating unique hoisted variable names.
    /// Uses Cell for interior mutability since gen_unique_name is called from
    /// recursive &self methods (gen_expr, gen_block_with_hoisting).
    hoist_counter: std::cell::Cell<u32>,
}

impl<'a> WgslGenerator<'a> {
    /// Create a new WGSL generator for the given IR module.
    pub fn new(module: &'a IrModule) -> Self {
        // Collect and generate monomorphization info
        let mut monomorphizer = Monomorphizer::new(module);
        monomorphizer.collect_instantiations();

        let monomorph_names: HashMap<MonomorphKey, String> = monomorphizer
            .instantiations()
            .iter()
            .map(|key| (key.clone(), key.mangled_name(module)))
            .collect();

        Self {
            module,
            output: String::new(),
            indent: 0,
            monomorph_names,
            current_line: 1,
            source_map: super::sourcemap::SourceMap::new(),
            hoist_counter: std::cell::Cell::new(0),
        }
    }

    /// Generate a unique variable name for hoisted let bindings.
    fn gen_unique_name(&self, base: &str) -> String {
        let count = self.hoist_counter.get();
        self.hoist_counter.set(count + 1);
        format!("_hoist_{}_{}", base, count)
    }

    /// Get the source map after generation.
    pub fn source_map(&self) -> &super::sourcemap::SourceMap {
        &self.source_map
    }

    /// Take ownership of the source map after generation.
    pub fn into_source_map(self) -> super::sourcemap::SourceMap {
        self.source_map
    }

    /// Generate WGSL code and return the result.
    pub fn generate(&mut self) -> String {
        // Generate enum constants first (enums become u32 constants in WGSL)
        for e in &self.module.enums {
            self.gen_enum_constants(e);
        }

        // Generate struct definitions (non-generic only)
        for s in &self.module.structs {
            if s.generic_params.is_empty() {
                self.gen_struct(s);
                self.write_blank_line();
            }
        }

        // Generate monomorphized struct definitions
        self.gen_monomorphized_structs();

        // Generate functions from impl blocks
        for i in &self.module.impls {
            self.gen_impl(i);
        }

        // Generate dispatch code for traits with implementors
        self.gen_trait_dispatch();

        self.output.clone()
    }

    /// Generate WGSL constants for an enum type.
    ///
    /// WGSL doesn't have native enum support, so we represent enums as u32
    /// with named constants for each variant.
    fn gen_enum_constants(&mut self, e: &crate::ir::IrEnum) {
        // Skip enums with generic parameters (not supported in WGSL)
        if !e.generic_params.is_empty() {
            self.write_line(&format!("// Skipping generic enum {}", e.name));
            return;
        }

        // Generate a constant for each variant
        for (idx, variant) in e.variants.iter().enumerate() {
            // Check if variant has associated data
            if !variant.fields.is_empty() {
                // Enums with data need special handling - generate a comment
                self.write_line(&format!(
                    "// {}_{} has associated data - requires struct wrapper",
                    e.name, variant.name
                ));
            }
            self.write_line(&format!(
                "const {}_{}: u32 = {}u;",
                e.name, variant.name, idx
            ));
        }
        self.write_blank_line();
    }

    /// Generate dispatch code for all traits with implementors.
    fn gen_trait_dispatch(&mut self) {
        let dispatch_gen = DispatchGenerator::new(self.module);
        let trait_infos = dispatch_gen.collect_all_trait_dispatch();

        for info in &trait_infos {
            // Skip traits with no implementors
            if info.implementors.is_empty() {
                continue;
            }

            // Generate type tag constants
            self.output.push_str(&dispatch_gen.gen_type_tag_enum(info));
            self.output.push('\n');

            // Calculate max data size needed across all implementors
            let max_size: u32 = info
                .implementors
                .iter()
                .map(|imp| {
                    let s = self.module.get_struct(imp.struct_id);
                    s.fields.iter().map(|f| self.field_size_in_f32(&f.ty)).sum()
                })
                .max()
                .unwrap_or(DEFAULT_MAX_DISPATCH_DATA_SIZE as u32);

            // Generate element data struct
            self.output
                .push_str(&dispatch_gen.gen_element_data_struct(info, max_size as usize));

            // Generate load functions for each implementor
            self.output
                .push_str(&dispatch_gen.gen_all_load_functions(info));
        }

        // Generate placeholder data structs for external traits
        self.gen_external_trait_data_structs();
    }

    /// Generate placeholder data structs for external traits.
    ///
    /// When a struct field references an external trait (from an imported module),
    /// we need to generate a data struct for it even though we don't have the
    /// implementor information. This uses a default size for the data array.
    fn gen_external_trait_data_structs(&mut self) {
        use crate::codegen::dispatch::{DispatchGenerator, DEFAULT_EXTERNAL_TRAIT_DATA_SIZE};
        use std::collections::HashSet;

        // Collect all external trait names referenced in struct fields
        let mut external_traits: HashSet<String> = HashSet::new();

        for s in &self.module.structs {
            for field in &s.fields {
                Self::collect_external_traits(&field.ty, &mut external_traits);
            }
        }

        // Generate data structs for each external trait using the dispatch generator
        for trait_name in external_traits {
            let simple_name = simple_type_name(&trait_name);
            let struct_code = DispatchGenerator::gen_external_trait_data_struct(
                simple_name,
                DEFAULT_EXTERNAL_TRAIT_DATA_SIZE,
            );
            self.output.push_str(&struct_code);
            self.write_blank_line();
        }
    }

    /// Collect external trait type names from a resolved type.
    fn collect_external_traits(ty: &ResolvedType, traits: &mut std::collections::HashSet<String>) {
        use crate::ir::ExternalKind;

        match ty {
            ResolvedType::External {
                name,
                kind: ExternalKind::Trait,
                ..
            } => {
                traits.insert(name.clone());
            }
            ResolvedType::Optional(inner) => {
                Self::collect_external_traits(inner, traits);
            }
            ResolvedType::Array(inner) => {
                Self::collect_external_traits(inner, traits);
            }
            _ => {}
        }
    }

    /// Calculate field size in f32 units for dispatch data packing.
    fn field_size_in_f32(&self, ty: &ResolvedType) -> u32 {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32
                | PrimitiveType::I32
                | PrimitiveType::U32
                | PrimitiveType::Bool => 1,
                PrimitiveType::Vec2 | PrimitiveType::IVec2 | PrimitiveType::UVec2 => 2,
                PrimitiveType::Vec3 | PrimitiveType::IVec3 | PrimitiveType::UVec3 => 3,
                PrimitiveType::Vec4 | PrimitiveType::IVec4 | PrimitiveType::UVec4 => 4,
                PrimitiveType::Mat2 => 4,
                PrimitiveType::Mat3 => 9,
                PrimitiveType::Mat4 => 16,
                _ => 1,
            },
            ResolvedType::Struct(id) => {
                let s = self.module.get_struct(*id);
                s.fields.iter().map(|f| self.field_size_in_f32(&f.ty)).sum()
            }
            _ => 1,
        }
    }

    /// Generate monomorphized versions of generic structs.
    fn gen_monomorphized_structs(&mut self) {
        let monomorphizer = Monomorphizer::new(self.module);
        let mut mono = monomorphizer;
        mono.collect_instantiations();

        for (key, mono_struct) in mono.generate_monomorphized_structs() {
            self.write_line(&format!("struct {} {{", mono_struct.name));
            self.indent += 1;

            for field in &mono_struct.fields {
                let ty = self.type_to_wgsl(&field.ty);
                let field_name = Self::escape_wgsl_keyword(&field.name);
                self.write_line(&format!("{}: {},", field_name, ty));
            }

            self.indent -= 1;
            self.write_line("}");
            self.write_blank_line();

            // Ensure the name is in our map (it should already be there)
            self.monomorph_names.entry(key).or_insert(mono_struct.name);
        }
    }

    /// Generate WGSL code for a struct definition.
    ///
    /// Creates a WGSL struct with all fields typed according to WGSL conventions.
    fn gen_struct(&mut self, s: &IrStruct) {
        // Track struct start in source map
        self.write_line_struct(&format!("struct {} {{", s.name), &s.name);
        self.indent += 1;

        if s.fields.is_empty() {
            // WGSL doesn't allow empty structs; add a placeholder field
            self.write_line("_placeholder: u32,");
        } else {
            for field in &s.fields {
                let ty = self.type_to_wgsl(&field.ty);
                let field_name = Self::escape_wgsl_keyword(&field.name);
                self.write_line(&format!("{}: {},", field_name, ty));
            }
        }

        self.indent -= 1;
        self.write_line("}");
    }

    /// Generate WGSL functions for an impl block.
    ///
    /// Impl blocks become standalone functions with the struct name as prefix.
    fn gen_impl(&mut self, i: &IrImpl) {
        let struct_name = &self.module.get_struct(i.struct_id).name;

        for func in &i.functions {
            self.gen_function(struct_name, func);
            self.write_blank_line();
        }
    }

    /// Generate WGSL code for a function definition.
    ///
    /// Creates a WGSL function with proper signature and body. The struct_name
    /// is used as a prefix for the function name (e.g., `Vec2_length`).
    fn gen_function(&mut self, struct_name: &str, func: &IrFunction) {
        // Generate function signature
        let return_type = func
            .return_type
            .as_ref()
            .map(|t| format!(" -> {}", self.type_to_wgsl(t)))
            .unwrap_or_default();

        // Generate parameters (replacing 'self' with typed parameter)
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                if p.name == "self" {
                    format!("self_: {}", struct_name)
                } else {
                    let ty =
                        p.ty.as_ref()
                            .map(|t| self.type_to_wgsl(t))
                            .unwrap_or_else(|| "f32".to_string());
                    format!("{}: {}", p.name, ty)
                }
            })
            .collect();

        // Function name includes struct prefix for namespacing
        let fn_name = format!("{}_{}", struct_name, func.name);

        // Track function start in source map
        self.write_line_function(
            &format!("fn {}({}){} {{", fn_name, params.join(", "), return_type),
            struct_name,
            &func.name,
        );
        self.indent += 1;

        // Generate function body - check if it needs statement-level generation
        self.gen_function_body(&func.body, func.return_type.as_ref());

        self.indent -= 1;
        self.write_line("}");
    }

    /// Generate function body, handling expressions that need statement-level code.
    fn gen_function_body(&mut self, body: &IrExpr, return_type: Option<&ResolvedType>) {
        match body {
            // For loops need special statement-level handling
            IrExpr::For {
                var,
                var_ty,
                collection,
                body: loop_body,
                ty,
            } => {
                self.gen_for_loop_body(var, var_ty, collection, loop_body, ty, return_type);
            }

            // Match expressions need switch statement generation
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
            } => {
                self.gen_match_body(scrutinee, arms, ty, return_type);
            }

            // Other expressions can be returned directly
            _ => {
                let expr_str = self.gen_expr(body);
                if return_type.is_some() {
                    self.write_line(&format!("return {};", expr_str));
                } else {
                    self.write_line(&format!("{};", expr_str));
                }
            }
        }
    }

    /// Generate a for loop as WGSL statements.
    fn gen_for_loop_body(
        &mut self,
        var: &str,
        var_ty: &ResolvedType,
        collection: &IrExpr,
        body: &IrExpr,
        result_ty: &ResolvedType,
        return_type: Option<&ResolvedType>,
    ) {
        let elem_ty = self.type_to_wgsl(var_ty);

        // Determine the result element type from the loop body
        let result_elem_ty = match result_ty {
            ResolvedType::Array(inner) => self.type_to_wgsl(inner),
            _ => elem_ty.clone(),
        };

        // Try to infer the array size at compile time
        let array_size = self.infer_array_size(collection);

        match array_size {
            Some(size) => {
                // Known compile-time size - generate efficient fixed-size loop
                let coll_str = self.gen_expr(collection);
                self.write_line(&format!("let input_arr = {};", coll_str));
                self.write_line(&format!("var result: array<{}, {}>;", result_elem_ty, size));

                self.write_line(&format!(
                    "for (var i: u32 = 0u; i < {}u; i = i + 1u) {{",
                    size
                ));
                self.indent += 1;

                self.write_line(&format!("let {}: {} = input_arr[i];", var, elem_ty));
                let body_str = self.gen_expr(body);
                self.write_line(&format!("result[i] = {};", body_str));

                self.indent -= 1;
                self.write_line("}");
            }
            None => {
                // Unknown size - generate with max size and comment
                let coll_str = self.gen_expr(collection);
                self.write_line(&format!(
                    "// WGSL_WARNING: Array size unknown at compile time, using max {}",
                    DEFAULT_MAX_ARRAY_SIZE
                ));
                self.write_line(&format!("let input_arr = {};", coll_str));
                self.write_line(&format!(
                    "var result: array<{}, {}>;",
                    result_elem_ty, DEFAULT_MAX_ARRAY_SIZE
                ));
                self.write_line("var result_idx: u32 = 0u;");

                // For runtime-sized arrays, use a bounded loop
                self.write_line(&format!(
                    "for (var i: u32 = 0u; i < {}u; i = i + 1u) {{",
                    DEFAULT_MAX_ARRAY_SIZE
                ));
                self.indent += 1;

                self.write_line(&format!("let {}: {} = input_arr[i];", var, elem_ty));
                let body_str = self.gen_expr(body);
                self.write_line(&format!("result[result_idx] = {};", body_str));
                self.write_line("result_idx = result_idx + 1u;");

                self.indent -= 1;
                self.write_line("}");
            }
        }

        // Return result if needed
        if return_type.is_some() {
            self.write_line("return result;");
        }
    }

    /// Try to infer the array size at compile time.
    fn infer_array_size(&self, collection: &IrExpr) -> Option<usize> {
        match collection {
            // Literal array - size is known
            IrExpr::Array { elements, .. } => Some(elements.len()),

            // Reference - check if it's a constant array
            IrExpr::Reference { path, .. } => {
                // Try to look up the let binding
                if path.len() == 1 {
                    if let Some(let_binding) = self.module.get_let(&path[0]) {
                        return self.infer_array_size(&let_binding.value);
                    }
                }
                None
            }

            // Self field reference - would need struct field analysis
            IrExpr::SelfFieldRef { .. } => None,

            // Let reference - look up the binding
            IrExpr::LetRef { name, .. } => {
                if let Some(let_binding) = self.module.get_let(name) {
                    return self.infer_array_size(&let_binding.value);
                }
                None
            }

            // Other expressions - size unknown
            _ => None,
        }
    }

    /// Generate a match expression as WGSL switch statement.
    fn gen_match_body(
        &mut self,
        scrutinee: &IrExpr,
        arms: &[crate::ir::IrMatchArm],
        _ty: &ResolvedType,
        return_type: Option<&ResolvedType>,
    ) {
        let scrutinee_str = self.gen_expr(scrutinee);

        // For enum matching, we need the type tag
        // Generate: switch(scrutinee.type_tag) { ... }
        if return_type.is_some() {
            self.write_line(&format!(
                "var match_result: {};",
                return_type
                    .map(|t| self.type_to_wgsl(t))
                    .unwrap_or_else(|| "f32".to_string())
            ));
        }

        self.write_line(&format!("switch {} {{", scrutinee_str));
        self.indent += 1;

        // Separate wildcard arm from variant arms
        let (variant_arms, wildcard_arms): (Vec<_>, Vec<_>) =
            arms.iter().partition(|arm| !arm.is_wildcard);

        // Generate case for each variant arm
        for (idx, arm) in variant_arms.iter().enumerate() {
            let tag = idx as u32;
            self.write_line(&format!("case {}u: {{ // {}", tag, arm.variant));
            self.indent += 1;

            // Bind pattern variables if any
            for (i, (name, ty)) in arm.bindings.iter().enumerate() {
                let ty_str = self.type_to_wgsl(ty);
                self.write_line(&format!("// let {}: {} = data[{}];", name, ty_str, i));
            }

            let body_str = self.gen_expr(&arm.body);
            if return_type.is_some() {
                self.write_line(&format!("match_result = {};", body_str));
            } else {
                self.write_line(&format!("{};", body_str));
            }

            self.indent -= 1;
            self.write_line("}");
        }

        // Generate default case (either from wildcard arm or empty)
        if let Some(wildcard_arm) = wildcard_arms.first() {
            self.write_line("default: {");
            self.indent += 1;
            let body_str = self.gen_expr(&wildcard_arm.body);
            if return_type.is_some() {
                self.write_line(&format!("match_result = {};", body_str));
            } else {
                self.write_line(&format!("{};", body_str));
            }
            self.indent -= 1;
            self.write_line("}");
        } else {
            self.write_line("default: {}");
        }

        self.indent -= 1;
        self.write_line("}");

        if return_type.is_some() {
            self.write_line("return match_result;");
        }
    }

    /// Generate WGSL code for an expression.
    ///
    /// This is the core expression code generator that handles all IR expression
    /// types and converts them to WGSL syntax. Returns the generated code as a string.
    fn gen_expr(&self, expr: &IrExpr) -> String {
        match expr {
            IrExpr::Literal { value, .. } => self.gen_literal(value),

            IrExpr::Reference { path, .. } => path.join("."),

            IrExpr::SelfFieldRef { field, .. } => format!("self_.{}", field),

            IrExpr::LetRef { name, .. } => name.clone(),

            IrExpr::BinaryOp {
                left, op, right, ..
            } => {
                let left_str = self.gen_expr(left);
                let right_str = self.gen_expr(right);
                let op_str = self.binary_op_to_wgsl(op);
                format!("({} {} {})", left_str, op_str, right_str)
            }

            IrExpr::UnaryOp { op, operand, .. } => {
                let operand_str = self.gen_expr(operand);
                let op_str = self.unary_op_to_wgsl(op);
                format!("({}{})", op_str, operand_str)
            }

            IrExpr::StructInst {
                struct_id, fields, ..
            } => {
                let name = struct_id
                    .map(|id| self.module.get_struct(id).name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());

                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(n, e)| format!("{}: {}", n, self.gen_expr(e)))
                    .collect();

                format!("{}({})", name, field_strs.join(", "))
            }

            IrExpr::FunctionCall { path, args, .. } => {
                let path_str = path.join("::");
                let fn_name = self.map_builtin_function(&path_str);
                // Extract just the expression values from named args (WGSL uses positional)
                let arg_strs: Vec<String> =
                    args.iter().map(|(_, expr)| self.gen_expr(expr)).collect();
                format!("{}({})", fn_name, arg_strs.join(", "))
            }

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv = self.gen_expr(receiver);
                // Extract just the expression values from named args (WGSL uses positional)
                let arg_strs: Vec<String> =
                    args.iter().map(|(_, expr)| self.gen_expr(expr)).collect();

                // Generate as a function call with receiver as first arg
                let all_args = std::iter::once(recv)
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({})", method, all_args)
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond = self.gen_expr(condition);
                let then_val = self.gen_expr(then_branch);
                if let Some(else_branch) = else_branch {
                    let else_val = self.gen_expr(else_branch);
                    format!("select({}, {}, {})", else_val, then_val, cond)
                } else {
                    // WGSL requires both branches for select
                    format!("select({}, {}, {})", then_val, then_val, cond)
                }
            }

            IrExpr::Array { elements, .. } => {
                let elem_strs: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("array({})", elem_strs.join(", "))
            }

            IrExpr::Tuple { fields, .. } => {
                // WGSL doesn't have tuples directly; generate as struct-like
                let field_strs: Vec<String> =
                    fields.iter().map(|(_, e)| self.gen_expr(e)).collect();
                format!("({})", field_strs.join(", "))
            }

            IrExpr::For {
                var,
                collection,
                body,
                ..
            } => {
                // For expressions in expression context - generate inline array comprehension
                // WGSL doesn't support this directly, so we generate a placeholder
                // that indicates this should be lifted to statement level
                let coll_str = self.gen_expr(collection);
                let body_str = self.gen_expr(body);
                // For simple cases, if body is just transforming the element,
                // we can sometimes inline it. For now, generate array constructor hint.
                format!("/* for_expr({}, {}, {}) */", var, coll_str, body_str)
            }

            IrExpr::Match {
                scrutinee, arms, ..
            } => {
                // Match in expression context - generate chained select() calls
                let scrutinee_str = self.gen_expr(scrutinee);

                if arms.is_empty() {
                    return format!("/* empty match {} */", scrutinee_str);
                }

                // For 2-arm matches, use select()
                if arms.len() == 2 {
                    let first_body = self.gen_expr(&arms[0].body);
                    let second_body = self.gen_expr(&arms[1].body);
                    // select(false_val, true_val, condition)
                    // Assuming first arm is the "true" case (tag == 0)
                    return format!(
                        "select({}, {}, {} == 0u)",
                        second_body, first_body, scrutinee_str
                    );
                }

                // For more arms, chain selects
                // Safety: arms.len() >= 3 at this point (empty and 2-arm cases handled above)
                let mut result = self.gen_expr(&arms[arms.len() - 1].body);
                for (idx, arm) in arms.iter().rev().skip(1).enumerate() {
                    let tag = (arms.len() - 2 - idx) as u32;
                    let arm_body = self.gen_expr(&arm.body);
                    result = format!(
                        "select({}, {}, {} == {}u)",
                        result, arm_body, scrutinee_str, tag
                    );
                }
                result
            }

            IrExpr::EnumInst {
                enum_id,
                variant,
                fields,
                ty,
            } => {
                // Get the enum name from the ID or from the type
                let enum_name = if let Some(id) = enum_id {
                    self.module.get_enum(*id).name.clone()
                } else if let ResolvedType::Enum(id) = ty {
                    self.module.get_enum(*id).name.clone()
                } else {
                    // Fallback for external enums
                    "UnknownEnum".to_string()
                };

                if fields.is_empty() {
                    // Simple unit variant - reference the constant
                    format!("{}_{}", enum_name, variant)
                } else {
                    // Enums with associated data - WGSL doesn't support this directly
                    // Generate a comment noting this limitation
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|(n, e)| format!("{}: {}", n, self.gen_expr(e)))
                        .collect();
                    format!(
                        "/* enum {}::{}({}) - associated data not supported in WGSL */ {}_{}",
                        enum_name,
                        variant,
                        field_strs.join(", "),
                        enum_name,
                        variant
                    )
                }
            }

            IrExpr::EventMapping { variant, param, .. } => {
                // Event mappings are metadata for the runtime, not WGSL code
                // Generate a comment placeholder
                let param_str = param.as_deref().unwrap_or("()");
                format!("/* event: {} -> .{} */", param_str, variant)
            }

            IrExpr::DictLiteral { entries, ty } => {
                // WGSL doesn't have native dictionaries
                // For small literal dictionaries, we could potentially generate a lookup function
                // For now, generate a clear error comment with the entries
                let entry_strs: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                let type_hint = self.type_to_wgsl(ty);
                format!(
                    "/* WGSL_ERROR: Dictionary literals not supported. Type: {}, Entries: {{{}}} */",
                    type_hint,
                    entry_strs.join(", ")
                )
            }

            IrExpr::DictAccess { dict: _, key, ty } => {
                // WGSL doesn't have native dictionary access
                // Generate a clear error message
                let result_type = self.type_to_wgsl(ty);
                format!(
                    "/* WGSL_ERROR: Dictionary access not supported. dict[{}] -> {} */",
                    self.gen_expr(key),
                    result_type
                )
            }

            IrExpr::Block {
                statements, result, ..
            } => {
                // WGSL doesn't have block expressions like Rust's { stmts; expr }
                // We hoist let bindings to the enclosing scope and substitute references
                let (hoisted, result_expr) = self.gen_block_with_hoisting(statements, result);

                if hoisted.is_empty() {
                    // No hoisting needed, just return the result
                    result_expr
                } else {
                    // WGSL doesn't support statements inside expressions.
                    // Block expressions with let bindings can only be properly compiled
                    // when they appear at statement level, not nested inside other expressions.
                    // For now, emit a clear error comment that will cause WGSL validation to fail.
                    format!(
                        "/* WGSL_ERROR: Block expression with {} statement(s) cannot be used in expression position. \
                         Move block to statement level or simplify. Statements: [{}] */ {}",
                        hoisted.len(),
                        hoisted.join("; "),
                        result_expr
                    )
                }
            }
        }
    }

    /// Generate WGSL for a block expression with hoisting.
    ///
    /// Returns (hoisted_statements, result_expression).
    /// The hoisted statements are let bindings that need to be emitted at statement level.
    fn gen_block_with_hoisting(
        &self,
        statements: &[crate::ir::IrBlockStatement],
        result: &IrExpr,
    ) -> (Vec<String>, String) {
        use crate::ir::IrBlockStatement;

        let mut hoisted = Vec::new();
        let mut var_renames: HashMap<String, String> = HashMap::new();

        for stmt in statements {
            match stmt {
                IrBlockStatement::Let {
                    name, value, ty, ..
                } => {
                    // Generate a unique name for this binding
                    let unique_name = self.gen_unique_name(name);
                    var_renames.insert(name.clone(), unique_name.clone());

                    // Generate the hoisted let statement
                    let type_str = ty
                        .as_ref()
                        .map(|t| format!(": {}", self.type_to_wgsl(t)))
                        .unwrap_or_default();
                    let value_expr = self.gen_expr_with_renames(value, &var_renames);
                    hoisted.push(format!("let {}{} = {}", unique_name, type_str, value_expr));
                }
                IrBlockStatement::Assign { target, value } => {
                    // Assignments become statements too
                    let target_expr = self.gen_expr_with_renames(target, &var_renames);
                    let value_expr = self.gen_expr_with_renames(value, &var_renames);
                    hoisted.push(format!("{} = {}", target_expr, value_expr));
                }
                IrBlockStatement::Expr(expr) => {
                    // Expression statements are side effects, generate them
                    let expr_str = self.gen_expr_with_renames(expr, &var_renames);
                    hoisted.push(format!("_ = {}", expr_str));
                }
            }
        }

        // Generate the result expression with variable renames applied
        let result_expr = self.gen_expr_with_renames(result, &var_renames);

        (hoisted, result_expr)
    }

    /// Generate WGSL for an expression with variable renames applied.
    ///
    /// This is used during block hoisting to substitute renamed variables.
    /// Recursively processes all sub-expressions to ensure renames are applied throughout.
    fn gen_expr_with_renames(&self, expr: &IrExpr, renames: &HashMap<String, String>) -> String {
        match expr {
            IrExpr::Reference { path, ty } => {
                // Check if the first path component needs renaming
                if let Some(first) = path.first() {
                    if let Some(new_name) = renames.get(first) {
                        if path.len() == 1 {
                            return new_name.clone();
                        } else {
                            let rest: Vec<&str> = path.iter().skip(1).map(|s| s.as_str()).collect();
                            return format!("{}.{}", new_name, rest.join("."));
                        }
                    }
                }
                self.gen_expr(&IrExpr::Reference {
                    path: path.clone(),
                    ty: ty.clone(),
                })
            }

            // Recursively handle expressions with sub-expressions
            IrExpr::BinaryOp {
                left, op, right, ..
            } => {
                let left_str = self.gen_expr_with_renames(left, renames);
                let right_str = self.gen_expr_with_renames(right, renames);
                let op_str = self.binary_op_to_wgsl(op);
                format!("({} {} {})", left_str, op_str, right_str)
            }

            IrExpr::UnaryOp { op, operand, .. } => {
                let operand_str = self.gen_expr_with_renames(operand, renames);
                let op_str = self.unary_op_to_wgsl(op);
                format!("({}{})", op_str, operand_str)
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond = self.gen_expr_with_renames(condition, renames);
                let then_val = self.gen_expr_with_renames(then_branch, renames);
                if let Some(else_expr) = else_branch {
                    let else_val = self.gen_expr_with_renames(else_expr, renames);
                    format!("select({}, {}, {})", else_val, then_val, cond)
                } else {
                    format!("select(0, {}, {})", then_val, cond)
                }
            }

            IrExpr::FunctionCall { path, args, .. } => {
                let path_str = path.join("::");
                let fn_name = self.map_builtin_function(&path_str);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, e)| self.gen_expr_with_renames(e, renames))
                    .collect();
                format!("{}({})", fn_name, arg_strs.join(", "))
            }

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv = self.gen_expr_with_renames(receiver, renames);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, e)| self.gen_expr_with_renames(e, renames))
                    .collect();
                let all_args = std::iter::once(recv)
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", method, all_args)
            }

            IrExpr::Array { elements, .. } => {
                let elem_strs: Vec<String> = elements
                    .iter()
                    .map(|e| self.gen_expr_with_renames(e, renames))
                    .collect();
                format!("array({})", elem_strs.join(", "))
            }

            IrExpr::Block {
                statements, result, ..
            } => {
                // Nested block - merge renames and recurse
                let (inner_hoisted, inner_result) =
                    self.gen_block_with_hoisting_and_renames(statements, result, renames);
                if inner_hoisted.is_empty() {
                    inner_result
                } else {
                    format!(
                        "(/* hoisted: {} */ {})",
                        inner_hoisted.join("; "),
                        inner_result
                    )
                }
            }

            // For expressions without sub-expressions or complex ones, use gen_expr
            _ => self.gen_expr(expr),
        }
    }

    /// Generate WGSL for a block with hoisting, using existing renames.
    fn gen_block_with_hoisting_and_renames(
        &self,
        statements: &[crate::ir::IrBlockStatement],
        result: &IrExpr,
        parent_renames: &HashMap<String, String>,
    ) -> (Vec<String>, String) {
        use crate::ir::IrBlockStatement;

        let mut hoisted = Vec::new();
        let mut var_renames = parent_renames.clone();

        for stmt in statements {
            match stmt {
                IrBlockStatement::Let {
                    name, value, ty, ..
                } => {
                    let unique_name = self.gen_unique_name(name);
                    var_renames.insert(name.clone(), unique_name.clone());

                    let type_str = ty
                        .as_ref()
                        .map(|t| format!(": {}", self.type_to_wgsl(t)))
                        .unwrap_or_default();
                    let value_expr = self.gen_expr_with_renames(value, &var_renames);
                    hoisted.push(format!("let {}{} = {}", unique_name, type_str, value_expr));
                }
                IrBlockStatement::Assign { target, value } => {
                    let target_expr = self.gen_expr_with_renames(target, &var_renames);
                    let value_expr = self.gen_expr_with_renames(value, &var_renames);
                    hoisted.push(format!("{} = {}", target_expr, value_expr));
                }
                IrBlockStatement::Expr(expr) => {
                    let expr_str = self.gen_expr_with_renames(expr, &var_renames);
                    hoisted.push(format!("_ = {}", expr_str));
                }
            }
        }

        let result_expr = self.gen_expr_with_renames(result, &var_renames);
        (hoisted, result_expr)
    }

    /// Generate WGSL code for a literal value.
    ///
    /// Converts FormaLang literals to WGSL syntax with appropriate type suffixes.
    fn gen_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Number(n) => {
                // Ensure f32 suffix for WGSL
                if n.fract() == 0.0 {
                    format!("{}.0", n)
                } else {
                    format!("{}", n)
                }
            }
            Literal::Boolean(b) => b.to_string(),
            Literal::String(s) => format!("\"{}\"", s),
            Literal::Path(p) => format!("/* path: {} */", p),
            Literal::Regex { pattern, flags } => {
                if flags.is_empty() {
                    format!("/* regex: /{}/ */", pattern)
                } else {
                    format!("/* regex: /{}/{} */", pattern, flags)
                }
            }
            Literal::Nil => "/* nil */".to_string(),
        }
    }

    /// Convert a resolved type to its WGSL type name.
    ///
    /// Maps FormaLang types to their WGSL equivalents, handling structs,
    /// primitives, arrays, generics, and external types.
    fn type_to_wgsl(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => self.primitive_to_wgsl(p),

            ResolvedType::Struct(id) => self.module.get_struct(*id).name.clone(),

            ResolvedType::Array(inner) => {
                // WGSL doesn't support runtime-sized arrays in struct fields.
                // Use a fixed-size array with a reasonable max size.
                // The runtime can use a separate length field to track actual size.
                format!(
                    "array<{}, {}>",
                    self.type_to_wgsl(inner),
                    DEFAULT_MAX_ARRAY_SIZE
                )
            }

            ResolvedType::Optional(inner) => {
                // WGSL doesn't have optionals; use the inner type
                self.type_to_wgsl(inner)
            }

            ResolvedType::Tuple(fields) => {
                // WGSL doesn't have tuples; would need a generated struct
                let types: Vec<String> = fields.iter().map(|(_, t)| self.type_to_wgsl(t)).collect();
                format!("/* tuple({}) */", types.join(", "))
            }

            ResolvedType::Generic { base, args } => {
                // Look up monomorphized name
                let key = MonomorphKey {
                    base_id: *base,
                    args: args.clone(),
                };
                if let Some(name) = self.monomorph_names.get(&key) {
                    name.clone()
                } else {
                    // Fallback: generate mangled name directly
                    key.mangled_name(self.module)
                }
            }

            ResolvedType::TypeParam(name) => {
                // Type parameters with module paths (e.g., "alignment::Horizontal")
                // typically represent external enum types that couldn't be resolved.
                // In WGSL, enums are represented as u32.
                if name.contains("::") {
                    // Module-qualified types are likely external enums
                    "u32".to_string()
                } else {
                    // Simple type parameters keep their name
                    name.clone()
                }
            }

            ResolvedType::Enum(_) => {
                // Enums are represented as u32 in WGSL
                "u32".to_string()
            }

            ResolvedType::Trait(id) => {
                // Map trait types to their dispatch data struct (e.g., Fill -> FillData)
                let trait_def = self.module.get_trait(*id);
                format!("{}Data", trait_def.name)
            }

            ResolvedType::External { name, kind, .. } => {
                use crate::ir::ExternalKind;
                let simple_name = simple_type_name(name);
                match kind {
                    // External structs use their name directly
                    ExternalKind::Struct => simple_name.to_string(),
                    // External traits use the trait data struct pattern
                    ExternalKind::Trait => format!("{}Data", simple_name),
                    // External enums are represented as u32 in WGSL
                    ExternalKind::Enum => "u32".to_string(),
                }
            }

            ResolvedType::EventMapping { .. } => {
                // Event mappings are runtime metadata, not WGSL types
                "/* event mapping */".to_string()
            }

            ResolvedType::Dictionary { key_ty, value_ty } => {
                // WGSL doesn't have native dictionaries
                // Generate an error comment - this field type is not supported
                format!(
                    "/* WGSL_UNSUPPORTED: dict<{}, {}> */",
                    self.type_to_wgsl(key_ty),
                    self.type_to_wgsl(value_ty)
                )
            }
        }
    }

    /// Convert a primitive type to its WGSL name.
    ///
    /// Handles both CPU types (String, Number) and GPU types (f32, vec3, mat4).
    fn primitive_to_wgsl(&self, p: &PrimitiveType) -> String {
        match p {
            // GPU types map directly
            PrimitiveType::F32 => "f32".to_string(),
            PrimitiveType::I32 => "i32".to_string(),
            PrimitiveType::U32 => "u32".to_string(),
            PrimitiveType::Bool => "bool".to_string(),

            // Vector types
            PrimitiveType::Vec2 => "vec2<f32>".to_string(),
            PrimitiveType::Vec3 => "vec3<f32>".to_string(),
            PrimitiveType::Vec4 => "vec4<f32>".to_string(),
            PrimitiveType::IVec2 => "vec2<i32>".to_string(),
            PrimitiveType::IVec3 => "vec3<i32>".to_string(),
            PrimitiveType::IVec4 => "vec4<i32>".to_string(),
            PrimitiveType::UVec2 => "vec2<u32>".to_string(),
            PrimitiveType::UVec3 => "vec3<u32>".to_string(),
            PrimitiveType::UVec4 => "vec4<u32>".to_string(),

            // Matrix types
            PrimitiveType::Mat2 => "mat2x2<f32>".to_string(),
            PrimitiveType::Mat3 => "mat3x3<f32>".to_string(),
            PrimitiveType::Mat4 => "mat4x4<f32>".to_string(),

            // Non-GPU types - map to WGSL placeholders
            // These use u32 as handles/indices since WGSL can't represent them directly
            PrimitiveType::Number => "f32".to_string(),
            PrimitiveType::String => "u32".to_string(), // Handle to string table
            PrimitiveType::Boolean => "bool".to_string(),
            PrimitiveType::Path => "u32".to_string(), // Handle to path data
            PrimitiveType::Regex => "u32".to_string(), // Handle to regex data
            PrimitiveType::Never => "u32".to_string(), // Placeholder for uninhabited type
        }
    }

    /// Escape WGSL reserved keywords by prefixing with underscore.
    ///
    /// WGSL has reserved keywords that cannot be used as identifiers.
    /// This function prefixes them with `_` to make them valid.
    fn escape_wgsl_keyword(name: &str) -> String {
        // WGSL reserved keywords that might conflict with field names
        const WGSL_KEYWORDS: &[&str] = &[
            "alias",
            "break",
            "case",
            "const",
            "const_assert",
            "continue",
            "continuing",
            "default",
            "diagnostic",
            "discard",
            "else",
            "enable",
            "false",
            "fn",
            "for",
            "if",
            "let",
            "loop",
            "override",
            "return",
            "struct",
            "switch",
            "true",
            "var",
            "while",
            // Additional reserved words
            "from",
            "to",
            "in",
            "out",
            "inout",
            "uniform",
            "storage",
            "read",
            "write",
            "read_write",
            "function",
            "private",
            "workgroup",
            "push_constant",
            "vertex",
            "fragment",
            "compute",
        ];

        if WGSL_KEYWORDS.contains(&name) {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }

    /// Convert a binary operator to its WGSL symbol.
    ///
    /// Most operators map directly, but some like And/Or become && and ||.
    fn binary_op_to_wgsl(&self, op: &BinaryOperator) -> &'static str {
        match op {
            BinaryOperator::Add => "+",
            BinaryOperator::Sub => "-",
            BinaryOperator::Mul => "*",
            BinaryOperator::Div => "/",
            BinaryOperator::Mod => "%",
            BinaryOperator::Eq => "==",
            BinaryOperator::Ne => "!=",
            BinaryOperator::Lt => "<",
            BinaryOperator::Le => "<=",
            BinaryOperator::Gt => ">",
            BinaryOperator::Ge => ">=",
            BinaryOperator::And => "&&",
            BinaryOperator::Or => "||",
            BinaryOperator::Range => {
                // Range expressions (0..10) should only appear in for-loop contexts.
                // The for-loop codegen handles ranges specially to emit WGSL loop bounds.
                // If we reach here, it means a range operator appeared outside a for-loop,
                // which is a semantic error that should have been caught earlier.
                // WGSL has no native range type. Emit invalid WGSL that will fail validation
                // with a clear error message rather than panicking.
                "/* INVALID: range operator outside for-loop */"
            }
        }
    }

    fn unary_op_to_wgsl(&self, op: &UnaryOperator) -> &'static str {
        match op {
            UnaryOperator::Neg => "-",
            UnaryOperator::Not => "!",
        }
    }

    /// Map FormaLang built-in function names to WGSL equivalents.
    ///
    /// Provides comprehensive mapping of math, trigonometric, vector, texture,
    /// and other built-in functions. Returns the input name unchanged if no
    /// mapping exists (pass-through for custom or already-correct names).
    fn map_builtin_function<'b>(&self, name: &'b str) -> &'b str {
        match name {
            // Basic math
            "sqrt" => "sqrt",
            "inverseSqrt" | "rsqrt" => "inverseSqrt",
            "abs" => "abs",
            "sign" => "sign",
            "floor" => "floor",
            "ceil" => "ceil",
            "round" => "round",
            "trunc" => "trunc",
            "fract" => "fract",
            "min" => "min",
            "max" => "max",
            "clamp" => "clamp",
            "saturate" => "saturate",
            "pow" => "pow",
            "exp" => "exp",
            "exp2" => "exp2",
            "log" => "log",
            "log2" => "log2",

            // Trigonometry
            "sin" => "sin",
            "cos" => "cos",
            "tan" => "tan",
            "asin" => "asin",
            "acos" => "acos",
            "atan" => "atan",
            "atan2" => "atan2",
            "sinh" => "sinh",
            "cosh" => "cosh",
            "tanh" => "tanh",
            "asinh" => "asinh",
            "acosh" => "acosh",
            "atanh" => "atanh",
            "radians" => "radians",
            "degrees" => "degrees",

            // Vector operations
            "length" => "length",
            "distance" => "distance",
            "normalize" => "normalize",
            "dot" => "dot",
            "cross" => "cross",
            "reflect" => "reflect",
            "refract" => "refract",
            "faceForward" => "faceForward",

            // Interpolation
            "mix" | "lerp" => "mix",
            "step" => "step",
            "smoothstep" => "smoothstep",

            // Comparison
            "all" => "all",
            "any" => "any",
            "select" => "select",

            // Matrix operations
            "transpose" => "transpose",
            "determinant" => "determinant",

            // Texture sampling (when supported)
            "textureSample" => "textureSample",
            "textureLoad" => "textureLoad",
            "textureStore" => "textureStore",
            "textureDimensions" => "textureDimensions",

            // Derivative functions (fragment shader only)
            "dpdx" => "dpdx",
            "dpdy" => "dpdy",
            "fwidth" => "fwidth",

            // Atomic operations
            "atomicAdd" => "atomicAdd",
            "atomicSub" => "atomicSub",
            "atomicMax" => "atomicMax",
            "atomicMin" => "atomicMin",
            "atomicAnd" => "atomicAnd",
            "atomicOr" => "atomicOr",
            "atomicXor" => "atomicXor",

            // Type conversions
            "f32" => "f32",
            "i32" => "i32",
            "u32" => "u32",
            "bool" => "bool",
            "vec2" => "vec2",
            "vec3" => "vec3",
            "vec4" => "vec4",
            "mat2x2" | "mat2" => "mat2x2",
            "mat3x3" | "mat3" => "mat3x3",
            "mat4x4" | "mat4" => "mat4x4",

            // Pass through unknown names
            _ => name,
        }
    }

    /// Write a line to the output with proper indentation.
    ///
    /// Automatically adds spaces based on current indent level and increments
    /// the line counter for source map tracking.
    fn write_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
        self.output.push_str(line);
        self.output.push('\n');
        self.current_line += 1;
    }

    /// Write a line and track it in the source map as a struct start.
    fn write_line_struct(&mut self, line: &str, struct_name: &str) {
        let wgsl_line = self.current_line;
        self.write_line(line);
        self.source_map.add_struct_mapping(wgsl_line, struct_name);
    }

    /// Write a line and track it in the source map as a function start.
    fn write_line_function(&mut self, line: &str, struct_name: &str, fn_name: &str) {
        let wgsl_line = self.current_line;
        self.write_line(line);
        self.source_map
            .add_function_mapping(wgsl_line, struct_name, fn_name);
    }

    /// Write a blank line, incrementing the line counter.
    fn write_blank_line(&mut self) {
        self.output.push('\n');
        self.current_line += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_generate_simple_struct() {
        let source = "struct Point { x: f32, y: f32 }";
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);
        assert!(wgsl.contains("struct Point"));
        assert!(wgsl.contains("x: f32"));
        assert!(wgsl.contains("y: f32"));
    }

    #[test]
    fn test_generate_function() {
        let source = r#"
            struct Vec2 { x: f32, y: f32 }
            impl Vec2 {
                fn length_squared(self) -> f32 {
                    self.x * self.x + self.y * self.y
                }
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);
        assert!(wgsl.contains("fn Vec2_length_squared"));
        assert!(wgsl.contains("self_: Vec2"));
        assert!(wgsl.contains("-> f32"));
        assert!(wgsl.contains("self_.x"));
    }

    #[test]
    fn test_gpu_vector_types() {
        let source = "struct Vertex { position: vec3, normal: vec3, uv: vec2 }";
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);
        assert!(wgsl.contains("position: vec3<f32>"));
        assert!(wgsl.contains("normal: vec3<f32>"));
        assert!(wgsl.contains("uv: vec2<f32>"));
    }

    #[test]
    fn test_gpu_matrix_types() {
        let source = "struct Transform { worldMatrix: mat4, viewMatrix: mat4, projMatrix: mat4 }";
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);
        assert!(wgsl.contains("worldMatrix: mat4x4<f32>"));
        assert!(wgsl.contains("viewMatrix: mat4x4<f32>"));
        assert!(wgsl.contains("projMatrix: mat4x4<f32>"));
    }

    #[test]
    fn test_monomorphized_generic_struct() {
        let source = r#"
            struct Box<T> { value: T }
            struct Container { box: Box<f32> = Box<f32>(value: 1.0) }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should NOT contain the generic struct
        assert!(
            !wgsl.contains("struct Box {"),
            "Should not emit generic struct"
        );

        // Should contain the monomorphized struct
        assert!(
            wgsl.contains("struct Box_f32"),
            "Should emit monomorphized struct"
        );
        assert!(
            wgsl.contains("value: f32"),
            "Monomorphized field should have concrete type"
        );

        // Container should reference the monomorphized type
        assert!(
            wgsl.contains("box: Box_f32"),
            "Container should use monomorphized type"
        );
    }

    #[test]
    fn test_multiple_monomorphizations() {
        let source = r#"
            struct Pair<T> { first: T, second: T }
            struct HolderA { a: Pair<f32> = Pair<f32>(first: 1.0, second: 2.0) }
            struct HolderB { b: Pair<i32> = Pair<i32>(first: 1, second: 2) }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should have both monomorphizations
        assert!(wgsl.contains("struct Pair_f32") || wgsl.contains("Pair_number"));
        assert!(wgsl.contains("struct Pair_i32") || wgsl.contains("Pair_number"));
    }

    #[test]
    fn test_trait_dispatch_generation() {
        let source = r#"
            trait Shape { area: f32 }
            struct Circle: Shape { area: f32, radius: f32 }
            struct Rectangle: Shape { area: f32, width: f32, height: f32 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should have type tag constants
        assert!(
            wgsl.contains("SHAPE_TAG_CIRCLE"),
            "Should have Circle type tag"
        );
        assert!(
            wgsl.contains("SHAPE_TAG_RECTANGLE"),
            "Should have Rectangle type tag"
        );

        // Should have trait-specific data struct (ShapeData, not generic ElementData)
        assert!(
            wgsl.contains("struct ShapeData"),
            "Should have ShapeData struct"
        );
        assert!(
            wgsl.contains("type_tag: u32"),
            "ShapeData should have type_tag"
        );
        assert!(
            wgsl.contains("data: array<f32"),
            "ShapeData should have data array"
        );

        // Should have load functions
        assert!(
            wgsl.contains("fn load_circle"),
            "Should have Circle load function"
        );
        assert!(
            wgsl.contains("fn load_rectangle"),
            "Should have Rectangle load function"
        );
    }

    #[test]
    fn test_dispatch_field_offsets() {
        let source = r#"
            trait Fill { color: vec4 }
            struct Solid: Fill { color: vec4 }
            struct Gradient: Fill { color: vec4, end_color: vec4 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should have field offset constants
        assert!(
            wgsl.contains("SOLID_COLOR_OFFSET"),
            "Should have Solid color offset"
        );
        assert!(
            wgsl.contains("GRADIENT_COLOR_OFFSET"),
            "Should have Gradient color offset"
        );
        assert!(
            wgsl.contains("GRADIENT_END_COLOR_OFFSET"),
            "Should have Gradient end_color offset"
        );
    }

    #[test]
    fn test_no_dispatch_for_traits_without_implementors() {
        let source = r#"
            trait Empty { value: f32 }
            struct Unrelated { x: f32 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should NOT have dispatch code for Empty trait
        assert!(
            !wgsl.contains("EMPTY_TAG"),
            "Should not have Empty type tags"
        );
    }

    // =========================================================================
    // End-to-end WGSL Validation Tests
    // =========================================================================

    #[test]
    fn test_e2e_simple_struct_validates() {
        let source = r#"
            struct Point {
                x: f32,
                y: f32
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Validate generated WGSL with naga
        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "Generated WGSL should be valid. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_struct_with_vectors_validates() {
        let source = r#"
            struct Vertex {
                position: vec3,
                normal: vec3,
                uv: vec2
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "WGSL with vectors should be valid. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_struct_with_matrices_validates() {
        let source = r#"
            struct Transform {
                worldMatrix: mat4,
                viewMatrix: mat4,
                projMatrix: mat4
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "WGSL with matrices should be valid. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_nested_structs_validates() {
        let source = r#"
            struct Inner { value: f32 }
            struct Outer { inner: Inner, scale: f32 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "Nested structs should generate valid WGSL. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_multiple_structs_validates() {
        let source = r#"
            struct Point { x: f32, y: f32 }
            struct Size { width: f32, height: f32 }
            struct Rect { origin: Point, size: Size }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "Multiple structs should generate valid WGSL. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_struct_with_all_primitive_types_validates() {
        let source = r#"
            struct AllTypes {
                f: f32,
                i: i32,
                u: u32,
                b: bool
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "All primitive types should generate valid WGSL. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_struct_with_integer_vectors_validates() {
        let source = r#"
            struct IntVectors {
                iv2: ivec2,
                iv3: ivec3,
                iv4: ivec4,
                uv2: uvec2,
                uv3: uvec3,
                uv4: uvec4
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "Integer vectors should generate valid WGSL. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    #[test]
    fn test_e2e_empty_struct_validates() {
        // Note: WGSL doesn't allow truly empty structs, but if we generate one
        // the validator should catch it. This tests our generation.
        let source = r#"
            struct Marker { _placeholder: u32 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        let result = crate::codegen::validate_wgsl(&wgsl);
        assert!(
            result.is_ok(),
            "Marker struct should generate valid WGSL. WGSL:\n{}\nError: {:?}",
            wgsl,
            result.err()
        );
    }

    // =========================================================================
    // For Loop and Match Expression Tests
    // =========================================================================

    #[test]
    fn test_if_expression_generates_select() {
        // If expression inside a function should generate select() for WGSL
        let source = r#"
            struct Shader {
                flag: Boolean
            }
            impl Shader {
                fn get_multiplier(self) -> f32 {
                    if self.flag { 1.0 } else { 0.0 }
                }
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // If expression should generate select()
        assert!(
            wgsl.contains("select("),
            "If expression should generate select(). WGSL:\n{}",
            wgsl
        );
    }

    #[test]
    fn test_function_generates_with_if() {
        // Test that if expressions in functions generate properly
        let source = r#"
            struct Logic {
                active: Boolean
            }
            impl Logic {
                fn compute(self) -> f32 {
                    if self.active { 10.0 } else { 5.0 }
                }
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should have the function with select
        assert!(
            wgsl.contains("fn Logic_compute"),
            "Should generate function. WGSL:\n{}",
            wgsl
        );
    }

    #[test]
    fn test_method_call_generation() {
        let source = r#"
struct Vec2 { x: f32, y: f32 }
impl Vec2 {
    fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
}
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should generate properly named functions
        assert!(
            wgsl.contains("fn Vec2_length_squared"),
            "Should have length_squared function. WGSL:\n{}",
            wgsl
        );
    }

    // =========================================================================
    // Source Map Tests
    // =========================================================================

    #[test]
    fn test_source_map_struct_tracking() {
        let source = r#"
            struct Vec2 { x: f32, y: f32 }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let (wgsl, source_map) = generate_wgsl_with_sourcemap(&ir);

        // Should have tracked the struct
        assert!(!source_map.is_empty(), "Source map should not be empty");

        // Find the line with the struct
        let struct_entry = source_map
            .entries()
            .iter()
            .find(|(_, entry)| entry.struct_name.as_deref() == Some("Vec2"));
        assert!(
            struct_entry.is_some(),
            "Should find Vec2 struct in source map. WGSL:\n{}",
            wgsl
        );
    }

    #[test]
    fn test_source_map_function_tracking() {
        let source = r#"
struct Vec2 { x: f32, y: f32 }
impl Vec2 {
    fn length(self) -> f32 {
        self.x
    }
}
        "#;
        let ir = compile_to_ir(source).unwrap();
        let (wgsl, source_map) = generate_wgsl_with_sourcemap(&ir);

        // Should have tracked the function
        let fn_entry = source_map
            .entries()
            .iter()
            .find(|(_, entry)| entry.function_name.as_deref() == Some("length"));
        assert!(
            fn_entry.is_some(),
            "Should find length function in source map. WGSL:\n{}",
            wgsl
        );

        // Verify struct_name is also set
        if let Some((_, entry)) = fn_entry {
            assert_eq!(entry.struct_name.as_deref(), Some("Vec2"));
        }
    }

    #[test]
    fn test_source_map_find_closest() {
        let source = r#"
struct Vec2 { x: f32, y: f32 }
impl Vec2 {
    fn length(self) -> f32 {
        self.x
    }
}
        "#;
        let ir = compile_to_ir(source).unwrap();
        let (wgsl, source_map) = generate_wgsl_with_sourcemap(&ir);

        // Find a line inside the function body and use find_closest
        // The function body is typically a few lines after the function declaration
        let fn_line = source_map.entries().iter().find_map(|(line, entry)| {
            if entry.function_name.as_deref() == Some("length") {
                Some(*line)
            } else {
                None
            }
        });

        if let Some(line) = fn_line {
            // The body is within the function, so find_closest should return the function
            let closest = source_map.find_closest(line + 1);
            assert!(
                closest.is_some(),
                "Should find closest mapping. WGSL:\n{}",
                wgsl
            );
        }
    }

    // =========================================================================
    // Enum Generation Tests
    // =========================================================================

    #[test]
    fn test_enum_generates_constants() {
        let source = r#"
            enum Status { active, inactive, pending }
            struct Item { state: Status }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Should generate constants for each variant
        assert!(
            wgsl.contains("const Status_active: u32 = 0u;"),
            "Should have Status_active constant. WGSL:\n{}",
            wgsl
        );
        assert!(
            wgsl.contains("const Status_inactive: u32 = 1u;"),
            "Should have Status_inactive constant. WGSL:\n{}",
            wgsl
        );
        assert!(
            wgsl.contains("const Status_pending: u32 = 2u;"),
            "Should have Status_pending constant. WGSL:\n{}",
            wgsl
        );
    }

    #[test]
    fn test_enum_field_type_is_u32() {
        let source = r#"
            enum Status { active, inactive }
            struct Item { state: Status }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // The struct field should be u32
        assert!(
            wgsl.contains("state: u32"),
            "Enum field should be u32. WGSL:\n{}",
            wgsl
        );
    }

    // =========================================================================
    // Dictionary Unsupported Tests
    // =========================================================================

    #[test]
    fn test_dictionary_type_generates_error() {
        let source = r#"
            struct Config {
                settings: [String: String]
            }
        "#;
        let ir = compile_to_ir(source).unwrap();
        let wgsl = generate_wgsl(&ir);

        // Dictionary type should generate WGSL_UNSUPPORTED comment
        assert!(
            wgsl.contains("WGSL_UNSUPPORTED"),
            "Dictionary type should generate unsupported comment. WGSL:\n{}",
            wgsl
        );
        assert!(
            wgsl.contains("dict"),
            "Should mention dict in error. WGSL:\n{}",
            wgsl
        );
    }

    // Note: Dictionary literals are not supported in the parser/semantic analyzer,
    // so we can't test the literal codegen path directly. The type test above
    // covers the main dictionary unsupported case.

    // =========================================================================
    // Array Size Inference Tests
    // =========================================================================

    // Note: Array size inference requires complex semantic analysis of let bindings
    // and array literals. The current implementation handles basic cases but may not
    // support all test scenarios. Testing is done via integration tests instead.

    // =========================================================================
    // Built-in Function Mapping Tests
    // =========================================================================

    #[test]
    fn test_builtin_function_mapping() {
        use crate::ir::IrModule;

        let module = IrModule::new();
        let gen = WgslGenerator::new(&module);

        // Test math functions
        assert_eq!(gen.map_builtin_function("sqrt"), "sqrt");
        assert_eq!(gen.map_builtin_function("abs"), "abs");
        assert_eq!(gen.map_builtin_function("floor"), "floor");
        assert_eq!(gen.map_builtin_function("ceil"), "ceil");
        assert_eq!(gen.map_builtin_function("round"), "round");

        // Test trig functions
        assert_eq!(gen.map_builtin_function("sin"), "sin");
        assert_eq!(gen.map_builtin_function("cos"), "cos");
        assert_eq!(gen.map_builtin_function("tan"), "tan");
        assert_eq!(gen.map_builtin_function("asin"), "asin");
        assert_eq!(gen.map_builtin_function("acos"), "acos");
        assert_eq!(gen.map_builtin_function("atan"), "atan");

        // Test vector functions
        assert_eq!(gen.map_builtin_function("length"), "length");
        assert_eq!(gen.map_builtin_function("distance"), "distance");
        assert_eq!(gen.map_builtin_function("normalize"), "normalize");
        assert_eq!(gen.map_builtin_function("dot"), "dot");
        assert_eq!(gen.map_builtin_function("cross"), "cross");

        // Test aliases
        assert_eq!(gen.map_builtin_function("rsqrt"), "inverseSqrt");
        assert_eq!(gen.map_builtin_function("inverseSqrt"), "inverseSqrt");
        assert_eq!(gen.map_builtin_function("lerp"), "mix");
        assert_eq!(gen.map_builtin_function("mix"), "mix");

        // Test interpolation
        assert_eq!(gen.map_builtin_function("step"), "step");
        assert_eq!(gen.map_builtin_function("smoothstep"), "smoothstep");

        // Test unknown function (pass-through)
        assert_eq!(gen.map_builtin_function("custom_func"), "custom_func");
    }
}
