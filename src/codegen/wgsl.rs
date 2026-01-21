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
/// This represents 64 f32 values (256 bytes), which is sufficient for most
/// struct data including nested trait references in GPU shaders.
/// Pattern struct needs: nested Fill (50) + width (1) + height (1) + repeat (1) = 53 minimum.
/// Data size for dispatch data arrays (in f32 units).
/// The main FillData.data array size.
const DEFAULT_MAX_DISPATCH_DATA_SIZE: usize = 128;

/// Maximum size reserved for nested trait data in offset calculations.
/// This is smaller than DEFAULT_MAX_DISPATCH_DATA_SIZE to leave room for
/// additional fields after the nested trait (e.g., Pattern has width, height, repeat).
/// Most Fill implementors need at most ~12 f32s (Radial/Angular), so 64 is plenty.
const NESTED_TRAIT_STORED_SIZE: usize = 64;

/// Maximum recursion depth for trait dispatch iteration.
///
/// This limits how deeply nested trait references can be resolved.
/// For example, a Pattern containing another Pattern would require depth 2.
const MAX_TRAIT_DISPATCH_DEPTH: usize = 8;

/// Convert a Rust-style name (potentially with `::` separators) to a valid WGSL identifier.
///
/// WGSL identifiers cannot contain `::`, so we replace them with `_`.
fn to_wgsl_identifier(name: &str) -> String {
    name.replace("::", "_")
}

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

/// Generate WGSL code from an IR module with imported module IR.
///
/// This function generates WGSL that includes functions from impl blocks
/// defined in imported modules. Use this when you need to generate complete
/// shaders that use types from the stdlib or other modules.
///
/// # Arguments
///
/// * `module` - The main IR module to generate WGSL from
/// * `imported_modules` - Cached IR modules from imports (from SemanticAnalyzer)
///
/// # Returns
///
/// Generated WGSL string including functions from imported impl blocks.
///
/// # Example
///
/// ```ignore
/// let (ast, analyzer) = compile_with_analyzer(source)?;
/// let ir = lower_to_ir(&ast, analyzer.symbols())?;
/// let wgsl = generate_wgsl_with_imports(&ir, analyzer.imported_ir_modules());
/// ```
pub fn generate_wgsl_with_imports(
    module: &IrModule,
    imported_modules: &std::collections::HashMap<std::path::PathBuf, IrModule>,
) -> String {
    let mut gen = WgslGenerator::new_with_imports(module, imported_modules);
    gen.generate()
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
    /// Accumulator for hoisted statements from block expressions in expression position.
    /// These are flushed at statement-level contexts (function bodies, etc.).
    /// Uses RefCell for interior mutability since gen_expr is called with &self.
    hoisted_statements: std::cell::RefCell<Vec<String>>,
    /// Cached IR modules for imported modules (keyed by file path).
    ///
    /// Used to generate impl blocks from imported types. Each imported module's
    /// IR is available here for looking up struct/enum definitions and their
    /// associated impl blocks.
    imported_modules: &'a HashMap<std::path::PathBuf, IrModule>,
    /// Current impl type name being generated (for method call mangling on self).
    /// Set during gen_impl_from_foreign to enable proper method name mangling
    /// when methods call other methods on self.
    current_impl_type: Option<String>,
    /// Optional types that need wrapper struct generation.
    /// Maps inner type WGSL name to whether the wrapper has been generated.
    optional_types: std::collections::HashSet<String>,
    /// Track which enum names have been generated to prevent duplicates.
    generated_enums: std::collections::HashSet<String>,
    /// Track which struct names have been generated to prevent duplicates.
    generated_structs: std::collections::HashSet<String>,
    /// Local binding types from match arm pattern bindings.
    /// Used to resolve types for method calls on pattern-bound variables.
    local_binding_types: HashMap<String, ResolvedType>,
    /// Current function's parameter types (name -> type).
    /// Used to resolve types for method calls on function parameters.
    current_function_params: HashMap<String, ResolvedType>,
    /// Maps closure names (from let bindings) to their generated function names.
    /// Used to replace closure calls with the generated function calls.
    closure_functions: std::cell::RefCell<HashMap<String, String>>,
    /// Pending closure function definitions to emit at top level.
    /// Each entry is the complete WGSL function source.
    pending_closure_fns: std::cell::RefCell<Vec<String>>,
    /// Counter for generating unique closure function names.
    closure_counter: std::cell::Cell<u32>,
    /// Track which impl functions have been generated to prevent duplicates.
    /// Maps full WGSL function name (e.g., "fill_Solid_sample") to avoid duplicates
    /// when the same impl is encountered via multiple import paths.
    generated_impl_fns: std::collections::HashSet<String>,
}

/// Empty HashMap for backward compatibility with existing code.
static EMPTY_IMPORTS: std::sync::LazyLock<HashMap<std::path::PathBuf, IrModule>> =
    std::sync::LazyLock::new(HashMap::new);

impl<'a> WgslGenerator<'a> {
    /// Create a new WGSL generator for the given IR module.
    ///
    /// This constructor is for backward compatibility. For generating WGSL
    /// with imported impl blocks, use `new_with_imports` instead.
    pub fn new(module: &'a IrModule) -> Self {
        Self::new_with_imports(module, &EMPTY_IMPORTS)
    }

    /// Create a new WGSL generator with imported module IR.
    ///
    /// # Arguments
    ///
    /// * `module` - The main IR module to generate WGSL from
    /// * `imported_modules` - Cached IR modules for imports (for impl block generation)
    ///
    /// # Returns
    ///
    /// A generator that will produce WGSL code including functions from
    /// imported impl blocks.
    pub fn new_with_imports(
        module: &'a IrModule,
        imported_modules: &'a HashMap<std::path::PathBuf, IrModule>,
    ) -> Self {
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
            hoisted_statements: std::cell::RefCell::new(Vec::new()),
            imported_modules,
            current_impl_type: None,
            optional_types: std::collections::HashSet::new(),
            generated_enums: std::collections::HashSet::new(),
            generated_structs: std::collections::HashSet::new(),
            local_binding_types: HashMap::new(),
            current_function_params: HashMap::new(),
            closure_functions: std::cell::RefCell::new(HashMap::new()),
            pending_closure_fns: std::cell::RefCell::new(Vec::new()),
            closure_counter: std::cell::Cell::new(0),
            generated_impl_fns: std::collections::HashSet::new(),
        }
    }

    /// Generate a unique variable name for hoisted let bindings.
    fn gen_unique_name(&self, base: &str) -> String {
        let count = self.hoist_counter.get();
        self.hoist_counter.set(count + 1);
        format!("_hoist_{}_{}", base, count)
    }

    /// Generate a unique function name for closures.
    fn gen_closure_fn_name(&self, base: &str) -> String {
        let count = self.closure_counter.get();
        self.closure_counter.set(count + 1);
        format!("_closure_{}_{}", base, count)
    }

    /// Register a closure from a let binding and generate its WGSL function.
    ///
    /// Returns the generated function name to use for closure calls.
    fn register_closure_from_foreign(
        &self,
        binding_name: &str,
        params: &[(String, ResolvedType)],
        body: &IrExpr,
        source_module: &IrModule,
    ) -> String {
        let fn_name = self.gen_closure_fn_name(binding_name);

        // Generate parameter list
        let param_strs: Vec<String> = params
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, self.type_to_wgsl_from(ty, source_module)))
            .collect();

        // Generate return type from closure body type
        let return_ty = self.type_to_wgsl_from(body.ty(), source_module);

        // Clear any existing hoisted statements before generating closure body
        // The closure has its own scope, so hoisted statements from the closure
        // must be contained within the closure function
        self.hoisted_statements.borrow_mut().clear();

        // Generate body expression
        let body_str = self.gen_expr_from_foreign(body, source_module);

        // Collect any hoisted statements generated by the closure body
        let body_hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();

        // Build the function source with hoisted statements inside the function body
        let fn_source = if body_hoisted.is_empty() {
            format!(
                "fn {}({}) -> {} {{ return {}; }}",
                fn_name,
                param_strs.join(", "),
                return_ty,
                body_str
            )
        } else {
            format!(
                "fn {}({}) -> {} {{ {} return {}; }}",
                fn_name,
                param_strs.join(", "),
                return_ty,
                body_hoisted
                    .iter()
                    .map(|s| format!("{};", s))
                    .collect::<Vec<_>>()
                    .join(" "),
                body_str
            )
        };

        // Register the mapping and add to pending functions
        self.closure_functions
            .borrow_mut()
            .insert(binding_name.to_string(), fn_name.clone());
        self.pending_closure_fns.borrow_mut().push(fn_source);

        fn_name
    }

    /// Check if a function call is a closure call and return the generated function name.
    fn get_closure_fn_name(&self, path: &[String]) -> Option<String> {
        if path.len() == 1 {
            self.closure_functions.borrow().get(&path[0]).cloned()
        } else {
            None
        }
    }

    /// Push hoisted statements to the accumulator.
    /// Called from gen_expr when encountering a block with statements.
    fn push_hoisted_statements(&self, statements: Vec<String>) {
        self.hoisted_statements.borrow_mut().extend(statements);
    }

    /// Flush hoisted statements to the output.
    /// Called from statement-level contexts before generating the main expression.
    fn flush_hoisted_statements(&mut self) {
        let statements: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();
        for stmt in statements {
            self.write_line(&format!("{};", stmt));
        }
    }

    /// Check if a string is a bare identifier (just alphanumeric chars and underscores).
    /// WGSL doesn't allow bare identifiers as statements.
    fn is_bare_identifier(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
    }

    /// Write an expression as a statement, handling bare identifiers.
    /// Bare identifiers are skipped since WGSL doesn't allow them as statements.
    fn write_expr_as_statement(&mut self, expr_str: &str) {
        if Self::is_bare_identifier(expr_str)
            || expr_str.is_empty()
            || expr_str == "/* nil */"
            || expr_str == "/* void */"
        {
            // Skip bare identifiers and nil placeholders
            return;
        }
        self.write_line(&format!("{};", expr_str));
    }

    /// Generate a for-loop expression as hoisted statements.
    ///
    /// Returns (hoisted_statements, result_variable_name).
    /// The statements should be emitted at statement level, and the result
    /// variable can be used in expression position.
    fn gen_for_expr_hoisted_from_foreign(
        &self,
        var: &str,
        collection: &IrExpr,
        loop_body: &IrExpr,
        ty: &ResolvedType,
        source_module: &IrModule,
    ) -> (Vec<String>, String) {
        // Check if loop body is nil - this is a degenerate case
        if Self::expr_is_nil(loop_body) {
            // Return empty array or just skip
            return (Vec::new(), "/* void for loop */".to_string());
        }

        let body_str = self.gen_expr_from_foreign(loop_body, source_module);

        // Collect any hoisted statements from the body (e.g., from if-expressions with statements)
        let body_hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();

        // Also check if the generated body is nil
        if body_str == "/* nil */" || body_str == "/* void */" {
            return (Vec::new(), "/* void for loop */".to_string());
        }

        let mut hoisted = Vec::new();
        let coll_str = self.gen_expr_from_foreign(collection, source_module);
        let result_var = self.gen_unique_name("for_result");
        let input_var = self.gen_unique_name("for_input");

        // Determine the result element type from the loop body result type
        let result_elem_ty = match ty {
            ResolvedType::Array(inner) => self.type_to_wgsl_from(inner, source_module),
            _ => self.type_to_wgsl_from(ty, source_module),
        };

        // Try to infer the array size at compile time
        let array_size = self
            .infer_array_size_from_foreign(collection, source_module)
            .unwrap_or(DEFAULT_MAX_ARRAY_SIZE);

        hoisted.push(format!("let {} = {}", input_var, coll_str));
        hoisted.push(format!(
            "var {}: array<{}, {}>",
            result_var, result_elem_ty, array_size
        ));

        // Generate loop body with any hoisted statements inside the loop
        let loop_body_stmts = if body_hoisted.is_empty() {
            format!(
                "let {} = {}[_i]; {}[_i] = {};",
                var, input_var, result_var, body_str
            )
        } else {
            format!(
                "let {} = {}[_i]; {} {}[_i] = {};",
                var,
                input_var,
                body_hoisted
                    .iter()
                    .map(|s| format!("{};", s))
                    .collect::<Vec<_>>()
                    .join(" "),
                result_var,
                body_str
            )
        };

        hoisted.push(format!(
            "for (var _i: u32 = 0u; _i < {}u; _i = _i + 1u) {{ {} }}",
            array_size, loop_body_stmts
        ));

        (hoisted, result_var)
    }

    /// Generate a match expression as hoisted statements.
    ///
    /// Returns (hoisted_statements, result_variable_name).
    fn gen_match_expr_hoisted_from_foreign(
        &self,
        scrutinee: &IrExpr,
        arms: &[crate::ir::IrMatchArm],
        ty: &ResolvedType,
        source_module: &IrModule,
    ) -> (Vec<String>, String) {
        let mut hoisted = Vec::new();
        let result_var = self.gen_unique_name("match_result");

        // Helper to check if a type string is valid WGSL
        let is_valid_wgsl_type = |s: &str| -> bool {
            !s.contains('.') && !s.contains(' ') && !s.contains('(') && !s.is_empty()
        };

        // Get the result type - try multiple sources if needed
        let result_ty_str = self.type_to_wgsl_from(ty, source_module);
        let result_ty = if is_valid_wgsl_type(&result_ty_str) {
            result_ty_str
        } else {
            // Try to get type from first arm's body
            let arm_ty = arms
                .first()
                .map(|arm| self.type_to_wgsl_from(arm.body.ty(), source_module));

            if let Some(ref ty_str) = arm_ty {
                if is_valid_wgsl_type(ty_str) {
                    ty_str.clone()
                } else {
                    // Fallback to f32 for unresolvable types
                    "f32".to_string()
                }
            } else {
                "f32".to_string()
            }
        };

        // Declare the result variable
        hoisted.push(format!("var {}: {}", result_var, result_ty));

        let scrutinee_str = self.gen_expr_from_foreign(scrutinee, source_module);

        // Check if the enum TYPE has any variants with data
        let enum_has_data_variants = match scrutinee.ty() {
            ResolvedType::Enum(id) => {
                let e = source_module.get_enum(*id);
                e.variants.iter().any(|v| !v.fields.is_empty())
            }
            ResolvedType::External { name, .. } => {
                // Use max_by_key to prefer enum with fields (re-exported enums may be empty)
                let simple_name = simple_type_name(name);
                self.imported_modules
                    .values()
                    .flat_map(|m| m.enums.iter())
                    .filter(|e| e.name == simple_name)
                    .max_by_key(|e| e.variants.iter().map(|v| v.fields.len()).sum::<usize>())
                    .map(|e| e.variants.iter().any(|v| !v.fields.is_empty()))
                    .unwrap_or(false)
            }
            _ => arms.iter().any(|arm| !arm.bindings.is_empty()),
        };

        // Generate the match as a switch statement with assignments
        let discriminant = if enum_has_data_variants {
            format!("{}.discriminant", scrutinee_str)
        } else {
            scrutinee_str.clone()
        };

        // Separate wildcard from variant arms
        let (variant_arms, wildcard_arms): (Vec<_>, Vec<_>) =
            arms.iter().partition(|arm| !arm.is_wildcard);

        let mut switch_body = Vec::new();
        for (idx, arm) in variant_arms.iter().enumerate() {
            let tag = idx as u32;

            // Clear any accumulated hoisted statements before processing arm body
            self.hoisted_statements.borrow_mut().clear();

            let body_str = self.gen_expr_from_foreign(&arm.body, source_module);

            // Collect any hoisted statements from the arm body
            let arm_hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();

            if arm_hoisted.is_empty() {
                // Simple case - no nested hoisting needed
                switch_body.push(format!(
                    "case {}u: {{ {} = {}; }}",
                    tag, result_var, body_str
                ));
            } else {
                // Complex case - emit hoisted statements within the case block
                let hoisted_stmts = arm_hoisted
                    .iter()
                    .map(|s| format!("{};", s))
                    .collect::<Vec<_>>()
                    .join(" ");
                switch_body.push(format!(
                    "case {}u: {{ {} {} = {}; }}",
                    tag, hoisted_stmts, result_var, body_str
                ));
            }
        }

        // Generate default case
        if let Some(wildcard_arm) = wildcard_arms.first() {
            // Clear before processing wildcard arm
            self.hoisted_statements.borrow_mut().clear();
            let body_str = self.gen_expr_from_foreign(&wildcard_arm.body, source_module);
            let arm_hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();

            if arm_hoisted.is_empty() {
                switch_body.push(format!("default: {{ {} = {}; }}", result_var, body_str));
            } else {
                let hoisted_stmts = arm_hoisted
                    .iter()
                    .map(|s| format!("{};", s))
                    .collect::<Vec<_>>()
                    .join(" ");
                switch_body.push(format!(
                    "default: {{ {} {} = {}; }}",
                    hoisted_stmts, result_var, body_str
                ));
            }
        } else {
            switch_body.push("default: { }".to_string());
        }

        hoisted.push(format!(
            "switch ({}) {{ {} }}",
            discriminant,
            switch_body.join(" ")
        ));

        (hoisted, result_var)
    }

    /// Generate WGSL for a block expression with hoisting (foreign module version).
    ///
    /// Returns (hoisted_statements, result_expression).
    /// The hoisted statements are let bindings that need to be emitted at statement level.
    fn gen_block_with_hoisting_from_foreign(
        &self,
        statements: &[crate::ir::IrBlockStatement],
        result: &IrExpr,
        source_module: &IrModule,
    ) -> (Vec<String>, String) {
        use crate::ir::IrBlockStatement;

        let mut hoisted = Vec::new();
        let mut var_renames: HashMap<String, String> = HashMap::new();

        for stmt in statements {
            match stmt {
                IrBlockStatement::Let {
                    name, value, ty, ..
                } => {
                    // Skip nil-valued let bindings (e.g., unsupported closures)
                    if Self::expr_is_nil(value) {
                        continue;
                    }

                    // Handle closure values: generate a function instead of a let binding
                    if let IrExpr::Closure { params, body, .. } = value {
                        // Register the closure and generate its function
                        self.register_closure_from_foreign(name, params, body, source_module);
                        // Don't generate a let binding for the closure
                        continue;
                    }

                    // Generate a unique name for this binding
                    let unique_name = self.gen_unique_name(name);
                    var_renames.insert(name.clone(), unique_name.clone());

                    // Generate the hoisted let statement
                    let type_str = ty
                        .as_ref()
                        .map(|t| format!(": {}", self.type_to_wgsl_from(t, source_module)))
                        .unwrap_or_default();
                    let value_expr =
                        self.gen_expr_with_renames_from_foreign(value, &var_renames, source_module);

                    // Collect any hoisted statements from the value expression (e.g., if-expressions)
                    // These must come BEFORE the let binding that uses the result
                    let value_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();
                    hoisted.extend(value_hoisted);

                    // Skip if the generated value is a nil placeholder
                    if value_expr == "/* nil */" || value_expr == "/* void */" {
                        continue;
                    }

                    hoisted.push(format!("let {}{} = {}", unique_name, type_str, value_expr));
                }
                IrBlockStatement::Assign { target, value } => {
                    // Assignments become statements too
                    let target_expr =
                        self.gen_expr_with_renames_from_foreign(target, &var_renames, source_module);
                    let value_expr =
                        self.gen_expr_with_renames_from_foreign(value, &var_renames, source_module);

                    // Collect any hoisted statements from target/value expressions
                    let stmt_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();
                    hoisted.extend(stmt_hoisted);

                    hoisted.push(format!("{} = {}", target_expr, value_expr));
                }
                IrBlockStatement::Expr(expr) => {
                    // Expression statements are side effects, generate them
                    // Skip nil expressions and bare identifiers (they have no side effects)
                    if Self::expr_is_nil(expr) {
                        continue;
                    }
                    let expr_str =
                        self.gen_expr_with_renames_from_foreign(expr, &var_renames, source_module);

                    // Collect any hoisted statements from the expression
                    let expr_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();
                    hoisted.extend(expr_hoisted);

                    // Skip bare identifiers (no side effects) and nil placeholders
                    if Self::is_bare_identifier(&expr_str)
                        || expr_str == "/* nil */"
                        || expr_str == "/* void */"
                    {
                        continue;
                    }
                    hoisted.push(format!("_ = {}", expr_str));
                }
            }
        }

        // Generate the result expression with variable renames applied
        let result_expr =
            self.gen_expr_with_renames_from_foreign(result, &var_renames, source_module);

        // Collect any hoisted statements from the result expression
        let result_hoisted: Vec<String> =
            self.hoisted_statements.borrow_mut().drain(..).collect();
        hoisted.extend(result_hoisted);

        (hoisted, result_expr)
    }

    /// Generate WGSL for an expression with variable renames applied (foreign module version).
    ///
    /// This function recursively applies variable renames to all sub-expressions,
    /// ensuring that hoisted variables (like `_hoist_dir_2` for `dir`) are correctly
    /// substituted throughout the expression tree.
    fn gen_expr_with_renames_from_foreign(
        &self,
        expr: &IrExpr,
        renames: &HashMap<String, String>,
        source_module: &IrModule,
    ) -> String {
        // If no renames, delegate directly
        if renames.is_empty() {
            return self.gen_expr_from_foreign(expr, source_module);
        }

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
                self.gen_expr_from_foreign(
                    &IrExpr::Reference {
                        path: path.clone(),
                        ty: ty.clone(),
                    },
                    source_module,
                )
            }

            IrExpr::BinaryOp {
                left, op, right, ..
            } => {
                let left_str = self.gen_expr_with_renames_from_foreign(left, renames, source_module);
                let right_str =
                    self.gen_expr_with_renames_from_foreign(right, renames, source_module);
                let op_str = self.binary_op_to_wgsl(op);
                format!("({} {} {})", left_str, op_str, right_str)
            }

            IrExpr::UnaryOp { op, operand, .. } => {
                let operand_str =
                    self.gen_expr_with_renames_from_foreign(operand, renames, source_module);
                let op_str = self.unary_op_to_wgsl(op);
                format!("{}{}", op_str, operand_str)
            }

            IrExpr::FieldAccess { object, field, .. } => {
                let obj_str =
                    self.gen_expr_with_renames_from_foreign(object, renames, source_module);
                format!("{}.{}", obj_str, Self::escape_wgsl_keyword(field))
            }

            IrExpr::FunctionCall { path, args, .. } => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, arg)| {
                        self.gen_expr_with_renames_from_foreign(arg, renames, source_module)
                    })
                    .collect();
                // Use last component of path as function name
                let fn_name = path.last().map(|s| s.as_str()).unwrap_or("unknown");
                format!("{}({})", Self::escape_wgsl_keyword(fn_name), arg_strs.join(", "))
            }

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv_str =
                    self.gen_expr_with_renames_from_foreign(receiver, renames, source_module);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, arg)| {
                        self.gen_expr_with_renames_from_foreign(arg, renames, source_module)
                    })
                    .collect();

                // Build method call - similar to gen_expr_from_foreign but with renames
                let receiver_ty = receiver.ty();
                let actual_receiver_ty = if let ResolvedType::Optional(inner) = receiver_ty {
                    inner.as_ref()
                } else {
                    receiver_ty
                };

                // Handle TypeParam that's a loop variable (renamed in unrolled loop)
                // The IR stores loop variables with TypeParam("varname") as type,
                // but we need the actual element type for method call mangling.
                let resolved_elem_type = self.resolve_renamed_array_element_type(
                    actual_receiver_ty,
                    renames,
                    source_module,
                );

                let resolved_receiver_ty = resolved_elem_type.as_ref().unwrap_or(actual_receiver_ty);

                let type_name = Self::get_method_type_name(resolved_receiver_ty, source_module)
                    .unwrap_or_else(|| "Unknown".to_string());
                let mangled_name = format!("{}_{}", type_name, method);
                let all_args = std::iter::once(recv_str)
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", mangled_name, all_args)
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_str =
                    self.gen_expr_with_renames_from_foreign(condition, renames, source_module);
                let then_str =
                    self.gen_expr_with_renames_from_foreign(then_branch, renames, source_module);
                if let Some(else_branch) = else_branch {
                    let else_str =
                        self.gen_expr_with_renames_from_foreign(else_branch, renames, source_module);
                    format!("select({}, {}, {})", else_str, then_str, cond_str)
                } else {
                    then_str
                }
            }

            IrExpr::Array { elements, ty } => {
                let elem_strs: Vec<String> = elements
                    .iter()
                    .map(|e| self.gen_expr_with_renames_from_foreign(e, renames, source_module))
                    .collect();
                if elem_strs.is_empty() {
                    if let ResolvedType::Array(inner) = ty {
                        let elem_ty = self.type_to_wgsl_from(inner, source_module);
                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                    } else {
                        format!("array<f32, {}>()", DEFAULT_MAX_ARRAY_SIZE)
                    }
                } else {
                    format!("array({})", elem_strs.join(", "))
                }
            }

            IrExpr::Tuple { fields, .. } => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(_, e)| self.gen_expr_with_renames_from_foreign(e, renames, source_module))
                    .collect();
                format!("({})", field_strs.join(", "))
            }

            IrExpr::DictAccess { dict, key, .. } => {
                let obj_str =
                    self.gen_expr_with_renames_from_foreign(dict, renames, source_module);
                let key_str =
                    self.gen_expr_with_renames_from_foreign(key, renames, source_module);
                format!("{}[{}]", obj_str, key_str)
            }

            IrExpr::Block {
                statements, result, ..
            } => {
                // For blocks with renames, we need to generate each statement with renames
                // and then generate the result with renames
                // Create a mutable copy of renames to accumulate let-bound names
                let mut local_renames = renames.clone();
                let mut parts = Vec::new();
                for stmt in statements {
                    match stmt {
                        crate::ir::IrBlockStatement::Let { name, value, .. } => {
                            // Generate the value expression with current renames
                            let value_str =
                                self.gen_expr_with_renames_from_foreign(value, &local_renames, source_module);
                            if value_str != "/* nil */" && value_str != "/* void */" {
                                // Instead of generating a let statement (which causes redefinition error),
                                // add this binding to the renames map so subsequent uses get inlined
                                local_renames.insert(name.clone(), value_str);
                            }
                        }
                        crate::ir::IrBlockStatement::Assign { target, value } => {
                            let target_str =
                                self.gen_expr_with_renames_from_foreign(target, &local_renames, source_module);
                            let value_str =
                                self.gen_expr_with_renames_from_foreign(value, &local_renames, source_module);
                            parts.push(format!("{} = {}", target_str, value_str));
                        }
                        crate::ir::IrBlockStatement::Expr(expr) => {
                            let expr_str =
                                self.gen_expr_with_renames_from_foreign(expr, &local_renames, source_module);
                            if !expr_str.is_empty() && expr_str != "/* nil */" && expr_str != "/* void */" {
                                parts.push(expr_str);
                            }
                        }
                    }
                }
                let result_str =
                    self.gen_expr_with_renames_from_foreign(result, &local_renames, source_module);
                if parts.is_empty() {
                    result_str
                } else {
                    // For simple blocks, just return the combined parts + result
                    parts.push(result_str);
                    parts.join("; ")
                }
            }

            // For expressions that don't contain sub-expressions with potential renames,
            // or for complex expressions that need special handling, delegate to gen_expr_from_foreign
            _ => self.gen_expr_from_foreign(expr, source_module),
        }
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
        // Collect optional types used in the module for wrapper struct generation
        self.collect_optional_types();

        // Generate imported enums FIRST - they have full field info from the imported IR.
        // Main module enums may have placeholder variants with empty fields for imported types.
        self.gen_imported_enums();

        // Generate enum constants for non-imported enums
        for e in &self.module.enums {
            self.gen_enum_constants(e, Some(self.module));
        }

        // Generate external trait data structs BEFORE optional wrappers
        // (Optional_FillData needs FillData to exist)
        self.gen_external_trait_data_structs();

        // Generate structs from imported modules that implement traits
        // (needed for trait dispatch - e.g., fill_Solid, fill_Pattern)
        self.gen_trait_implementor_structs();

        // Generate Optional wrapper structs before other structs
        self.gen_optional_wrappers();

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

        // Generate functions from imported impl blocks
        self.gen_imported_impls();

        // Generate standalone functions from imported modules
        self.gen_imported_standalone_functions();

        // Generate impl functions for trait implementors (needed for dispatch)
        self.gen_trait_implementor_impls();

        // Generate dispatch code for traits with implementors
        self.gen_trait_dispatch();

        // Emit any closure functions that were generated during expression processing
        self.emit_pending_closure_functions();

        // Generate top-level let bindings
        self.gen_top_level_lets();

        self.output.clone()
    }

    /// Emit all pending closure functions to the output.
    ///
    /// Closure functions are collected during expression generation (when processing
    /// let bindings with closure values). This method flushes them to the output.
    fn emit_pending_closure_functions(&mut self) {
        let pending: Vec<String> = self.pending_closure_fns.borrow_mut().drain(..).collect();
        for fn_source in pending {
            self.write_line(&fn_source);
            self.write_blank_line();
        }
    }

    /// Generate top-level let bindings as WGSL const declarations.
    ///
    /// Module-level let bindings like `let test_shape = Rect(...)` become
    /// global const declarations in WGSL.
    fn gen_top_level_lets(&mut self) {
        for ir_let in &self.module.lets.clone() {
            // Determine which module has the struct definition for the type
            let source_module = self.find_source_module_for_type(&ir_let.ty);

            let type_str = if let Some(imported) = source_module {
                self.type_to_wgsl_from(&ir_let.ty, imported)
            } else {
                self.type_to_wgsl(&ir_let.ty)
            };

            let value_str = if let Some(imported) = source_module {
                self.gen_expr_from_foreign(&ir_let.value, imported)
            } else {
                self.gen_expr(&ir_let.value)
            };

            self.output.push_str(&format!(
                "const {}: {} = {};\n",
                ir_let.name, type_str, value_str
            ));
        }
        if !self.module.lets.is_empty() {
            self.write_blank_line();
        }
    }

    /// Find the imported module that defines a type, if any.
    fn find_source_module_for_type(&self, ty: &ResolvedType) -> Option<&IrModule> {
        let type_name = match ty {
            ResolvedType::External { name, .. } => Some(simple_type_name(name)),
            ResolvedType::Struct(id) => {
                // Check if this struct is in the main module
                if (id.0 as usize) < self.module.structs.len() {
                    let s = self.module.get_struct(*id);
                    // If the struct has no fields, it's likely a placeholder for an imported type
                    if s.fields.is_empty() {
                        Some(s.name.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(name) = type_name {
            // Find the imported module that defines this type
            for imported in self.imported_modules.values() {
                if imported.structs.iter().any(|s| {
                    s.name == name || s.name.ends_with(&format!("::{}", name))
                }) {
                    return Some(imported);
                }
            }
        }
        None
    }

    /// Collect all optional types used in the module.
    ///
    /// Scans struct fields, function parameters, return types, and expressions
    /// for optional types, adding their inner type names to `optional_types`.
    fn collect_optional_types(&mut self) {
        // Collect from struct fields
        for s in &self.module.structs {
            for field in &s.fields {
                self.collect_optional_from_type(&field.ty);
            }
        }

        // Collect from imported modules
        for imported in self.imported_modules.values() {
            for s in &imported.structs {
                for field in &s.fields {
                    self.collect_optional_from_type(&field.ty);
                }
            }
        }
    }

    /// Recursively collect optional types from a ResolvedType.
    fn collect_optional_from_type(&mut self, ty: &ResolvedType) {
        match ty {
            ResolvedType::Optional(inner) => {
                let inner_name = self.type_to_wgsl(inner);
                self.optional_types.insert(inner_name);
                // Recurse into inner type in case of nested optionals
                self.collect_optional_from_type(inner);
            }
            ResolvedType::Array(inner) => {
                self.collect_optional_from_type(inner);
            }
            ResolvedType::Tuple(fields) => {
                for (_, t) in fields {
                    self.collect_optional_from_type(t);
                }
            }
            ResolvedType::Generic { args, .. } => {
                for arg in args {
                    self.collect_optional_from_type(arg);
                }
            }
            _ => {}
        }
    }

    /// Collect all trait names from the module and imported modules.
    ///
    /// Traits don't have concrete WGSL representations, so we need to skip them
    /// when generating Optional wrapper structs.
    fn collect_trait_names(&self) -> std::collections::HashSet<String> {
        let mut trait_names = std::collections::HashSet::new();

        // Collect from main module
        for t in &self.module.traits {
            trait_names.insert(t.name.clone());
        }

        // Collect from imported modules
        for imported in self.imported_modules.values() {
            for t in &imported.traits {
                trait_names.insert(t.name.clone());
            }
        }

        trait_names
    }

    /// Collect names of simple enums (those WITHOUT data variants).
    ///
    /// Simple enums are represented as u32 in WGSL, so they don't need Optional wrappers.
    /// Enums with data variants are represented as structs and DO need Optional wrappers.
    ///
    /// Note: Imported modules have full field info, but main module may have placeholders
    /// with empty fields for imported enums. We prioritize imported module info.
    fn collect_simple_enum_names(&self) -> std::collections::HashSet<String> {
        let mut simple_enums = std::collections::HashSet::new();
        let mut enums_with_data = std::collections::HashSet::new();

        // First, collect from imported modules (authoritative source for imported enums)
        for imported in self.imported_modules.values() {
            for e in &imported.enums {
                let has_data = e.variants.iter().any(|v| !v.fields.is_empty());
                if has_data {
                    enums_with_data.insert(e.name.clone());
                } else {
                    simple_enums.insert(e.name.clone());
                }
            }
        }

        // Then, collect from main module, but skip enums that imported modules say have data
        for e in &self.module.enums {
            // If imported module says this enum has data, trust that
            if enums_with_data.contains(&e.name) {
                continue;
            }
            // Otherwise, check main module's version
            let has_data = e.variants.iter().any(|v| !v.fields.is_empty());
            if !has_data {
                simple_enums.insert(e.name.clone());
            }
        }

        // Remove any enums with data from the simple set (safety check)
        for name in &enums_with_data {
            simple_enums.remove(name);
        }

        simple_enums
    }

    /// Generate Optional wrapper structs for WGSL.
    ///
    /// WGSL doesn't have native optionals, so we generate wrapper structs:
    /// ```wgsl
    /// struct Optional_Color4 {
    ///     has_value: bool,
    ///     value: Color4,
    /// };
    /// ```
    fn gen_optional_wrappers(&mut self) {
        let trait_names = self.collect_trait_names();
        let simple_enum_names = self.collect_simple_enum_names();
        let generic_struct_names = self.collect_generic_struct_names();
        let types: Vec<String> = self.optional_types.iter().cloned().collect();
        for inner_type in types {
            // Skip unsupported or fallback types that would produce invalid WGSL
            if inner_type == "UnsupportedType"
                || inner_type.starts_with("/*")
                || inner_type.contains("Unknown")
            {
                continue;
            }

            // Skip trait types - they don't have concrete WGSL representations
            if trait_names.contains(&inner_type) {
                continue;
            }

            // Skip simple enum types (no data variants) - they become u32 constants
            // Enums with data variants are struct types and need Optional wrappers
            if simple_enum_names.contains(&inner_type) {
                continue;
            }

            // Skip generic struct types that aren't monomorphized
            // (e.g., "Button" is generic as "Button<E>", but we might see just "Button")
            if generic_struct_names.contains(&inner_type) {
                continue;
            }

            let wrapper_name = format!("Optional_{}", inner_type);
            self.write_line(&format!("struct {} {{", wrapper_name));
            self.indent += 1;
            self.write_line("has_value: bool,");
            self.write_line(&format!("value: {},", inner_type));
            self.indent -= 1;
            self.write_line("};");
            self.write_blank_line();
        }
    }

    /// Collect names of generic structs (structs with type parameters).
    fn collect_generic_struct_names(&self) -> std::collections::HashSet<String> {
        let mut names = std::collections::HashSet::new();

        // Check main module
        for s in &self.module.structs {
            if !s.generic_params.is_empty() {
                names.insert(s.name.clone());
                names.insert(simple_type_name(&s.name).to_string());
            }
        }

        // Check imported modules
        for module in self.imported_modules.values() {
            for s in &module.structs {
                if !s.generic_params.is_empty() {
                    names.insert(s.name.clone());
                    names.insert(simple_type_name(&s.name).to_string());
                }
            }
        }

        names
    }

    /// Generate enum constants and wrapper structs for imported enums.
    ///
    /// Iterates over all imports, finds enums with variant data,
    /// and generates the necessary constants and wrapper structs.
    /// Also generates enums that are used as field types in imported structs.
    fn gen_imported_enums(&mut self) {
        use crate::ir::ExternalKind;

        // Track which enum names were imported directly
        let mut imported_enum_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        // Track which struct names were imported
        let mut imported_struct_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for import in &self.module.imports {
            for item in &import.items {
                if matches!(item.kind, ExternalKind::Enum) {
                    imported_enum_names.insert(item.name.clone());
                }
                if matches!(item.kind, ExternalKind::Struct) {
                    imported_struct_names.insert(item.name.clone());
                }
            }
        }

        // Also collect enums that are used as field types in imported structs
        for import in &self.module.imports {
            if let Some(imported_ir) = self.imported_modules.get(&import.source_file) {
                for ir_struct in &imported_ir.structs {
                    if imported_struct_names.contains(&ir_struct.name) {
                        for field in &ir_struct.fields {
                            // Check if field type is an enum (TypeParam that maps to an enum)
                            if let ResolvedType::TypeParam(type_name) = &field.ty {
                                // Look for this enum in imported modules
                                for (_, ir_mod) in self.imported_modules.iter() {
                                    for e in &ir_mod.enums {
                                        if e.name == *type_name
                                            || e.name.ends_with(&format!("::{}", type_name))
                                        {
                                            imported_enum_names.insert(e.name.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Process each import's source module
        // gen_enum_constants handles deduplication via self.generated_enums
        // Clone the imported modules keys to avoid borrow issues
        let import_sources: Vec<_> = self.module.imports.iter().map(|i| i.source_file.clone()).collect();
        for source_file in import_sources {
            if let Some(imported_ir) = self.imported_modules.get(&source_file).cloned() {
                for ir_enum in &imported_ir.enums {
                    // Generate if this enum was imported or needed by imported structs
                    if imported_enum_names.contains(&ir_enum.name) {
                        self.gen_enum_constants(ir_enum, Some(&imported_ir));
                    }
                }
            }
        }
    }

    /// Generate WGSL functions from impl blocks in imported modules.
    ///
    /// Iterates over all imports, finds the corresponding IrModule in the cache,
    /// and generates functions for impl blocks of imported structs/enums.
    ///
    /// Deduplicates by struct name to avoid generating the same function twice.
    fn gen_imported_impls(&mut self) {
        use crate::ir::{ExternalKind, ImplTarget};

        // Track which struct/enum names were imported (both simple and qualified names)
        let mut imported_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for import in &self.module.imports {
            for item in &import.items {
                if matches!(item.kind, ExternalKind::Struct | ExternalKind::Enum) {
                    imported_names.insert(item.name.clone());
                    // Also add simple name for qualified imports like "distribution::Vertical"
                    let simple = simple_type_name(&item.name);
                    imported_names.insert(simple.to_string());
                }
            }
        }

        // Track generated impl names to avoid duplicates
        let mut generated: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Process ALL modules in imported_modules, not just direct imports
        // This ensures transitively re-exported modules (like alignment.fv via stdlib.fv)
        // have their impl blocks generated.
        // Sort by path for deterministic output
        let mut sorted_paths: Vec<_> = self.imported_modules.keys().collect();
        sorted_paths.sort();

        // First pass: generate struct definitions for all structs that have impl blocks
        // This ensures struct definitions exist before we generate their impl functions
        for module_path in &sorted_paths {
            let imported_ir = &self.imported_modules[*module_path];
            for ir_impl in &imported_ir.impls {
                // Skip generic types that won't be emitted to WGSL
                if self.is_impl_target_generic(ir_impl, imported_ir) {
                    continue;
                }

                // If this is a struct impl, generate the struct definition
                if let ImplTarget::Struct(struct_id) = ir_impl.target {
                    let s = imported_ir.get_struct(struct_id);
                    // gen_struct_from_imported handles deduplication via generated_structs
                    self.gen_struct_from_imported(s, imported_ir);
                }
            }
        }

        // Second pass: generate impl functions
        for module_path in sorted_paths {
            let imported_ir = &self.imported_modules[module_path];
            // Generate impls for each type from this module
            for ir_impl in &imported_ir.impls {
                // Skip generic types that won't be emitted to WGSL
                if self.is_impl_target_generic(ir_impl, imported_ir) {
                    continue;
                }

                // Get the type name based on whether it's a struct or enum impl
                let type_name = self.get_impl_type_name(ir_impl, imported_ir);
                let _simple_name = simple_type_name(&type_name).to_string();

                // Generate if we haven't already (use full WGSL name to handle disambiguation)
                // For transitively imported modules, we generate all public impls since
                // we can't easily track which types were re-exported
                let wgsl_name = to_wgsl_identifier(&type_name);
                if !generated.contains(&wgsl_name) {
                    // Check if it's a known imported type OR is from a transitively imported module
                    // For now, generate all impls from imported modules to ensure nothing is missed
                    self.gen_impl_from_foreign(ir_impl, imported_ir);
                    generated.insert(wgsl_name);
                }
            }
        }
    }

    /// Generate standalone functions from imported modules.
    ///
    /// This generates WGSL for standalone functions (not in impl blocks) that
    /// are defined in imported modules. We generate all functions from any
    /// imported module since `use module::*` imports all public items.
    fn gen_imported_standalone_functions(&mut self) {
        // Track generated function names to avoid duplicates
        let mut generated: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Process each import's source module
        for import in &self.module.imports {
            if let Some(imported_ir) = self.imported_modules.get(&import.source_file) {
                // Generate all standalone functions from this module
                for func in &imported_ir.functions {
                    // Skip if already generated (could be imported from multiple modules)
                    if !generated.contains(&func.name) {
                        self.gen_standalone_function_from_foreign(func, imported_ir);
                        generated.insert(func.name.clone());
                    }
                }
            }
        }
    }

    /// Generate a standalone function from a foreign IR module.
    fn gen_standalone_function_from_foreign(&mut self, func: &IrFunction, source_module: &IrModule) {
        // Generate function signature
        let params: Vec<String> = func
            .params
            .iter()
            .filter_map(|p| {
                p.ty.as_ref().map(|ty| {
                    let param_name = Self::escape_wgsl_keyword(&p.name);
                    let param_type = self.type_to_wgsl_from(ty, source_module);
                    format!("{}: {}", param_name, param_type)
                })
            })
            .collect();

        let return_clause = func
            .return_type
            .as_ref()
            .map(|t| format!(" -> {}", self.type_to_wgsl_from(t, source_module)))
            .unwrap_or_default();

        self.output.push_str(&format!(
            "fn {}({}){} {{\n",
            func.name,
            params.join(", "),
            return_clause
        ));

        // Set up function parameter types for method call resolution
        self.current_function_params.clear();
        for p in &func.params {
            if let Some(ty) = &p.ty {
                self.current_function_params.insert(p.name.clone(), ty.clone());
            }
        }

        // Generate function body with proper return handling
        self.indent += 1;
        let return_type = func
            .return_type
            .as_ref()
            .map(|t| self.type_to_wgsl_from(t, source_module));
        self.gen_function_body_from_foreign(&func.body, return_type.as_deref(), source_module);
        self.indent -= 1;

        // Clear function parameter types
        self.current_function_params.clear();

        self.output.push_str("}\n\n");
    }

    /// Get the type name for an impl block (struct or enum).
    fn get_impl_type_name(&self, ir_impl: &crate::ir::IrImpl, module: &IrModule) -> String {
        use crate::ir::ImplTarget;
        match ir_impl.target {
            ImplTarget::Struct(id) => module.get_struct(id).name.clone(),
            ImplTarget::Enum(id) => module.get_enum(id).name.clone(),
        }
    }

    /// Generate impl functions for structs that implement external traits.
    ///
    /// This generates the actual method implementations (e.g., `fill_Solid_sample`)
    /// that are called by the trait dispatch functions.
    fn gen_trait_implementor_impls(&mut self) {
        use std::collections::HashSet;

        // Collect all external trait names
        let mut external_traits: HashSet<String> = HashSet::new();
        for s in &self.module.structs {
            for field in &s.fields {
                Self::collect_external_traits(&field.ty, &mut external_traits);
            }
        }
        for imported in self.imported_modules.values() {
            for s in &imported.structs {
                for field in &s.fields {
                    Self::collect_external_traits_from(&field.ty, &mut external_traits, imported);
                }
            }
        }

        if external_traits.is_empty() {
            return;
        }

        // Track generated impl names to avoid duplicates
        let mut generated: HashSet<String> = HashSet::new();

        // For each trait, find implementors and generate their impl functions
        for trait_name in &external_traits {
            let simple_trait_name = simple_type_name(trait_name);

            // Search imported modules for implementors
            for imported_ir in self.imported_modules.values() {
                for (struct_idx, s) in imported_ir.structs.iter().enumerate() {
                    // Check if this struct implements the trait
                    let implements_trait = if simple_trait_name == "Fill" {
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        imported_ir.impls.iter().any(|imp| {
                            imp.struct_id() == Some(struct_id)
                                && imp.functions.iter().any(|f| f.name == "sample")
                        })
                    } else {
                        s.traits.iter().any(|trait_id| {
                            if (trait_id.0 as usize) < imported_ir.traits.len() {
                                let t = imported_ir.get_trait(*trait_id);
                                t.name == simple_trait_name
                            } else {
                                false
                            }
                        })
                    };

                    if implements_trait {
                        let safe_name = to_wgsl_identifier(&s.name);
                        if generated.contains(&safe_name) {
                            continue;
                        }
                        generated.insert(safe_name.clone());

                        // Find and generate impl for this struct
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        for ir_impl in &imported_ir.impls {
                            if ir_impl.struct_id() == Some(struct_id) {
                                self.gen_impl_from_foreign(ir_impl, imported_ir);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Generate WGSL functions from an impl block using a foreign module's IR.
    ///
    /// This is similar to `gen_impl` but resolves IDs using the source module
    /// rather than `self.module`. This is necessary because IDs (StructId, etc.)
    /// are module-local - they mean different things in different modules.
    ///
    /// # Arguments
    ///
    /// * `ir_impl` - The impl block to generate functions from
    /// * `source_module` - The IrModule that contains the impl block (for ID lookups)
    fn gen_impl_from_foreign(&mut self, ir_impl: &crate::ir::IrImpl, source_module: &IrModule) {
        use crate::ir::ImplTarget;

        let raw_type_name = self.get_impl_type_name(ir_impl, source_module);
        let type_name = to_wgsl_identifier(&raw_type_name);
        let is_enum_impl = matches!(ir_impl.target, ImplTarget::Enum(_));

        // Check if enum has data variants (for self type determination)
        let enum_has_data = if let ImplTarget::Enum(id) = ir_impl.target {
            let e = source_module.get_enum(id);
            e.variants.iter().any(|v| !v.fields.is_empty())
        } else {
            false
        };

        // Set current impl type for method call mangling on self
        self.current_impl_type = Some(type_name.clone());

        for func in &ir_impl.functions {
            // Check if this function was already generated (could happen with
            // transitive re-exports where the same impl appears in multiple modules)
            let fn_name = format!("{}_{}", type_name, func.name);
            if self.generated_impl_fns.contains(&fn_name) {
                continue;
            }
            self.generated_impl_fns.insert(fn_name);

            self.gen_function_from_foreign(
                &type_name,
                func,
                source_module,
                is_enum_impl,
                enum_has_data,
            );
            self.write_blank_line();
        }

        // Clear current impl type
        self.current_impl_type = None;
    }

    /// Generate a function from a foreign module's impl block.
    ///
    /// Uses `source_module` for all ID-to-name lookups instead of `self.module`.
    fn gen_function_from_foreign(
        &mut self,
        struct_name: &str,
        func: &IrFunction,
        source_module: &IrModule,
        is_enum_impl: bool,
        enum_has_data: bool,
    ) {
        // Clear any hoisted statements from previous function generation
        // This prevents statements from leaking across function boundaries
        self.hoisted_statements.borrow_mut().clear();

        // Generate function signature
        let return_type = func
            .return_type
            .as_ref()
            .map(|t| self.type_to_wgsl_from(t, source_module));

        let fn_name = format!("{}_{}", struct_name, func.name);

        // For enum impls: use struct name if enum has data variants, otherwise u32
        let self_type = if is_enum_impl && !enum_has_data {
            "u32"
        } else {
            struct_name
        };

        // Check if self type is a struct with array fields - if so, we need to copy
        // the self parameter to a local var so its arrays can be dynamically indexed
        let needs_self_copy = if !is_enum_impl {
            self.struct_has_array_fields(struct_name, source_module)
        } else {
            false
        };

        // Generate parameters (escaping reserved keywords)
        // If we need a self copy for dynamic array indexing, use a different param name
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                if p.name == "self" {
                    if needs_self_copy {
                        format!("self_param_: {}", self_type)
                    } else {
                        format!("self_: {}", self_type)
                    }
                } else {
                    let param_name = Self::escape_wgsl_keyword(&p.name);
                    let ty =
                        p.ty.as_ref()
                            .map(|t| self.type_to_wgsl_from(t, source_module))
                            .unwrap_or_else(|| "f32".to_string());
                    format!("{}: {}", param_name, ty)
                }
            })
            .collect();

        // Write function signature
        let return_str = return_type
            .as_ref()
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default();
        self.write_line(&format!(
            "fn {}({}){} {{",
            fn_name,
            params.join(", "),
            return_str
        ));
        self.indent += 1;

        // If we need a self copy for dynamic array indexing, emit it first
        // In WGSL, function parameters are implicitly `let` bound, so array fields
        // cannot be dynamically indexed. Copying to a `var` allows dynamic indexing.
        if needs_self_copy {
            self.write_line(&format!("var self_ = self_param_;"));
        }

        // Set up function parameter types for method call resolution
        self.current_function_params.clear();
        for p in &func.params {
            if let Some(ty) = &p.ty {
                self.current_function_params.insert(p.name.clone(), ty.clone());
            }
        }

        // Generate function body using foreign module for lookups
        self.gen_function_body_from_foreign(&func.body, return_type.as_deref(), source_module);

        // Clear function parameter types
        self.current_function_params.clear();

        self.indent -= 1;
        self.write_line("}");
    }

    /// Check if a struct has any array fields (which would require dynamic indexing workaround).
    fn struct_has_array_fields(&self, struct_name: &str, source_module: &IrModule) -> bool {
        // Check in source module
        if let Some(s) = source_module
            .structs
            .iter()
            .find(|s| to_wgsl_identifier(&s.name) == struct_name || s.name == struct_name)
        {
            return s.fields.iter().any(|f| matches!(f.ty, ResolvedType::Array(_)));
        }

        // Check in imported modules
        for imported in self.imported_modules.values() {
            if let Some(s) = imported
                .structs
                .iter()
                .find(|s| to_wgsl_identifier(&s.name) == struct_name || s.name == struct_name)
            {
                return s.fields.iter().any(|f| matches!(f.ty, ResolvedType::Array(_)));
            }
        }

        false
    }

    /// Generate function body from a foreign module's expression.
    fn gen_function_body_from_foreign(
        &mut self,
        body: &IrExpr,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        match body {
            // Block expressions need statement-level handling for let bindings
            IrExpr::Block {
                statements, result, ..
            } => {
                self.gen_block_body_from_foreign(statements, result, return_type, source_module);
            }

            // For loops need special statement-level handling
            IrExpr::For {
                var,
                collection,
                body: loop_body,
                ty,
                ..
            } => {
                self.gen_for_loop_body_from_foreign(
                    var,
                    collection,
                    loop_body,
                    ty,
                    return_type,
                    source_module,
                );
            }

            // Match expressions need switch statement generation
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
                ..
            } => {
                self.gen_match_body_from_foreign(scrutinee, arms, ty, return_type, source_module);
            }

            // Nil body - void function with no operations
            IrExpr::Literal {
                value: Literal::Nil,
                ..
            } => {
                // Nil function bodies mean "do nothing" - generate empty body for void functions
                // For functions with return types, this shouldn't happen, but handle gracefully
                if return_type.is_some() {
                    // Generate a default zero value return (shouldn't reach here in valid code)
                    self.write_line("return;");
                }
                // For void functions, an empty body is valid
            }

            // If expressions with block branches need statement-level handling
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => {
                // Use statement-level if when:
                // 1. Branches have statements, OR
                // 2. Result type is a struct (select() doesn't work with struct types in WGSL)
                let needs_statement_level = Self::branch_has_statements(then_branch)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::branch_has_statements(e))
                    || !self.can_use_select(ty, source_module);

                if needs_statement_level {
                    self.gen_if_body_from_foreign(
                        condition,
                        then_branch,
                        else_branch,
                        ty,
                        return_type,
                        source_module,
                    );
                } else {
                    // Simple if-else without statements - use select()
                    let expr_str = self.gen_expr_from_foreign(body, source_module);
                    if return_type.is_some() {
                        self.write_line(&format!("return {};", expr_str));
                    } else {
                        // For void returns, skip bare variable refs and nil placeholders
                        // WGSL doesn't allow bare identifiers as statements
                        self.write_expr_as_statement(&expr_str);
                    }
                }
            }

            // Other expressions can be returned directly
            _ => {
                let expr_str = self.gen_expr_from_foreign(body, source_module);
                // Flush any hoisted statements before the expression
                self.flush_hoisted_statements();
                if return_type.is_some() {
                    self.write_line(&format!("return {};", expr_str));
                } else {
                    // For void returns, skip bare variable refs and nil placeholders
                    // WGSL doesn't allow bare identifiers as statements
                    let is_bare_ident = expr_str
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_');
                    if !is_bare_ident
                        && !expr_str.is_empty()
                        && expr_str != "/* nil */"
                        && expr_str != "/* void */"
                    {
                        self.write_line(&format!("{};", expr_str));
                    }
                }
            }
        }
    }

    /// Collect variable names that are assigned to in an expression tree.
    /// Used to determine which variables need `var` instead of `let`.
    fn collect_assigned_vars(expr: &IrExpr, vars: &mut std::collections::HashSet<String>) {
        use crate::ir::IrBlockStatement;

        match expr {
            IrExpr::Block { statements, result, .. } => {
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Assign { target, value } => {
                            // Add the root variable of the assignment target
                            // Handle both direct references and indexed access (DictAccess)
                            Self::extract_root_var_from_target(target, vars);
                            Self::collect_assigned_vars(value, vars);
                        }
                        IrBlockStatement::Let { value, .. } => {
                            Self::collect_assigned_vars(value, vars);
                        }
                        IrBlockStatement::Expr(e) => {
                            Self::collect_assigned_vars(e, vars);
                        }
                    }
                }
                Self::collect_assigned_vars(result, vars);
            }
            IrExpr::If { condition, then_branch, else_branch, .. } => {
                Self::collect_assigned_vars(condition, vars);
                Self::collect_assigned_vars(then_branch, vars);
                if let Some(e) = else_branch {
                    Self::collect_assigned_vars(e, vars);
                }
            }
            IrExpr::For { collection, body, .. } => {
                Self::collect_assigned_vars(collection, vars);
                Self::collect_assigned_vars(body, vars);
            }
            IrExpr::Match { scrutinee, arms, .. } => {
                Self::collect_assigned_vars(scrutinee, vars);
                for arm in arms {
                    Self::collect_assigned_vars(&arm.body, vars);
                }
            }
            _ => {}
        }
    }

    /// Collect assigned variables from a slice of block statements.
    fn collect_assigned_vars_from_statements(
        statements: &[crate::ir::IrBlockStatement],
        result: &IrExpr,
        vars: &mut std::collections::HashSet<String>,
    ) {
        use crate::ir::IrBlockStatement;

        for stmt in statements {
            match stmt {
                IrBlockStatement::Assign { target, value } => {
                    // Handle both direct references and indexed access (DictAccess)
                    Self::extract_root_var_from_target(target, vars);
                    Self::collect_assigned_vars(value, vars);
                }
                IrBlockStatement::Let { value, .. } => {
                    Self::collect_assigned_vars(value, vars);
                }
                IrBlockStatement::Expr(e) => {
                    Self::collect_assigned_vars(e, vars);
                }
            }
        }
        Self::collect_assigned_vars(result, vars);
    }

    /// Extract the root variable name from an assignment target.
    ///
    /// Handles:
    /// - Direct references: `result` -> "result"
    /// - Indexed access: `result[i]` -> "result"
    /// - Field access: `obj.field` -> "obj"
    fn extract_root_var_from_target(
        target: &IrExpr,
        vars: &mut std::collections::HashSet<String>,
    ) {
        match target {
            IrExpr::Reference { path, .. } => {
                if !path.is_empty() {
                    vars.insert(path[0].clone());
                }
            }
            IrExpr::DictAccess { dict, .. } => {
                // For indexed access, extract from the dictionary/array expression
                Self::extract_root_var_from_target(dict, vars);
            }
            IrExpr::FieldAccess { object, .. } => {
                // For field access, extract from the object expression
                Self::extract_root_var_from_target(object, vars);
            }
            _ => {}
        }
    }

    /// Generate block body from a foreign module.
    fn gen_block_body_from_foreign(
        &mut self,
        statements: &[crate::ir::IrBlockStatement],
        result: &IrExpr,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        use crate::ir::IrBlockStatement;

        // First, collect all variables that are assigned to later
        let mut assigned_vars = std::collections::HashSet::new();
        Self::collect_assigned_vars_from_statements(statements, result, &mut assigned_vars);

        // Generate each statement
        for stmt in statements {
            match stmt {
                IrBlockStatement::Let {
                    name, value, ty, ..
                } => {
                    // Check if value is an if-else with blocks that needs statement-level handling
                    if let IrExpr::If {
                        condition,
                        then_branch,
                        else_branch,
                        ty: if_ty,
                        ..
                    } = value
                    {
                        // Use statement-level if when:
                        // 1. Branches have statements, OR
                        // 2. Result type is a struct (select() doesn't work with struct types)
                        let needs_statement_level = Self::branch_has_statements(then_branch)
                            || else_branch
                                .as_ref()
                                .map_or(false, |e| Self::branch_has_statements(e))
                            || !self.can_use_select(if_ty, source_module);

                        if needs_statement_level {
                            // Generate as var declaration + if/else assignment
                            // Use let binding type if available, otherwise infer from branch result
                            let ty_str = if let Some(t) = ty {
                                self.type_to_wgsl_from(t, source_module)
                            } else if matches!(if_ty, ResolvedType::Primitive(PrimitiveType::Never)) {
                                // If the if expression type is Never (unknown), infer from then branch
                                Self::get_branch_result_type_name(then_branch)
                                    .unwrap_or_else(|| "f32".to_string())
                            } else {
                                let base_type_str = self.type_to_wgsl_from(if_ty, source_module);
                                // Check if the type looks suspicious (f32 default, Number, etc.)
                                // Number is a generic numeric type that needs specialization
                                let is_suspicious = base_type_str == "f32"
                                    || matches!(if_ty, ResolvedType::Primitive(PrimitiveType::Number))
                                    || base_type_str.contains("UnknownElement")
                                    || base_type_str.contains('.');
                                if is_suspicious {
                                    // Try to infer from the then branch
                                    let inferred = Self::infer_concrete_type_from_expr(then_branch);
                                    // If then branch inference failed, try else branch
                                    let inferred = if inferred.is_none() {
                                        if let Some(else_branch) = else_branch {
                                            Self::infer_concrete_type_from_expr(else_branch)
                                        } else {
                                            None
                                        }
                                    } else {
                                        inferred
                                    };
                                    // If still failed, try method lookup on then branch
                                    let inferred = if inferred.is_none() {
                                        self.try_infer_type_from_method_call(then_branch, source_module)
                                    } else {
                                        inferred
                                    };
                                    if let Some(concrete) = inferred {
                                        let concrete_str = self.type_to_wgsl_from(&concrete, source_module);
                                        if concrete_str != "f32" && !concrete_str.contains("UnknownElement") {
                                            concrete_str
                                        } else {
                                            base_type_str
                                        }
                                    } else {
                                        base_type_str
                                    }
                                } else {
                                    base_type_str
                                }
                            };
                            self.write_line(&format!("var {}: {};", name, ty_str));
                            // Register the binding type BEFORE generating branches so empty arrays can use it
                            if let Some(t) = ty {
                                self.local_binding_types.insert(name.clone(), t.clone());
                            } else {
                                self.local_binding_types.insert(name.clone(), if_ty.clone());
                            }
                            self.gen_let_if_from_foreign(
                                name,
                                condition,
                                then_branch,
                                else_branch,
                                source_module,
                            );
                        } else {
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Flush any hoisted statements from the value expression
                            self.flush_hoisted_statements();
                            // Use var if variable is assigned to later, let otherwise
                            let binding_kw = if assigned_vars.contains(name) { "var" } else { "let" };
                            self.write_line(&format!("{} {} = {};", binding_kw, name, value_str));
                        }
                    }
                    // Check if value is a match that needs statement-level handling
                    else if let IrExpr::Match {
                        scrutinee,
                        arms,
                        ty: match_ty,
                        ..
                    } = value
                    {
                        // Generate as var declaration + switch assignment
                        let ty_str = if let Some(t) = ty {
                            self.type_to_wgsl_from(t, source_module)
                        } else {
                            self.type_to_wgsl_from(match_ty, source_module)
                        };
                        self.write_line(&format!("var {}: {};", name, ty_str));
                        self.gen_let_match_from_foreign(name, scrutinee, arms, source_module);
                    } else {
                        // Special handling for empty array literals with type annotation
                        // Use the Let's type annotation for proper WGSL type generation
                        let value_str =
                            if let (Some(let_ty), IrExpr::Array { elements, .. }) = (ty, value) {
                                if elements.is_empty() {
                                    // Use the Let statement's type annotation for the array
                                    if let ResolvedType::Array(inner) = let_ty {
                                        let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                    } else {
                                        self.gen_expr_from_foreign(value, source_module)
                                    }
                                } else {
                                    self.gen_expr_from_foreign(value, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(value, source_module)
                            };
                        // Flush any hoisted statements from the value expression
                        self.flush_hoisted_statements();
                        // Use var if variable is assigned to later, let otherwise
                        let binding_kw = if assigned_vars.contains(name) { "var" } else { "let" };
                        self.write_line(&format!("{} {} = {};", binding_kw, name, value_str));
                    }

                    // Register the binding type for method call resolution
                    // Resolve TypeParam types to actual types if they match function parameters
                    let binding_ty = ty.as_ref().cloned().unwrap_or_else(|| value.ty().clone());
                    let resolved_ty = if let ResolvedType::TypeParam(param_name) = &binding_ty {
                        // If TypeParam matches a function parameter, use that parameter's type
                        self.current_function_params
                            .get(simple_type_name(param_name))
                            .cloned()
                            .unwrap_or(binding_ty)
                    } else {
                        binding_ty
                    };
                    self.local_binding_types.insert(name.clone(), resolved_ty);
                }
                IrBlockStatement::Assign { target, value } => {
                    let target_str = self.gen_expr_from_foreign(target, source_module);
                    let value_str = self.gen_expr_from_foreign(value, source_module);
                    // Flush any hoisted statements from target/value expressions
                    self.flush_hoisted_statements();
                    self.write_line(&format!("{} = {};", target_str, value_str));
                }
                IrBlockStatement::Expr(expr) => {
                    // Handle for-loops specially since they need statement-level generation
                    if let IrExpr::For {
                        var,
                        var_ty,
                        collection,
                        body,
                        ..
                    } = expr
                    {
                        self.gen_imperative_for_from_foreign(
                            var,
                            var_ty,
                            collection,
                            body,
                            source_module,
                        );
                    } else {
                        let expr_str = self.gen_expr_from_foreign(expr, source_module);
                        // Flush any hoisted statements from the expression
                        self.flush_hoisted_statements();
                        self.write_expr_as_statement(&expr_str);
                    }
                }
            }
        }

        // Generate the result expression
        self.gen_function_body_from_foreign(result, return_type, source_module);
    }

    /// Generate an imperative for-loop (one that executes for side effects, not producing a result).
    ///
    /// This handles loops like:
    /// ```formalang
    /// for i in 0u..samples {
    ///     min_dist = min(min_dist, dist)
    /// }
    /// ```
    fn gen_imperative_for_from_foreign(
        &mut self,
        var: &str,
        var_ty: &ResolvedType,
        collection: &IrExpr,
        body: &IrExpr,
        source_module: &IrModule,
    ) {
        let var_ty_str = self.type_to_wgsl_from(var_ty, source_module);

        // Check if collection is a range expression (e.g., 0u..samples)
        // Range is represented as BinaryOp with Range operator
        if let IrExpr::BinaryOp {
            op: BinaryOperator::Range,
            left,
            right,
            ..
        } = collection
        {
            let start_str = self.gen_expr_from_foreign(left.as_ref(), source_module);
            let end_str = self.gen_expr_from_foreign(right.as_ref(), source_module);

            // For range expressions, the element type should be the type of the range bounds
            // The IR var_ty might be incorrectly inferred as f32 or UnknownElement
            // Priority: if literals end with 'u' suffix, use u32 (most reliable indicator)
            let actual_var_ty_str = if start_str.ends_with('u') || end_str.ends_with('u') {
                // Literal suffix is the most reliable type indicator
                "u32".to_string()
            } else if start_str.ends_with('i') || end_str.ends_with('i') {
                "i32".to_string()
            } else {
                // Fall back to bound type
                let bound_type = self.type_to_wgsl_from(left.ty(), source_module);
                if bound_type != "f32" && bound_type != "UnknownElement" {
                    bound_type
                } else {
                    "f32".to_string()
                }
            };

            // Check if bounds are constant integers and body contains array access with loop var
            // If so, unroll the loop to avoid WGSL's "index must be constant" restriction
            let needs_unroll = Self::body_has_dynamic_array_access(body, var);
            let start_val = Self::parse_int_literal(&start_str);
            let end_val = Self::parse_int_literal(&end_str);

            if needs_unroll && start_val.is_some() && end_val.is_some() {
                let start = start_val.unwrap();
                let end = end_val.unwrap();

                // Limit unroll size to prevent code explosion
                if end > start && (end - start) <= 256 {
                    // Unroll the loop
                    for i in start..end {
                        // Create a rename map that maps the loop variable to the current index
                        let mut renames: HashMap<String, String> = HashMap::new();
                        let idx_suffix = if actual_var_ty_str == "u32" { "u" } else { "" };
                        renames.insert(var.to_string(), format!("{}{}", i, idx_suffix));

                        let body_str =
                            self.gen_expr_with_renames_from_foreign(body, &renames, source_module);
                        // Flush any hoisted statements from the body
                        let hoisted: Vec<String> =
                            self.hoisted_statements.borrow_mut().drain(..).collect();
                        for stmt in hoisted {
                            self.write_line(&format!("{};", stmt));
                        }

                        if !body_str.is_empty()
                            && body_str != "/* nil */"
                            && body_str != "/* void */"
                        {
                            self.write_expr_as_statement(&body_str);
                        }
                    }
                    return;
                }
            }

            // Generate C-style for loop for range iteration
            self.write_line(&format!(
                "for (var {}: {} = {}; {} < {}; {} = {} + 1{}) {{",
                var,
                actual_var_ty_str,
                start_str,
                var,
                end_str,
                var,
                var,
                if actual_var_ty_str == "u32" { "u" } else { "" }
            ));
        } else {
            // For array iteration, use indexed access
            let coll_str = self.gen_expr_from_foreign(collection, source_module);
            let array_size = self.infer_array_size_from_foreign(collection, source_module);

            // For array iteration, infer element type from collection if var_ty is unknown/placeholder
            // Check the original var_ty before WGSL conversion (UnknownElement becomes f32 after conversion)
            let is_unknown_type = matches!(var_ty, ResolvedType::TypeParam(name) if name == "UnknownElement")
                || var_ty_str == "f32"; // Also infer if type defaulted to f32
            let actual_var_ty_str = if is_unknown_type {
                // Try to get element type from collection's type
                let inferred_type = match collection.ty() {
                    ResolvedType::Array(inner) => {
                        Some(self.type_to_wgsl_from(inner, source_module))
                    }
                    ResolvedType::TypeParam(name) => {
                        // TypeParam might be a reference to a parameter - look it up
                        if let Some(param_ty) = self.current_function_params.get(name) {
                            if let ResolvedType::Array(inner) = param_ty {
                                Some(self.type_to_wgsl_from(inner, source_module))
                            } else {
                                None
                            }
                        } else if name.starts_with('[') && name.ends_with(']') {
                            // Extract element type from "[Type]" format
                            let inner_type = &name[1..name.len() - 1];
                            Some(inner_type.to_string())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // If collection is a Reference to a parameter, look up its type
                let inferred_type = inferred_type.or_else(|| {
                    if let IrExpr::Reference { path, .. } = collection {
                        if path.len() == 1 {
                            if let Some(param_ty) = self.current_function_params.get(&path[0]) {
                                if let ResolvedType::Array(inner) = param_ty {
                                    return Some(self.type_to_wgsl_from(inner, source_module));
                                }
                            }
                        }
                    }
                    None
                });

                inferred_type.unwrap_or_else(|| "f32".to_string())
            } else {
                var_ty_str.clone()
            };

            // For array iteration loops, we need to unroll to avoid dynamic indexing
            // WGSL doesn't allow dynamic array indexing for function-local arrays
            if let Some(size) = array_size {
                if size <= 256 {
                    // Unroll the array iteration loop
                    for i in 0..size {
                        // Map the loop variable to the array access at this index
                        let mut renames: HashMap<String, String> = HashMap::new();
                        renames.insert(var.to_string(), format!("{}[{}u]", coll_str, i));

                        let body_str =
                            self.gen_expr_with_renames_from_foreign(body, &renames, source_module);
                        // Flush any hoisted statements from the body
                        let hoisted: Vec<String> =
                            self.hoisted_statements.borrow_mut().drain(..).collect();
                        for stmt in hoisted {
                            self.write_line(&format!("{};", stmt));
                        }

                        if !body_str.is_empty()
                            && body_str != "/* nil */"
                            && body_str != "/* void */"
                        {
                            self.write_expr_as_statement(&body_str);
                        }
                    }
                    return;
                }
            }

            // Fallback: generate a loop (this may fail WGSL validation if dynamic indexing is used)
            if let Some(size) = array_size {
                self.write_line(&format!("let _loop_arr = {};", coll_str));
                self.write_line(&format!(
                    "for (var _i: u32 = 0u; _i < {}u; _i = _i + 1u) {{",
                    size
                ));
                self.indent += 1;
                self.write_line(&format!("let {}: {} = _loop_arr[_i];", var, actual_var_ty_str));
                self.indent -= 1;
            } else {
                // Unknown size - use max bound
                self.write_line(&format!("let _loop_arr = {};", coll_str));
                self.write_line(&format!(
                    "for (var _i: u32 = 0u; _i < {}u; _i = _i + 1u) {{",
                    DEFAULT_MAX_ARRAY_SIZE
                ));
                self.indent += 1;
                self.write_line(&format!("let {}: {} = _loop_arr[_i];", var, actual_var_ty_str));
                self.indent -= 1;
            }
        }

        self.indent += 1;

        // Generate loop body - handle block expressions properly
        if let IrExpr::Block {
            statements, result, ..
        } = body
        {
            // Generate each statement in the loop body
            for stmt in statements {
                match stmt {
                    crate::ir::IrBlockStatement::Let { name, value, ty, .. } => {
                        // Skip nil-valued let bindings (e.g., unsupported closures)
                        if Self::expr_is_nil(value) {
                            continue;
                        }
                        // Handle closure values: generate a function instead of a let binding
                        if let IrExpr::Closure { params, body, .. } = value {
                            self.register_closure_from_foreign(name, params, body, source_module);
                            continue;
                        }
                        let value_str = self.gen_expr_from_foreign(value, source_module);
                        // Skip if the generated value is a nil placeholder
                        if value_str == "/* nil */" || value_str == "/* void */" {
                            continue;
                        }
                        self.write_line(&format!("let {} = {};", name, value_str));
                        // Register the binding type for method call resolution
                        let binding_ty = ty.as_ref().cloned().unwrap_or_else(|| value.ty().clone());
                        self.local_binding_types.insert(name.clone(), binding_ty);
                    }
                    crate::ir::IrBlockStatement::Assign { target, value } => {
                        let target_str = self.gen_expr_from_foreign(target, source_module);
                        let value_str = self.gen_expr_from_foreign(value, source_module);
                        self.write_line(&format!("{} = {};", target_str, value_str));
                    }
                    crate::ir::IrBlockStatement::Expr(expr) => {
                        // Handle If expressions with statement bodies specially
                        // to generate if-statements instead of select() expressions
                        if let IrExpr::If {
                            condition,
                            then_branch,
                            else_branch,
                            ..
                        } = expr
                        {
                            // Generate if-statement for imperative control flow
                            self.gen_if_statement_from_foreign(
                                condition,
                                then_branch,
                                else_branch.as_deref(),
                                source_module,
                            );
                        } else {
                            let expr_str = self.gen_expr_from_foreign(expr, source_module);
                            self.write_line(&format!("{};", expr_str));
                        }
                    }
                }
            }
            // The result of the loop body is typically unit/void for imperative loops
            // Don't generate a return statement for it
            if !matches!(
                result.as_ref(),
                IrExpr::Literal {
                    value: Literal::Nil,
                    ..
                }
            ) {
                // Special handling for if expressions as block result - treat as if-statement
                if let IrExpr::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } = result.as_ref()
                {
                    self.gen_if_statement_from_foreign(
                        condition,
                        then_branch,
                        else_branch.as_deref(),
                        source_module,
                    );
                } else {
                    let result_str = self.gen_expr_from_foreign(result.as_ref(), source_module);
                    if !result_str.is_empty() && result_str != "()" {
                        self.write_line(&format!("{};", result_str));
                    }
                }
            }
        } else if let IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = body
        {
            // If expression body - generate as if-statement
            self.gen_if_statement_from_foreign(
                condition,
                then_branch,
                else_branch.as_deref(),
                source_module,
            );
        } else {
            // Simple expression body
            let body_str = self.gen_expr_from_foreign(body, source_module);
            self.write_line(&format!("{};", body_str));
        }

        self.indent -= 1;
        self.write_line("}");
    }

    /// Generate an if-statement (not select expression) from a foreign module.
    ///
    /// This is used for imperative control flow where branches have side effects.
    fn gen_if_statement_from_foreign(
        &mut self,
        condition: &IrExpr,
        then_branch: &IrExpr,
        else_branch: Option<&IrExpr>,
        source_module: &IrModule,
    ) {
        let cond_str = self.gen_expr_from_foreign(condition, source_module);
        self.write_line(&format!("if ({}) {{", cond_str));
        self.indent += 1;

        // Generate then branch body
        self.gen_statement_body_from_foreign(then_branch, source_module);

        self.indent -= 1;

        if let Some(else_expr) = else_branch {
            self.write_line("} else {");
            self.indent += 1;

            // Generate else branch body
            self.gen_statement_body_from_foreign(else_expr, source_module);

            self.indent -= 1;
        }
        self.write_line("}");
    }

    /// Generate statement body from a foreign module expression.
    ///
    /// Handles Block expressions by generating each statement, and other expressions
    /// as simple statements.
    fn gen_statement_body_from_foreign(&mut self, expr: &IrExpr, source_module: &IrModule) {
        match expr {
            IrExpr::Block {
                statements, result, ..
            } => {
                for stmt in statements {
                    match stmt {
                        crate::ir::IrBlockStatement::Let { name, value, ty, .. } => {
                            // Skip nil-valued let bindings (e.g., unsupported closures)
                            if Self::expr_is_nil(value) {
                                continue;
                            }
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                self.register_closure_from_foreign(name, params, body, source_module);
                                continue;
                            }
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Skip if the generated value is a nil placeholder
                            if value_str == "/* nil */" || value_str == "/* void */" {
                                continue;
                            }
                            self.write_line(&format!("let {} = {};", name, value_str));
                            // Register the binding type for method call resolution
                            let binding_ty = ty.as_ref().cloned().unwrap_or_else(|| value.ty().clone());
                            self.local_binding_types.insert(name.clone(), binding_ty);
                        }
                        crate::ir::IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr_from_foreign(target, source_module);
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        crate::ir::IrBlockStatement::Expr(e) => {
                            // Recursively handle nested If statements
                            if let IrExpr::If {
                                condition,
                                then_branch,
                                else_branch,
                                ..
                            } = e
                            {
                                self.gen_if_statement_from_foreign(
                                    condition,
                                    then_branch,
                                    else_branch.as_deref(),
                                    source_module,
                                );
                            } else {
                                let e_str = self.gen_expr_from_foreign(e, source_module);
                                if !e_str.is_empty() && e_str != "/* nil */" {
                                    self.write_line(&format!("{};", e_str));
                                }
                            }
                        }
                    }
                }
                // Handle result if not nil
                match result.as_ref() {
                    IrExpr::Literal {
                        value: Literal::Nil,
                        ..
                    } => {
                        // Nil result - no code needed
                    }
                    IrExpr::If {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        // If as result - generate as statement
                        self.gen_if_statement_from_foreign(
                            condition,
                            then_branch,
                            else_branch.as_deref(),
                            source_module,
                        );
                    }
                    _ => {
                        let result_str = self.gen_expr_from_foreign(result.as_ref(), source_module);
                        if !result_str.is_empty() && result_str != "()" && result_str != "/* nil */" {
                            self.write_line(&format!("{};", result_str));
                        }
                    }
                }
            }
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Nested if - generate as statement
                self.gen_if_statement_from_foreign(
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    source_module,
                );
            }
            IrExpr::Literal {
                value: Literal::Nil,
                ..
            } => {
                // Nil - no code needed
            }
            _ => {
                // Simple expression
                let expr_str = self.gen_expr_from_foreign(expr, source_module);
                if !expr_str.is_empty() && expr_str != "/* nil */" {
                    self.write_line(&format!("{};", expr_str));
                }
            }
        }
    }

    /// Generate for loop body from a foreign module.
    fn gen_for_loop_body_from_foreign(
        &mut self,
        var: &str,
        collection: &IrExpr,
        loop_body: &IrExpr,
        ty: &ResolvedType,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        // Check if collection is a range expression (e.g., 0u..10u)
        if let IrExpr::BinaryOp {
            op: BinaryOperator::Range,
            left,
            right,
            ..
        } = collection
        {
            // Range-based for loop - generate C-style for loop
            let start_str = self.gen_expr_from_foreign(left, source_module);
            let end_str = self.gen_expr_from_foreign(right, source_module);

            // Check if bounds are constant integers and body contains array access with loop var
            // If so, unroll the loop to avoid WGSL's "index must be constant" restriction
            let needs_unroll = Self::body_has_dynamic_array_access(loop_body, var);
            let start_val = Self::parse_int_literal(&start_str);
            let end_val = Self::parse_int_literal(&end_str);
            if needs_unroll && start_val.is_some() && end_val.is_some() {
                let start = start_val.unwrap();
                let end = end_val.unwrap();

                // Limit unroll size to prevent code explosion
                if end > start && (end - start) <= 256 {
                    // Unroll the loop
                    for i in start..end {
                        // Create a rename map that maps the loop variable to the current index
                        let mut renames: HashMap<String, String> = HashMap::new();
                        let idx_suffix = if start_str.ends_with('u') { "u" } else { "" };
                        renames.insert(var.to_string(), format!("{}{}", i, idx_suffix));

                        let body_str =
                            self.gen_expr_with_renames_from_foreign(loop_body, &renames, source_module);
                        // Flush any hoisted statements from the body
                        let hoisted: Vec<String> =
                            self.hoisted_statements.borrow_mut().drain(..).collect();
                        for stmt in hoisted {
                            self.write_line(&format!("{};", stmt));
                        }

                        if !body_str.is_empty()
                            && body_str != "/* nil */"
                            && body_str != "/* void */"
                        {
                            self.write_expr_as_statement(&body_str);
                        }
                    }

                    if return_type.is_some() {
                        // Unrolled loops don't produce array results
                    }
                    return;
                }
            }

            // Determine the loop variable type from the bounds
            // Priority: if literals end with 'u' suffix, use u32 (most reliable indicator)
            let var_ty_str = if start_str.ends_with('u') || end_str.ends_with('u') {
                "u32".to_string()
            } else if start_str.ends_with('i') || end_str.ends_with('i') {
                "i32".to_string()
            } else {
                let bound_type = self.type_to_wgsl_from(left.ty(), source_module);
                if bound_type != "f32" && bound_type != "UnknownElement" {
                    bound_type
                } else {
                    "f32".to_string()
                }
            };

            // Generate C-style for loop
            self.write_line(&format!(
                "for (var {}: {} = {}; {} < {}; {} = {} + 1{}) {{",
                var,
                var_ty_str,
                start_str,
                var,
                end_str,
                var,
                var,
                if var_ty_str == "u32" { "u" } else { "" }
            ));
            self.indent += 1;

            let body_str = self.gen_expr_from_foreign(loop_body, source_module);
            // Flush any hoisted statements from the body
            let hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();
            for stmt in hoisted {
                self.write_line(&format!("{};", stmt));
            }

            // Only write body as statement if it's not void
            if !body_str.is_empty()
                && body_str != "/* nil */"
                && body_str != "/* void */"
                && body_str != "/* void for loop */"
            {
                self.write_expr_as_statement(&body_str);
            }

            self.indent -= 1;
            self.write_line("}");

            if return_type.is_some() {
                // Range-based loops don't produce array results
                // Return unit/void by not writing anything
            }
            return;
        }

        let coll_str = self.gen_expr_from_foreign(collection, source_module);

        // Determine the result element type from the loop body result type
        let result_elem_ty = match ty {
            ResolvedType::Array(inner) => self.type_to_wgsl_from(inner, source_module),
            _ => self.type_to_wgsl_from(ty, source_module),
        };

        // Try to infer the array size at compile time
        let array_size = self.infer_array_size_from_foreign(collection, source_module);

        match array_size {
            Some(size) => {
                // Known compile-time size - generate efficient fixed-size loop
                self.write_line(&format!("let input_arr = {};", coll_str));
                self.write_line(&format!("var result: array<{}, {}>;", result_elem_ty, size));

                self.write_line(&format!(
                    "for (var i: u32 = 0u; i < {}u; i = i + 1u) {{",
                    size
                ));
                self.indent += 1;

                self.write_line(&format!("let {} = input_arr[i];", var));
                let body_str = self.gen_expr_from_foreign(loop_body, source_module);
                self.write_line(&format!("result[i] = {};", body_str));

                self.indent -= 1;
                self.write_line("}");
            }
            None => {
                // Unknown size - generate with max size and comment
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

                // For unknown-sized arrays, use a bounded loop
                self.write_line(&format!(
                    "for (var i: u32 = 0u; i < {}u; i = i + 1u) {{",
                    DEFAULT_MAX_ARRAY_SIZE
                ));
                self.indent += 1;

                self.write_line(&format!("let {} = input_arr[i];", var));
                let body_str = self.gen_expr_from_foreign(loop_body, source_module);
                self.write_line(&format!("result[result_idx] = {};", body_str));
                self.write_line("result_idx = result_idx + 1u;");

                self.indent -= 1;
                self.write_line("}");
            }
        }

        if return_type.is_some() {
            self.write_line("return result;");
        }
    }

    /// Generate match body from a foreign module.
    fn gen_match_body_from_foreign(
        &mut self,
        scrutinee: &IrExpr,
        arms: &[crate::ir::IrMatchArm],
        _ty: &ResolvedType,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        let scrutinee_str = self.gen_expr_from_foreign(scrutinee, source_module);

        // Check if the enum TYPE has any variants with data
        // This determines if the enum is represented as a struct (with .discriminant)
        // or as a plain u32
        let enum_has_data_variants = match scrutinee.ty() {
            ResolvedType::Enum(id) => {
                let e = source_module.get_enum(*id);
                e.variants.iter().any(|v| !v.fields.is_empty())
            }
            ResolvedType::External { name, .. } => {
                // For external enums, try to find in imported modules
                // Use max_by_key to prefer enum with fields (re-exported enums may be empty)
                let simple_name = simple_type_name(name);
                self.imported_modules
                    .values()
                    .flat_map(|m| m.enums.iter())
                    .filter(|e| e.name == simple_name)
                    .max_by_key(|e| e.variants.iter().map(|v| v.fields.len()).sum::<usize>())
                    .map(|e| e.variants.iter().any(|v| !v.fields.is_empty()))
                    .unwrap_or(false)
            }
            _ => arms.iter().any(|arm| !arm.bindings.is_empty()),
        };

        // Declare match_result variable if we have a return type
        if let Some(ret_ty) = return_type {
            self.write_line(&format!("var match_result: {};", ret_ty));
        }

        // If enum has data variants, switch on discriminant; otherwise switch directly
        let switch_expr = if enum_has_data_variants {
            format!("{}.discriminant", scrutinee_str)
        } else {
            scrutinee_str.clone()
        };

        self.write_line(&format!("switch {} {{", switch_expr));
        self.indent += 1;

        // Separate wildcard arm from variant arms
        let (variant_arms, wildcard_arms): (Vec<_>, Vec<_>) =
            arms.iter().partition(|arm| !arm.is_wildcard);

        // Generate case for each variant arm - use index as numeric tag
        for (idx, arm) in variant_arms.iter().enumerate() {
            let tag = idx as u32;
            self.write_line(&format!("case {}u: {{ // {}", tag, arm.variant));
            self.indent += 1;

            // Bind pattern variables by extracting from data array
            let mut data_offset = 0;
            for (name, ty) in arm.bindings.iter() {
                let load_expr =
                    self.gen_binding_load_expr_from_data(&scrutinee_str, data_offset, ty, source_module);
                self.write_line(&format!("let {} = {};", name, load_expr));
                data_offset += self.binding_type_size_in_f32(ty, source_module);
            }

            // Set up local binding types for method call resolution
            for (name, ty) in &arm.bindings {
                self.local_binding_types.insert(name.clone(), ty.clone());
            }

            // Handle arm body - properly emit let statements for Block bodies
            self.gen_match_arm_body_from_foreign(&arm.body, return_type, source_module);

            // Clear local binding types after processing arm
            self.local_binding_types.clear();

            self.indent -= 1;
            self.write_line("}");
        }

        // Generate default case (either from wildcard arm or empty)
        if let Some(wildcard_arm) = wildcard_arms.first() {
            self.write_line("default: {");
            self.indent += 1;
            self.gen_match_arm_body_from_foreign(&wildcard_arm.body, return_type, source_module);
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

    /// Generate a load expression for a binding from an enum's data array.
    ///
    /// For complex types (enums, structs), this reconstructs the full type from
    /// the data array at the given offset.
    fn gen_binding_load_expr_from_data(
        &self,
        scrutinee: &str,
        offset: u32,
        ty: &ResolvedType,
        source_module: &IrModule,
    ) -> String {
        match ty {
            ResolvedType::Primitive(p) => {
                let base = format!("{}.data[{}]", scrutinee, offset);
                match p {
                    PrimitiveType::F32 => base,
                    PrimitiveType::I32 => format!("i32(bitcast<i32>({}))", base),
                    PrimitiveType::U32 => format!("u32(bitcast<u32>({}))", base),
                    PrimitiveType::Bool => format!("{} != 0.0", base),
                    // String is stored as u32 in WGSL (not supported natively)
                    PrimitiveType::String => format!("u32(bitcast<u32>({}))", base),
                    _ => base,
                }
            }
            ResolvedType::Enum(id) => {
                // Look up enum in the source module (the module that defines this type)
                if (id.0 as usize) < source_module.enums.len() {
                    let e = source_module.get_enum(*id);
                    let safe_name = to_wgsl_identifier(&e.name);
                    // Calculate max data size by summing f32 sizes of fields
                    let max_variant_size: u32 = e
                        .variants
                        .iter()
                        .map(|v| {
                            v.fields
                                .iter()
                                .map(|f| self.binding_type_size_in_f32(&f.ty, source_module))
                                .sum::<u32>()
                        })
                        .max()
                        .unwrap_or(0);

                    // For unit enums, just return the discriminant
                    // Use u32() conversion since discriminant is stored as float value
                    if max_variant_size == 0 {
                        return format!(
                            "u32({}.data[{}])",
                            scrutinee, offset
                        );
                    }

                    let discriminant =
                        format!("u32({}.data[{}])", scrutinee, offset);
                    let data_loads: Vec<String> = (0..max_variant_size)
                        .map(|i| format!("{}.data[{}]", scrutinee, offset + 1 + i as u32))
                        .collect();

                    return format!(
                        "{}({}, array<f32, {}>({}))",
                        safe_name, discriminant, max_variant_size, data_loads.join(", ")
                    );
                }
                // Fallback
                format!("{}.data[{}]", scrutinee, offset)
            }
            ResolvedType::TypeParam(name) => {
                // Check if it's an enum - search with module context for proper field size calculation
                for enum_source_module in self.imported_modules.values() {
                    if let Some(e) = enum_source_module
                        .enums
                        .iter()
                        .find(|e| e.name == *name || e.name.ends_with(&format!("::{}", name)))
                    {
                        let safe_name = to_wgsl_identifier(&e.name);
                        // Calculate max data size by summing f32 sizes of fields
                        let max_variant_size: u32 = e
                            .variants
                            .iter()
                            .map(|v| {
                                v.fields
                                    .iter()
                                    .map(|f| self.type_size_in_f32(&f.ty, Some(enum_source_module)))
                                    .sum::<u32>()
                            })
                            .max()
                            .unwrap_or(0);

                        // For unit enums, just return the discriminant
                        // Use u32() conversion since discriminant is stored as float value
                        if max_variant_size == 0 {
                            return format!("u32({}.data[{}])", scrutinee, offset);
                        }

                        let discriminant = format!("u32({}.data[{}])", scrutinee, offset);
                        let data_loads: Vec<String> = (0..max_variant_size)
                            .map(|i| format!("{}.data[{}]", scrutinee, offset + 1 + i as u32))
                            .collect();

                        return format!(
                            "{}({}, array<f32, {}>({}))",
                            safe_name, discriminant, max_variant_size, data_loads.join(", ")
                        );
                    }
                }

                // Check if it's a struct
                if let Some(s) = source_module
                    .structs
                    .iter()
                    .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)))
                    .or_else(|| {
                        self.imported_modules
                            .values()
                            .flat_map(|m| m.structs.iter())
                            .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)))
                    })
                {
                    let safe_name = to_wgsl_identifier(&s.name);
                    let mut field_loads = Vec::new();
                    let mut field_offset = offset;
                    for field in &s.fields {
                        field_loads.push(self.gen_binding_load_expr_from_data(
                            scrutinee,
                            field_offset,
                            &field.ty,
                            source_module,
                        ));
                        field_offset += self.binding_type_size_in_f32(&field.ty, source_module);
                    }
                    return format!("{}({})", safe_name, field_loads.join(", "));
                }

                // Fallback
                format!("{}.data[{}]", scrutinee, offset)
            }
            _ => format!("{}.data[{}]", scrutinee, offset),
        }
    }

    /// Calculate the size in f32 units for a binding type.
    /// Delegates to `type_size_in_f32` with the source module.
    fn binding_type_size_in_f32(&self, ty: &ResolvedType, source_module: &IrModule) -> u32 {
        self.type_size_in_f32(ty, Some(source_module))
    }

    /// Generate match arm body from a foreign module.
    ///
    /// Handles block expressions by emitting let statements, similar to gen_branch_body_from_foreign.
    fn gen_match_arm_body_from_foreign(
        &mut self,
        body: &IrExpr,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        use crate::ir::IrBlockStatement;

        match body {
            IrExpr::Block {
                statements, result, ..
            } => {
                // Generate statements first (let bindings, assignments, etc.)
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Let { name, value, .. } => {
                            // Skip nil-valued let bindings (e.g., unsupported closures)
                            if Self::expr_is_nil(value) {
                                continue;
                            }
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                self.register_closure_from_foreign(name, params, body, source_module);
                                continue;
                            }
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Flush any hoisted statements from the value expression
                            self.flush_hoisted_statements();
                            // Skip if the generated value is a nil placeholder
                            if value_str == "/* nil */" || value_str == "/* void */" {
                                continue;
                            }
                            self.write_line(&format!("let {} = {};", name, value_str));
                        }
                        IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr_from_foreign(target, source_module);
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Flush any hoisted statements from target/value expressions
                            self.flush_hoisted_statements();
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        IrBlockStatement::Expr(expr) => {
                            let expr_str = self.gen_expr_from_foreign(expr, source_module);
                            // Flush any hoisted statements from the expression
                            self.flush_hoisted_statements();
                            self.write_expr_as_statement(&expr_str);
                        }
                    }
                }

                // Then generate the result expression
                let result_str = self.gen_expr_from_foreign(result, source_module);
                // Flush any hoisted statements from the result expression
                self.flush_hoisted_statements();
                if return_type.is_some() {
                    self.write_line(&format!("match_result = {};", result_str));
                } else {
                    self.write_expr_as_statement(&result_str);
                }
            }
            _ => {
                // Non-block expression - just generate as expression
                let body_str = self.gen_expr_from_foreign(body, source_module);
                // Flush any hoisted statements from the body expression
                self.flush_hoisted_statements();
                if return_type.is_some() {
                    self.write_line(&format!("match_result = {};", body_str));
                } else {
                    self.write_expr_as_statement(&body_str);
                }
            }
        }
    }

    /// Generate if/else body from a foreign module.
    ///
    /// Handles if-else expressions at statement level, properly preserving
    /// let bindings in block branches that would be lost with select().
    fn gen_if_body_from_foreign(
        &mut self,
        condition: &IrExpr,
        then_branch: &IrExpr,
        else_branch: &Option<Box<IrExpr>>,
        ty: &ResolvedType,
        return_type: Option<&str>,
        source_module: &IrModule,
    ) {
        let cond_str = self.gen_expr_from_foreign(condition, source_module);

        // Declare result variable if we have a return type
        if let Some(ret_ty) = return_type {
            self.write_line(&format!("var if_result: {};", ret_ty));
            // Store the return type string in local_binding_types for empty array handling
            // We use a special key to store the string representation
            // Parse element type from "array<ElemType, N>" format if applicable
            if ret_ty.starts_with("array<") {
                // Store a placeholder that indicates this is an array type with the string
                self.local_binding_types.insert(
                    "__if_result_type_str".to_string(),
                    ResolvedType::TypeParam(ret_ty.to_string()),
                );
            }
            self.local_binding_types
                .insert("if_result".to_string(), ty.clone());
        }

        // Generate if statement
        self.write_line(&format!("if ({}) {{", cond_str));
        self.indent += 1;

        // Generate then branch
        self.gen_branch_body_from_foreign(then_branch, return_type.is_some(), source_module);

        self.indent -= 1;

        // Generate else branch if present
        if let Some(else_expr) = else_branch {
            self.write_line("} else {");
            self.indent += 1;
            self.gen_branch_body_from_foreign(else_expr, return_type.is_some(), source_module);
            self.indent -= 1;
        }

        self.write_line("}");

        if return_type.is_some() {
            self.write_line("return if_result;");
        }
    }

    /// Generate branch body from a foreign module.
    ///
    /// Handles block expressions by emitting statements, or simple expressions
    /// by assigning to if_result.
    fn gen_branch_body_from_foreign(
        &mut self,
        branch: &IrExpr,
        has_return: bool,
        source_module: &IrModule,
    ) {
        use crate::ir::IrBlockStatement;

        match branch {
            IrExpr::Block {
                statements, result, ..
            } => {
                // Generate statements first
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Let { name, value, ty, .. } => {
                            // Skip nil-valued let bindings (e.g., unsupported closures)
                            if Self::expr_is_nil(value) {
                                continue;
                            }
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                self.register_closure_from_foreign(name, params, body, source_module);
                                continue;
                            }
                            // Special handling for empty arrays with type annotation
                            let value_str = if let IrExpr::Array { elements, ty: arr_ty } = value {
                                if elements.is_empty() {
                                    // Try the Let's type annotation first
                                    if let Some(let_ty) = ty {
                                        if let ResolvedType::Array(inner) = let_ty {
                                            let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                            format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                        } else {
                                            self.gen_expr_from_foreign(value, source_module)
                                        }
                                    } else if let ResolvedType::Array(inner) = arr_ty {
                                        let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                    } else {
                                        self.gen_expr_from_foreign(value, source_module)
                                    }
                                } else {
                                    self.gen_expr_from_foreign(value, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(value, source_module)
                            };
                            // Flush any hoisted statements BEFORE the let binding
                            self.flush_hoisted_statements();
                            // Skip if the generated value is a nil placeholder
                            if value_str == "/* nil */" || value_str == "/* void */" {
                                continue;
                            }
                            self.write_line(&format!("let {} = {};", name, value_str));
                            // Register the binding type for later use
                            if let Some(t) = ty {
                                self.local_binding_types.insert(name.clone(), t.clone());
                            } else {
                                self.local_binding_types.insert(name.clone(), value.ty().clone());
                            }
                        }
                        IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr_from_foreign(target, source_module);
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Flush any hoisted statements BEFORE the assignment
                            self.flush_hoisted_statements();
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        IrBlockStatement::Expr(expr) => {
                            let expr_str = self.gen_expr_from_foreign(expr, source_module);
                            if !expr_str.is_empty() && expr_str != "/* nil */" && expr_str != "/* void */" {
                                self.write_line(&format!("{};", expr_str));
                            }
                        }
                    }
                }
                // Generate result expression
                if has_return {
                    // Special handling for empty arrays
                    let result_str = if let IrExpr::Array { elements, ty } = result.as_ref() {
                        if elements.is_empty() {
                            if let ResolvedType::Array(inner) = ty {
                                let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                            } else if let Some(var_ty) = self.local_binding_types.get("if_result") {
                                if let ResolvedType::Array(inner) = var_ty {
                                    let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                    format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                } else {
                                    self.gen_expr_from_foreign(result, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(result, source_module)
                            }
                        } else {
                            self.gen_expr_from_foreign(result, source_module)
                        }
                    } else {
                        self.gen_expr_from_foreign(result, source_module)
                    };
                    // Flush any hoisted statements from the result expression
                    // (e.g., from nested if-expressions that need hoisting)
                    let hoisted: Vec<String> = self.hoisted_statements.borrow_mut().drain(..).collect();
                    for stmt in hoisted {
                        self.write_line(&format!("{};", stmt));
                    }
                    self.write_line(&format!("if_result = {};", result_str));
                }
            }
            // Nested if - generate inline if/else without redeclaring if_result
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_str = self.gen_expr_from_foreign(condition, source_module);
                self.write_line(&format!("if ({}) {{", cond_str));
                self.indent += 1;
                self.gen_branch_body_from_foreign(then_branch, has_return, source_module);
                self.indent -= 1;
                if let Some(else_expr) = else_branch {
                    self.write_line("} else {");
                    self.indent += 1;
                    self.gen_branch_body_from_foreign(else_expr, has_return, source_module);
                    self.indent -= 1;
                }
                self.write_line("}");
            }
            // Simple expression
            _ => {
                if has_return {
                    // Special handling for empty arrays
                    let expr_str = if let IrExpr::Array { elements, ty } = branch {
                        if elements.is_empty() {
                            // First, try to get element type from the stored type string
                            // This is the most reliable source as it comes from the actual return type
                            if let Some(ResolvedType::TypeParam(type_str)) =
                                self.local_binding_types.get("__if_result_type_str")
                            {
                                // Parse element type from "array<ElemType, N>" format
                                if type_str.starts_with("array<") {
                                    if let Some(comma_pos) = type_str.find(',') {
                                        let elem_ty = &type_str[6..comma_pos].trim();
                                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                    } else {
                                        self.gen_expr_from_foreign(branch, source_module)
                                    }
                                } else {
                                    self.gen_expr_from_foreign(branch, source_module)
                                }
                            } else if let ResolvedType::Array(inner) = ty {
                                let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                            } else if let Some(var_ty) = self.local_binding_types.get("if_result") {
                                if let ResolvedType::Array(inner) = var_ty {
                                    let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                    format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                } else {
                                    self.gen_expr_from_foreign(branch, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(branch, source_module)
                            }
                        } else {
                            self.gen_expr_from_foreign(branch, source_module)
                        }
                    } else {
                        self.gen_expr_from_foreign(branch, source_module)
                    };
                    self.write_line(&format!("if_result = {};", expr_str));
                }
            }
        }
    }

    /// Check if an expression branch has statements (is a block with non-empty statements).
    fn branch_has_statements(expr: &IrExpr) -> bool {
        match expr {
            IrExpr::Block { statements, .. } => !statements.is_empty(),
            IrExpr::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::branch_has_statements(then_branch)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::branch_has_statements(e))
            }
            _ => false,
        }
    }

    /// Check if a loop body contains dynamic array access using the loop variable.
    /// This is used to decide whether to unroll the loop to satisfy WGSL's
    /// "index must be constant" restriction for fixed-size arrays.
    fn body_has_dynamic_array_access(expr: &IrExpr, loop_var: &str) -> bool {
        match expr {
            IrExpr::DictAccess { key, .. } => {
                // Check if the key is a reference to the loop variable
                if let IrExpr::Reference { path, .. } = key.as_ref() {
                    if path.len() == 1 && path[0] == loop_var {
                        return true;
                    }
                }
                Self::body_has_dynamic_array_access(key, loop_var)
            }
            IrExpr::Block {
                statements, result, ..
            } => {
                for stmt in statements {
                    match stmt {
                        crate::ir::IrBlockStatement::Let { value, .. } => {
                            if Self::body_has_dynamic_array_access(value, loop_var) {
                                return true;
                            }
                        }
                        crate::ir::IrBlockStatement::Assign { target, value } => {
                            if Self::body_has_dynamic_array_access(target, loop_var)
                                || Self::body_has_dynamic_array_access(value, loop_var)
                            {
                                return true;
                            }
                        }
                        crate::ir::IrBlockStatement::Expr(e) => {
                            if Self::body_has_dynamic_array_access(e, loop_var) {
                                return true;
                            }
                        }
                    }
                }
                Self::body_has_dynamic_array_access(result, loop_var)
            }
            IrExpr::BinaryOp { left, right, .. } => {
                Self::body_has_dynamic_array_access(left, loop_var)
                    || Self::body_has_dynamic_array_access(right, loop_var)
            }
            IrExpr::FunctionCall { args, .. } => args
                .iter()
                .any(|(_, arg)| Self::body_has_dynamic_array_access(arg, loop_var)),
            IrExpr::MethodCall { receiver, args, .. } => {
                Self::body_has_dynamic_array_access(receiver, loop_var)
                    || args
                        .iter()
                        .any(|(_, arg)| Self::body_has_dynamic_array_access(arg, loop_var))
            }
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::body_has_dynamic_array_access(condition, loop_var)
                    || Self::body_has_dynamic_array_access(then_branch, loop_var)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::body_has_dynamic_array_access(e, loop_var))
            }
            _ => false,
        }
    }

    /// Parse an integer literal from a WGSL literal string.
    /// Returns None if the string is not an integer literal.
    fn parse_int_literal(s: &str) -> Option<i64> {
        // Remove suffix (u, i, etc.)
        let numeric = s.trim_end_matches(|c: char| c.is_alphabetic());
        numeric.parse::<i64>().ok()
    }

    /// Check if an expression evaluates to nil (void result).
    fn expr_is_nil(expr: &IrExpr) -> bool {
        match expr {
            IrExpr::Literal {
                value: Literal::Nil,
                ..
            } => true,
            IrExpr::Block { result, .. } => Self::expr_is_nil(result),
            IrExpr::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_is_nil(then_branch)
                    && else_branch
                        .as_ref()
                        .map_or(true, |e| Self::expr_is_nil(e))
            }
            _ => false,
        }
    }

    /// Try to extract a concrete type from an expression by looking at its structure.
    ///
    /// This is useful when the IR expression type is unreliable (e.g., stored as a variable name).
    /// We look for method calls, struct constructors, and other expressions that have clear return types.
    fn infer_concrete_type_from_expr(expr: &IrExpr) -> Option<ResolvedType> {
        // Helper to unwrap optional types
        fn unwrap_optional(ty: &ResolvedType) -> &ResolvedType {
            if let ResolvedType::Optional(inner) = ty {
                unwrap_optional(inner)
            } else {
                ty
            }
        }

        // Helper to check if a type is concrete (not a suspicious placeholder)
        fn is_concrete_type(ty: &ResolvedType) -> bool {
            match ty {
                ResolvedType::TypeParam(name) => {
                    // Lowercase names are suspicious (variable names, not type names)
                    !name.chars().next().map_or(false, |c| c.is_lowercase())
                }
                ResolvedType::Optional(inner) => is_concrete_type(inner),
                ResolvedType::External { .. } => true,
                ResolvedType::Primitive(_) => true,
                ResolvedType::Struct(_) => true,
                ResolvedType::Enum(_) => true,
                ResolvedType::Array(inner) => is_concrete_type(inner),
                _ => true,
            }
        }

        match expr {
            // Method calls have a return type stored in ty field
            IrExpr::MethodCall { ty, .. } => {
                let unwrapped = unwrap_optional(ty);
                if is_concrete_type(unwrapped) {
                    Some(unwrapped.clone())
                } else {
                    None
                }
            }
            // Struct constructors return the struct type
            IrExpr::StructInst { ty, .. } => {
                let unwrapped = unwrap_optional(ty);
                Some(unwrapped.clone())
            }
            // Enum variant instantiations return the enum type
            IrExpr::EnumInst { ty, .. } => {
                let unwrapped = unwrap_optional(ty);
                Some(unwrapped.clone())
            }
            // Function calls have a stored type
            IrExpr::FunctionCall { ty, path, args, .. } => {
                let unwrapped = unwrap_optional(ty);
                // Check if the type is concrete
                if is_concrete_type(unwrapped)
                    && !matches!(unwrapped, ResolvedType::Primitive(PrimitiveType::Number))
                    && !matches!(unwrapped, ResolvedType::Primitive(PrimitiveType::Never))
                {
                    Some(unwrapped.clone())
                } else {
                    // For builtins like min/max, infer from argument types
                    let fn_name = path.last().map(|s| s.as_str()).unwrap_or("");

                    // Check if this looks like a struct constructor (PascalCase name with named args)
                    // Struct constructors in FormaLang are represented as FunctionCall with the struct name.
                    // Heuristic: PascalCase function name + at least one named argument.
                    // This correctly identifies `Size2D(width: 100.0, height: 50.0)` as a struct.
                    // Note: Could false-positive on PascalCase functions with named args, but these
                    // are rare in practice and the fallback (None) handles incorrect inference gracefully.
                    let looks_like_struct_constructor =
                        fn_name.chars().next().map_or(false, |c| c.is_uppercase())
                            && args.iter().any(|(name, _)| name.is_some());

                    if looks_like_struct_constructor {
                        // Return TypeParam with struct name - this will be resolved by type_to_wgsl_from
                        return Some(ResolvedType::TypeParam(fn_name.to_string()));
                    }

                    if matches!(fn_name, "min" | "max" | "clamp") && !args.is_empty() {
                        // These functions return the same type as their arguments
                        // Try each argument to find a concrete type
                        for (_, arg) in args {
                            let arg_ty = arg.ty();
                            let unwrapped_arg = unwrap_optional(arg_ty);
                            // Check if it's a concrete primitive or External
                            if let ResolvedType::Primitive(p) = unwrapped_arg {
                                if !matches!(p, PrimitiveType::Number) {
                                    return Some(unwrapped_arg.clone());
                                }
                            } else if is_concrete_type(unwrapped_arg) {
                                return Some(unwrapped_arg.clone());
                            }
                        }
                    }
                    None
                }
            }
            // Blocks - check the result expression
            IrExpr::Block { result, .. } => Self::infer_concrete_type_from_expr(result),
            // If expressions - try both branches
            IrExpr::If {
                then_branch,
                else_branch,
                ..
            } => {
                // Try then branch first
                if let Some(ty) = Self::infer_concrete_type_from_expr(then_branch) {
                    return Some(ty);
                }
                // If then branch didn't work, try else branch
                if let Some(else_expr) = else_branch {
                    return Self::infer_concrete_type_from_expr(else_expr);
                }
                None
            }
            // Use the expression's stored type if it's usable
            _ => {
                let ty = expr.ty();
                let unwrapped = unwrap_optional(ty);
                match unwrapped {
                    ResolvedType::External { kind, .. } => {
                        if matches!(kind, crate::ir::ExternalKind::Struct) {
                            Some(unwrapped.clone())
                        } else {
                            None
                        }
                    }
                    // Return concrete primitive types (except Number which needs inference)
                    ResolvedType::Primitive(p) if !matches!(p, PrimitiveType::Number) => {
                        Some(unwrapped.clone())
                    }
                    _ => None,
                }
            }
        }
    }

    /// Check if a type can be used with WGSL's select() function.
    ///
    /// select() works with scalar, vector, and matrix types, but not structs.
    fn can_use_select(&self, ty: &ResolvedType, source_module: &IrModule) -> bool {
        match ty {
            ResolvedType::Primitive(p) => {
                use PrimitiveType::*;
                matches!(
                    p,
                    F32 | I32
                        | U32
                        | Bool
                        | Vec2
                        | Vec3
                        | Vec4
                        | IVec2
                        | IVec3
                        | IVec4
                        | UVec2
                        | UVec3
                        | UVec4
                        | Mat2
                        | Mat3
                        | Mat4
                        | Boolean
                )
            }
            // Struct types cannot be used with select()
            ResolvedType::Struct(_) => false,
            // External struct/trait types cannot be used with select()
            ResolvedType::External { kind, name, .. } => {
                // Check the kind field directly - structs and traits cannot use select
                if matches!(
                    kind,
                    crate::ir::ExternalKind::Struct | crate::ir::ExternalKind::Trait
                ) {
                    return false;
                }
                let simple_name = simple_type_name(name);
                // Also check if it's a struct in source module or imported modules
                let is_struct = source_module
                    .structs
                    .iter()
                    .any(|s| simple_type_name(&s.name) == simple_name)
                    || self
                        .imported_modules
                        .values()
                        .any(|m| m.structs.iter().any(|s| simple_type_name(&s.name) == simple_name));
                !is_struct
            }
            // TypeParam might refer to a struct - check if it matches any known struct
            // Default to false (cannot use select) if type is unknown, since it might be a struct
            ResolvedType::TypeParam(name) => {
                let simple_name = simple_type_name(name);
                // Check if it's a known primitive type (scalar/vector) that can use select
                let is_primitive_type = matches!(
                    simple_name,
                    "f32" | "i32" | "u32" | "bool" | "vec2" | "vec3" | "vec4"
                );
                if is_primitive_type {
                    return true;
                }
                // For non-primitive types, default to false (cannot use select)
                // since they are likely structs or other unsupported types
                false
            }
            // Enums might be represented as structs with data arrays
            ResolvedType::Enum(_) => false,
            // Optional types wrap inner types - check inner
            ResolvedType::Optional(inner) => self.can_use_select(inner, source_module),
            // Arrays and other types don't work with select
            _ => false,
        }
    }

    /// Get the WGSL type name from a branch expression.
    ///
    /// For blocks, gets the type from the result expression.
    /// Handles cases where IR types are `Never` by inferring from expression structure.
    fn get_branch_result_type_name(expr: &IrExpr) -> Option<String> {
        match expr {
            IrExpr::Block { result, .. } => Self::get_branch_result_type_name(result),
            // For FunctionCall, the path often indicates the return type
            // e.g., Color4(...) or Color4::transparent()
            IrExpr::FunctionCall { path, .. } => {
                if !path.is_empty() {
                    // Use the first segment as the type name
                    Some(path[0].clone())
                } else {
                    None
                }
            }
            // For StructInst, use the struct type
            IrExpr::StructInst { ty, .. } => match ty {
                ResolvedType::Struct(_) | ResolvedType::External { .. } | ResolvedType::TypeParam(_) => {
                    // The type name is embedded in the type
                    Some(Self::type_to_simple_name(ty))
                }
                _ => None,
            },
            // For If expressions, recurse into the then branch
            IrExpr::If { then_branch, .. } => Self::get_branch_result_type_name(then_branch),
            // Fallback to the expression type if it's not Never
            _ => {
                let ty = expr.ty();
                if !matches!(ty, ResolvedType::Primitive(PrimitiveType::Never)) {
                    Some(Self::type_to_simple_name(ty))
                } else {
                    None
                }
            }
        }
    }

    /// Convert a ResolvedType to a simple type name string.
    fn type_to_simple_name(ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => {
                use PrimitiveType::*;
                match p {
                    F32 => "f32",
                    I32 => "i32",
                    U32 => "u32",
                    Bool | Boolean => "bool",
                    Vec2 => "vec2<f32>",
                    Vec3 => "vec3<f32>",
                    Vec4 => "vec4<f32>",
                    _ => "f32",
                }
                .to_string()
            }
            ResolvedType::Struct(_) => "struct".to_string(), // Shouldn't happen
            ResolvedType::External { name, .. } => simple_type_name(name).to_string(),
            ResolvedType::TypeParam(name) => simple_type_name(name).to_string(),
            ResolvedType::Optional(inner) => format!("Optional_{}", Self::type_to_simple_name(inner)),
            _ => "f32".to_string(),
        }
    }

    /// Generate if/else for a let statement from a foreign module.
    ///
    /// Generates `if (cond) { var = then_val; } else { var = else_val; }`
    /// where the branches may contain statements that get properly emitted.
    fn gen_let_if_from_foreign(
        &mut self,
        var_name: &str,
        condition: &IrExpr,
        then_branch: &IrExpr,
        else_branch: &Option<Box<IrExpr>>,
        source_module: &IrModule,
    ) {
        let cond_str = self.gen_expr_from_foreign(condition, source_module);

        self.write_line(&format!("if ({}) {{", cond_str));
        self.indent += 1;
        self.gen_let_branch_from_foreign(var_name, then_branch, source_module);
        self.indent -= 1;

        if let Some(else_expr) = else_branch {
            self.write_line("} else {");
            self.indent += 1;
            self.gen_let_branch_from_foreign(var_name, else_expr, source_module);
            self.indent -= 1;
        }

        self.write_line("}");
    }

    /// Generate a branch body for a let if/else, assigning to var_name.
    fn gen_let_branch_from_foreign(
        &mut self,
        var_name: &str,
        branch: &IrExpr,
        source_module: &IrModule,
    ) {
        use crate::ir::IrBlockStatement;

        match branch {
            IrExpr::Block {
                statements, result, ..
            } => {
                // Generate statements first
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Let { name, value, ty, .. } => {
                            // Skip nil-valued let bindings (e.g., unsupported closures)
                            if Self::expr_is_nil(value) {
                                continue;
                            }
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                self.register_closure_from_foreign(name, params, body, source_module);
                                continue;
                            }
                            // Special handling for empty arrays with type annotation
                            let value_str = if let IrExpr::Array { elements, ty: arr_ty } = value {
                                if elements.is_empty() {
                                    // Try the Let's type annotation first
                                    if let Some(let_ty) = ty {
                                        if let ResolvedType::Array(inner) = let_ty {
                                            let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                            format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                        } else {
                                            self.gen_expr_from_foreign(value, source_module)
                                        }
                                    } else if let ResolvedType::Array(inner) = arr_ty {
                                        let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                    } else {
                                        self.gen_expr_from_foreign(value, source_module)
                                    }
                                } else {
                                    self.gen_expr_from_foreign(value, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(value, source_module)
                            };
                            // Flush any hoisted statements BEFORE the let binding
                            self.flush_hoisted_statements();
                            // Skip if the generated value is a nil placeholder
                            if value_str == "/* nil */" || value_str == "/* void */" {
                                continue;
                            }
                            self.write_line(&format!("let {} = {};", name, value_str));
                            // Register the binding type for later use
                            if let Some(t) = ty {
                                self.local_binding_types.insert(name.clone(), t.clone());
                            } else {
                                self.local_binding_types.insert(name.clone(), value.ty().clone());
                            }
                        }
                        IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr_from_foreign(target, source_module);
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Flush any hoisted statements BEFORE the assignment
                            self.flush_hoisted_statements();
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        IrBlockStatement::Expr(expr) => {
                            let expr_str = self.gen_expr_from_foreign(expr, source_module);
                            if !expr_str.is_empty() && expr_str != "/* nil */" && expr_str != "/* void */" {
                                self.write_line(&format!("{};", expr_str));
                            }
                        }
                    }
                }
                // Assign result to var_name
                // Special case for empty arrays - use the target variable's type
                let result_str = if let IrExpr::Array { elements, ty } = result.as_ref() {
                    if elements.is_empty() {
                        // Try to get element type from array's type annotation
                        if let ResolvedType::Array(inner) = ty {
                            let elem_ty = self.type_to_wgsl_from(inner, source_module);
                            format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                        } else {
                            // Look up the target variable's type
                            if let Some(var_ty) = self.local_binding_types.get(var_name) {
                                if let ResolvedType::Array(inner) = var_ty {
                                    let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                    format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                                } else {
                                    self.gen_expr_from_foreign(result, source_module)
                                }
                            } else {
                                self.gen_expr_from_foreign(result, source_module)
                            }
                        }
                    } else {
                        self.gen_expr_from_foreign(result, source_module)
                    }
                } else {
                    self.gen_expr_from_foreign(result, source_module)
                };
                self.write_line(&format!("{} = {};", var_name, result_str));
            }
            // Nested if - generate inline
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.gen_let_if_from_foreign(
                    var_name,
                    condition,
                    then_branch,
                    else_branch,
                    source_module,
                );
            }
            // Simple expression
            _ => {
                // Special case for empty arrays
                let expr_str = if let IrExpr::Array { elements, ty } = branch {
                    if elements.is_empty() {
                        if let ResolvedType::Array(inner) = ty {
                            let elem_ty = self.type_to_wgsl_from(inner, source_module);
                            format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                        } else if let Some(var_ty) = self.local_binding_types.get(var_name) {
                            if let ResolvedType::Array(inner) = var_ty {
                                let elem_ty = self.type_to_wgsl_from(inner, source_module);
                                format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                            } else {
                                self.gen_expr_from_foreign(branch, source_module)
                            }
                        } else {
                            self.gen_expr_from_foreign(branch, source_module)
                        }
                    } else {
                        self.gen_expr_from_foreign(branch, source_module)
                    }
                } else {
                    self.gen_expr_from_foreign(branch, source_module)
                };
                self.write_line(&format!("{} = {};", var_name, expr_str));
            }
        }
    }

    /// Generate let statement with match value from a foreign module.
    /// Generates a switch statement that assigns to the var_name.
    fn gen_let_match_from_foreign(
        &mut self,
        var_name: &str,
        scrutinee: &IrExpr,
        arms: &[crate::ir::IrMatchArm],
        source_module: &IrModule,
    ) {
        let scrutinee_str = self.gen_expr_from_foreign(scrutinee, source_module);

        // Check if this is an enum with data variants
        let enum_has_data = if let ResolvedType::Enum(id) = scrutinee.ty() {
            let e = source_module.get_enum(*id);
            e.variants.iter().any(|v| !v.fields.is_empty())
        } else {
            false
        };

        // Generate switch statement
        if enum_has_data {
            self.write_line(&format!("switch {}.discriminant {{", scrutinee_str));
        } else {
            self.write_line(&format!("switch {} {{", scrutinee_str));
        }
        self.indent += 1;

        // Generate case for each arm
        for (idx, arm) in arms.iter().enumerate() {
            let tag = idx as u32;
            self.write_line(&format!("case {}u: {{ // {}", tag, arm.variant));
            self.indent += 1;

            // Extract bindings from data array if enum has data
            if enum_has_data && !arm.bindings.is_empty() {
                for (i, (name, _ty)) in arm.bindings.iter().enumerate() {
                    self.write_line(&format!("let {} = {}.data[{}];", name, scrutinee_str, i));
                }
            }

            // Set up local binding types for method call resolution
            for (name, ty) in &arm.bindings {
                self.local_binding_types.insert(name.clone(), ty.clone());
            }

            // Generate the arm body - assign result to var_name
            self.gen_let_match_arm_from_foreign(var_name, &arm.body, source_module);

            // Clear local binding types after processing arm
            self.local_binding_types.clear();

            self.indent -= 1;
            self.write_line("}");
        }

        // Add default case
        self.write_line("default: {}");

        self.indent -= 1;
        self.write_line("}");
    }

    /// Generate match arm body for a let statement, assigning to var_name.
    fn gen_let_match_arm_from_foreign(
        &mut self,
        var_name: &str,
        body: &IrExpr,
        source_module: &IrModule,
    ) {
        use crate::ir::IrBlockStatement;

        match body {
            IrExpr::Block {
                statements, result, ..
            } => {
                // Generate statements first
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Let { name, value, .. } => {
                            // Skip nil-valued let bindings (e.g., unsupported closures)
                            if Self::expr_is_nil(value) {
                                continue;
                            }
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                self.register_closure_from_foreign(name, params, body, source_module);
                                continue;
                            }
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            // Skip if the generated value is a nil placeholder
                            if value_str == "/* nil */" || value_str == "/* void */" {
                                continue;
                            }
                            self.write_line(&format!("let {} = {};", name, value_str));
                        }
                        IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr_from_foreign(target, source_module);
                            let value_str = self.gen_expr_from_foreign(value, source_module);
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        IrBlockStatement::Expr(expr) => {
                            let expr_str = self.gen_expr_from_foreign(expr, source_module);
                            if !expr_str.is_empty() && expr_str != "/* nil */" && expr_str != "/* void */" {
                                self.write_line(&format!("{};", expr_str));
                            }
                        }
                    }
                }
                // Assign result to var_name
                let result_str = self.gen_expr_from_foreign(result, source_module);
                self.write_line(&format!("{} = {};", var_name, result_str));
            }
            // Simple expression - just assign
            _ => {
                let expr_str = self.gen_expr_from_foreign(body, source_module);
                self.write_line(&format!("{} = {};", var_name, expr_str));
            }
        }
    }

    /// Get the type name for method call mangling from a ResolvedType.
    ///
    /// Used to determine the prefix for mangled method names (e.g., Struct_method).
    /// Returns None if the type doesn't have a meaningful name for mangling.
    /// Resolve the type of a field access chain.
    ///
    /// Given a base type and a chain of field names, traverse the struct fields to find
    /// the final type. For example, for `stop.color` where stop is ColorStop, this would
    /// return the type of the `color` field.
    fn resolve_field_chain_type(
        &self,
        base_type: &ResolvedType,
        field_chain: &[String],
        source_module: &IrModule,
    ) -> Option<ResolvedType> {
        if field_chain.is_empty() {
            return Some(base_type.clone());
        }

        let field_name = &field_chain[0];
        let remaining = &field_chain[1..];

        // Look up the field type from the struct
        let field_type = match base_type {
            ResolvedType::Struct(id) => {
                let s = source_module.get_struct(*id);
                s.fields
                    .iter()
                    .find(|f| &f.name == field_name)
                    .map(|f| f.ty.clone())
            }
            ResolvedType::External { name, .. } => {
                // Look up external struct in imported modules
                let simple = simple_type_name(name);
                self.imported_modules
                    .values()
                    .flat_map(|m| m.structs.iter())
                    .find(|s| s.name == simple)
                    .and_then(|s| {
                        s.fields
                            .iter()
                            .find(|f| &f.name == field_name)
                            .map(|f| f.ty.clone())
                    })
            }
            _ => None,
        };

        // Continue resolving if there are more fields in the chain
        if let Some(ft) = field_type {
            if remaining.is_empty() {
                Some(ft)
            } else {
                self.resolve_field_chain_type(&ft, remaining, source_module)
            }
        } else {
            None
        }
    }

    /// Find the type that defines a method with the given name by searching all imported modules.
    /// This is a fallback when the receiver type can't be resolved from bindings.
    /// Returns the WGSL-safe type name (with module prefixes converted from :: to _).
    fn find_method_owner_type(&self, method_name: &str, source_module: &IrModule) -> Option<String> {
        use crate::ir::ImplTarget;

        // Search in source module first
        for impl_block in &source_module.impls {
            for func in &impl_block.functions {
                if func.name == method_name {
                    // Found the method - get the type name from the impl target
                    match &impl_block.target {
                        ImplTarget::Struct(id) => {
                            return Some(to_wgsl_identifier(&source_module.get_struct(*id).name));
                        }
                        ImplTarget::Enum(id) => {
                            return Some(to_wgsl_identifier(&source_module.get_enum(*id).name));
                        }
                    }
                }
            }
        }

        // Search in imported modules
        for module in self.imported_modules.values() {
            for impl_block in &module.impls {
                for func in &impl_block.functions {
                    if func.name == method_name {
                        match &impl_block.target {
                            ImplTarget::Struct(id) => {
                                return Some(to_wgsl_identifier(&module.get_struct(*id).name));
                            }
                            ImplTarget::Enum(id) => {
                                return Some(to_wgsl_identifier(&module.get_enum(*id).name));
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn get_method_type_name(ty: &ResolvedType, source_module: &IrModule) -> Option<String> {
        match ty {
            ResolvedType::Struct(id) => Some(to_wgsl_identifier(&source_module.get_struct(*id).name)),
            ResolvedType::Trait(id) => Some(to_wgsl_identifier(&source_module.get_trait(*id).name)),
            ResolvedType::Enum(id) => Some(to_wgsl_identifier(&source_module.get_enum(*id).name)),
            ResolvedType::External { name, .. } => {
                // Preserve module prefix for types like "distribution::Vertical" -> "distribution_Vertical"
                // This avoids collisions with types that have the same simple name in different modules
                Some(to_wgsl_identifier(name))
            }
            ResolvedType::Optional(inner) => {
                // For optional types, unwrap and get the inner type name
                Self::get_method_type_name(inner, source_module)
            }
            ResolvedType::TypeParam(name) => {
                // Type parameters might be trait or enum types - preserve module prefix
                Some(to_wgsl_identifier(name))
            }
            _ => None,
        }
    }

    /// Get the type of a struct field by name.
    ///
    /// Used for resolving loop variable types when the IR stores placeholder types.
    fn get_struct_field_type_by_name(
        &self,
        struct_name: &str,
        field_name: &str,
        source_module: &IrModule,
    ) -> Option<ResolvedType> {
        // First, check in source module
        for struct_def in &source_module.structs {
            if struct_def.name == struct_name {
                for field in &struct_def.fields {
                    if field.name == field_name {
                        return Some(field.ty.clone());
                    }
                }
            }
        }
        // Then check imported modules
        for (_path, module) in self.imported_modules.iter() {
            for struct_def in &module.structs {
                if struct_def.name == struct_name {
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            return Some(field.ty.clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Resolve the element type of a renamed array access in unrolled loops.
    ///
    /// When a loop variable like `op` is renamed to `self_.transforms[0u]` during
    /// loop unrolling, this function extracts the array element type by:
    /// 1. Checking if the receiver type is a TypeParam matching a renamed variable
    /// 2. Parsing the renamed string to extract the base expression (before `[`)
    /// 3. Looking up the field type if it's a self field access
    /// 4. Returning the array element type
    fn resolve_renamed_array_element_type(
        &self,
        receiver_ty: &ResolvedType,
        renames: &std::collections::HashMap<String, String>,
        source_module: &IrModule,
    ) -> Option<ResolvedType> {
        const SELF_PREFIX: &str = "self_.";

        let param_name = match receiver_ty {
            ResolvedType::TypeParam(name) => name,
            _ => return None,
        };

        let renamed = renames.get(param_name)?;

        // Extract base expression before array index (e.g., "self_.transforms" from "self_.transforms[0u]")
        let bracket_pos = renamed.find('[')?;
        let base_expr = &renamed[..bracket_pos];

        // Currently only handle self field access pattern
        if !base_expr.starts_with(SELF_PREFIX) {
            return None;
        }

        let field_name = &base_expr[SELF_PREFIX.len()..];
        let self_ty_name = self.current_impl_type.as_ref()?;
        let field_ty = self.get_struct_field_type_by_name(self_ty_name, field_name, source_module)?;

        // Extract element type from array
        match field_ty {
            ResolvedType::Array(elem_ty) => Some((*elem_ty).clone()),
            _ => None,
        }
    }

    /// Find the return type of a method by searching impl blocks in the source module
    /// and all imported modules.
    ///
    /// This is used when the IR stores placeholder types like `TypeParam("sampleResult")`
    /// for trait method calls where the return type couldn't be resolved at IR lowering time.
    fn find_method_return_type(
        &self,
        method_name: &str,
        source_module: &IrModule,
    ) -> Option<ResolvedType> {
        // Search source module's impl blocks first
        for impl_block in &source_module.impls {
            for func in &impl_block.functions {
                if func.name == method_name {
                    // Found the method - return its return type
                    if let Some(ret_ty) = &func.return_type {
                        return Some(ret_ty.clone());
                    }
                    // If no explicit return type, infer from body
                    return Some(func.body.ty().clone());
                }
            }
        }

        // Search all imported modules
        for (_path, imported_module) in self.imported_modules.iter() {
            for impl_block in &imported_module.impls {
                for func in &impl_block.functions {
                    if func.name == method_name {
                        if let Some(ret_ty) = &func.return_type {
                            return Some(ret_ty.clone());
                        }
                        return Some(func.body.ty().clone());
                    }
                }
            }
        }

        None
    }

    /// Try to infer type from a method call expression by looking up the method in impl blocks.
    ///
    /// This is a fallback when `infer_concrete_type_from_expr` fails because the IR
    /// stores placeholder types like `TypeParam("sampleResult")`.
    fn try_infer_type_from_method_call(
        &self,
        expr: &IrExpr,
        source_module: &IrModule,
    ) -> Option<ResolvedType> {
        match expr {
            IrExpr::MethodCall { method, .. } => {
                // Look up the method's return type from impl blocks
                self.find_method_return_type(method, source_module)
            }
            IrExpr::FunctionCall { path, .. } => {
                // For function calls, try the last component of the path as function name
                if let Some(fn_name) = path.last() {
                    // Check if there's a function with this name that returns a known type
                    for func in &source_module.functions {
                        if &func.name == fn_name {
                            if let Some(ret_ty) = &func.return_type {
                                return Some(ret_ty.clone());
                            }
                        }
                    }
                    // Also check impl blocks for static method patterns like Color4_black
                    self.find_method_return_type(fn_name, source_module)
                } else {
                    None
                }
            }
            IrExpr::Block { result, .. } => {
                self.try_infer_type_from_method_call(result, source_module)
            }
            IrExpr::If {
                then_branch,
                else_branch,
                ..
            } => {
                // Try then branch first
                if let Some(ty) = self.try_infer_type_from_method_call(then_branch, source_module) {
                    return Some(ty);
                }
                // Try else branch
                if let Some(else_expr) = else_branch {
                    return self.try_infer_type_from_method_call(else_expr, source_module);
                }
                None
            }
            _ => None,
        }
    }

    /// Generate expression WGSL from a foreign module.
    ///
    /// Uses `source_module` for ID-to-name lookups instead of `self.module`.
    ///
    /// TODO(P4): Consider unifying with `gen_expr` by adding an optional module parameter.
    /// This would reduce code duplication and ensure consistent handling. The main
    /// difference is which module is used for ID-to-name lookups (struct_id -> name, etc).
    fn gen_expr_from_foreign(&self, expr: &IrExpr, source_module: &IrModule) -> String {
        match expr {
            IrExpr::Literal { value, ty } => self.gen_literal(value, ty),

            IrExpr::Reference { path, ty: _ } => {
                // Handle bare "self" reference - convert to "self_" for WGSL
                if path.len() == 1 && path[0] == "self" {
                    "self_".to_string()
                } else if path.len() == 1 {
                    // Single-element path - escape reserved keywords
                    Self::escape_wgsl_keyword(&path[0])
                } else {
                    // Escape reserved keywords in reference paths
                    // Also handle paths starting with "self" -> "self_"
                    let escaped_path: Vec<String> = path
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            if i == 0 && p == "self" {
                                "self_".to_string()
                            } else {
                                Self::escape_wgsl_keyword(p)
                            }
                        })
                        .collect();
                    escaped_path.join(".")
                }
            }

            IrExpr::SelfFieldRef { field, .. } => {
                format!("self_.{}", Self::escape_wgsl_keyword(field))
            }

            IrExpr::FieldAccess { object, field, .. } => {
                let object_str = self.gen_expr_from_foreign(object, source_module);
                format!("{}.{}", object_str, Self::escape_wgsl_keyword(field))
            }

            IrExpr::LetRef { name, .. } => name.clone(),

            IrExpr::BinaryOp {
                left, op, right, ..
            } => {
                // Handle nil comparisons specially (x == nil, x != nil)
                if matches!(op, BinaryOperator::Eq | BinaryOperator::Ne) {
                    if let Some(nil_cmp) = self.gen_nil_comparison(left, op, right, source_module) {
                        return nil_cmp;
                    }
                }
                let left_str = self.gen_expr_from_foreign(left, source_module);
                let right_str = self.gen_expr_from_foreign(right, source_module);
                let op_str = self.binary_op_to_wgsl(op);
                format!("({} {} {})", left_str, op_str, right_str)
            }

            IrExpr::UnaryOp { op, operand, .. } => {
                let operand_str = self.gen_expr_from_foreign(operand, source_module);
                let op_str = self.unary_op_to_wgsl(op);
                format!("({}{})", op_str, operand_str)
            }

            IrExpr::StructInst {
                struct_id: _,
                fields,
                ty,
                ..
            } => {
                // Get name from the type, which has the correct struct name
                let name = self.type_to_wgsl_from(ty, source_module);

                // Check if this struct implements Fill trait (by having a `sample` method)
                // This enables direct struct instantiation like `fill::relative::Linear(...)`
                // to automatically wrap in FillData for trait dispatch
                // Compare WGSL-mangled names since `name` is already mangled
                let is_fill_implementor = self.imported_modules.values().any(|m| {
                    m.structs.iter().enumerate().any(|(struct_idx, s)| {
                        to_wgsl_identifier(&s.name) == name && {
                            let struct_id = crate::ir::StructId(struct_idx as u32);
                            m.impls.iter().any(|imp| {
                                imp.struct_id() == Some(struct_id)
                                    && imp.functions.iter().any(|f| f.name == "sample")
                            })
                        }
                    })
                }) || source_module.structs.iter().enumerate().any(|(struct_idx, s)| {
                    to_wgsl_identifier(&s.name) == name && {
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        source_module.impls.iter().any(|imp| {
                            imp.struct_id() == Some(struct_id)
                                && imp.functions.iter().any(|f| f.name == "sample")
                        })
                    }
                });

                if is_fill_implementor {
                    // Generate FillData wrapping for trait dispatch
                    let safe_struct_name = to_wgsl_identifier(&name);
                    let type_tag = format!("FILL_TAG_{}", safe_struct_name.to_uppercase());

                    // Flatten fields to f32s for FillData array
                    let mut data_values: Vec<String> = Vec::new();
                    for (_, field_expr) in fields {
                        data_values.extend(self.flatten_expr_to_f32s(field_expr));
                    }
                    while data_values.len() < DEFAULT_MAX_DISPATCH_DATA_SIZE {
                        data_values.push("0.0".to_string());
                    }

                    return format!(
                        "FillData({}, 0u, array<f32, {}>({}))",
                        type_tag,
                        DEFAULT_MAX_DISPATCH_DATA_SIZE,
                        data_values.join(", ")
                    );
                }

                // Find the struct definition in source module by name
                let struct_def = source_module
                    .structs
                    .iter()
                    .find(|s| s.name == name || to_wgsl_identifier(&s.name) == name);

                // WGSL struct constructors use positional arguments.
                // We need to reorder fields to match struct definition order.
                let arg_strs: Vec<String> = if let Some(s) = struct_def {
                    // Get struct field order from definition
                    let field_map: std::collections::HashMap<&str, &IrExpr> = fields
                        .iter()
                        .map(|(name, expr)| (name.as_str(), expr))
                        .collect();

                    // Emit values in struct field order
                    s.fields
                        .iter()
                        .map(|field| {
                            if let Some(expr) = field_map.get(field.name.as_str()) {
                                let value = self.gen_expr_from_foreign(expr, source_module);
                                // If field type is Optional, wrap value in Optional wrapper
                                if let ResolvedType::Optional(inner) = &field.ty {
                                    let inner_type = self.type_to_wgsl_from(inner, source_module);
                                    format!("Optional_{}(true, {})", inner_type, value)
                                } else {
                                    value
                                }
                            } else {
                                // Generate default value for missing fields
                                self.gen_default_value_for_type(&field.ty, source_module)
                            }
                        })
                        .collect()
                } else {
                    // For builtin types without struct definition, use order as-is
                    fields
                        .iter()
                        .map(|(_, e)| self.gen_expr_from_foreign(e, source_module))
                        .collect()
                };
                format!("{}({})", name, arg_strs.join(", "))
            }

            IrExpr::FunctionCall { path, args, .. } => {
                // Check if this is a closure call
                if let Some(closure_fn_name) = self.get_closure_fn_name(path) {
                    let arg_strs: Vec<String> = args
                        .iter()
                        .map(|(_, expr)| self.gen_expr_from_foreign(expr, source_module))
                        .collect();
                    return format!("{}({})", closure_fn_name, arg_strs.join(", "));
                }

                // Special handling for len() on arrays
                // In WGSL, fixed-size arrays don't have a runtime length, so we extract
                // the size from the type or use a default
                if path.len() == 1 && path[0] == "len" && !args.is_empty() {
                    let arg_ty = args[0].1.ty();
                    // For arrays, return the fixed size (8u for color stop arrays)
                    // TODO: Extract actual array size from struct field definition
                    match arg_ty {
                        ResolvedType::Array(_) => return "8u".to_string(),
                        ResolvedType::TypeParam(_) => return "8u".to_string(),
                        _ => {}
                    }
                }

                // For multi-segment paths like Color4::transparent, mangle to Color4_transparent
                let fn_name = if path.len() > 1 {
                    // Static method call: Type::method -> Type_method
                    path.join("_")
                } else {
                    let path_str = path.join("::");
                    self.map_builtin_function(&path_str).to_string()
                };
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, expr)| {
                        let arg_str = self.gen_expr_from_foreign(expr, source_module);
                        // If the argument has an Optional type, unwrap it with .value
                        // This handles cases like min(count, self.optionalField) where
                        // optionalField has been nil-checked in an outer if condition
                        if let ResolvedType::Optional(_) = expr.ty() {
                            format!("{}.value", arg_str)
                        } else {
                            arg_str
                        }
                    })
                    .collect();
                format!("{}({})", fn_name, arg_strs.join(", "))
            }

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let mut recv_str = self.gen_expr_from_foreign(receiver, source_module);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(_, expr)| self.gen_expr_from_foreign(expr, source_module))
                    .collect();

                // Method calls need mangled names: StructName_method
                // Check if receiver is "self" - use current_impl_type for mangling
                let is_self_receiver = matches!(
                    receiver.as_ref(),
                    IrExpr::Reference { path, .. } if path.len() == 1 && path[0] == "self"
                );

                let mangled_name = if is_self_receiver {
                    // Use current impl type for self method calls
                    if let Some(ref impl_type) = self.current_impl_type {
                        format!("{}_{}", impl_type, method)
                    } else {
                        method.clone()
                    }
                } else {
                    // Check if receiver is a local binding variable from a match pattern
                    // or a function parameter
                    let binding_type = if let IrExpr::Reference { path, .. } = receiver.as_ref() {
                        if path.len() == 1 {
                            // Check local bindings first, then function params
                            self.local_binding_types
                                .get(&path[0])
                                .or_else(|| self.current_function_params.get(&path[0]))
                                .cloned()
                        } else if path.len() >= 2 {
                            // Multi-part path like ["stop", "color"] - this is a field access chain
                            // Try to look up the base binding's type and traverse fields
                            let base_type = self
                                .local_binding_types
                                .get(&path[0])
                                .or_else(|| self.current_function_params.get(&path[0]));
                            if let Some(base_ty) = base_type {
                                // Try to resolve the field chain type
                                self.resolve_field_chain_type(base_ty, &path[1..], source_module)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Check if receiver is a SelfFieldRef - look up field type from current impl
                    let field_type = if let IrExpr::SelfFieldRef { field, .. } = receiver.as_ref() {
                        // Try to find field type in current impl struct
                        self.current_impl_type.as_ref().and_then(|impl_name| {
                            // Look up the struct in source module
                            source_module
                                .structs
                                .iter()
                                .find(|s| to_wgsl_identifier(&s.name) == *impl_name)
                                .and_then(|s| s.fields.iter().find(|f| f.name == *field))
                                .map(|f| f.ty.clone())
                        })
                    } else {
                        None
                    };

                    // Use binding type, field type, or receiver's type
                    let receiver_ty = binding_type
                        .as_ref()
                        .or(field_type.as_ref())
                        .unwrap_or_else(|| receiver.ty());

                    // If receiver type is Optional, unwrap it for the method call
                    // and use the inner type for method name mangling
                    let actual_receiver_ty = if let ResolvedType::Optional(inner) = receiver_ty {
                        // Append .value to unwrap the optional
                        recv_str = format!("{}.value", recv_str);
                        inner.as_ref()
                    } else {
                        receiver_ty
                    };

                    // Try to get type name from the resolved receiver type
                    let type_name_from_ty = Self::get_method_type_name(actual_receiver_ty, source_module);

                    // Check if the resolved type name is invalid:
                    // - Contains dots from TypeParam path
                    // - Is a placeholder like "resolveResult" (auto-generated when return type couldn't be resolved)
                    // - Is "DictValue" (placeholder for dictionary value types)
                    // - Is a lowercase variable name (e.g., "op" from for-loop variable)
                    let is_placeholder_or_invalid = |name: &str| -> bool {
                        // Placeholder types:
                        // - "DictValue" from dictionary access with unresolved value type
                        // - End with "Result" and start with lowercase (method name + "Result")
                        // - All lowercase short names (likely variable names like "op", "i", "x")
                        // - Names starting with lowercase that don't look like module paths
                        name == "DictValue"
                            || (name.ends_with("Result")
                                && name.chars().next().map_or(false, |c| c.is_lowercase()))
                            || (name.chars().all(|c| c.is_lowercase() || c == '_')
                                && !name.contains("::"))
                    };
                    let is_valid_type_name = type_name_from_ty.as_ref()
                        .map(|name| !name.contains('.') && !is_placeholder_or_invalid(name))
                        .unwrap_or(false);

                    if is_valid_type_name {
                        format!("{}_{}", type_name_from_ty.unwrap(), method)
                    } else {
                        // Fallback: search for which type defines this method
                        self.find_method_owner_type(method, source_module)
                            .map(|type_name| format!("{}_{}", type_name, method))
                            .unwrap_or_else(|| method.clone())
                    }
                };

                let all_args = std::iter::once(recv_str.clone())
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({})", mangled_name, all_args)
            }

            IrExpr::EnumInst {
                enum_id,
                variant,
                fields,
                ty,
            } => {
                // Handle InferredEnum (TypeParam("InferredEnum")) specially
                if let ResolvedType::TypeParam(param_name) = ty {
                    if param_name == "InferredEnum" {
                        // Check if this is a Color enum variant
                        let color_variants = ["rgba", "rgb", "hsla", "hex"];
                        if color_variants.contains(&variant.as_str()) {
                            // Generate Color enum instantiation
                            let field_values: Vec<String> = fields
                                .iter()
                                .map(|(_, e)| self.gen_expr_from_foreign(e, source_module))
                                .collect();
                            let mut data_values = field_values;
                            while data_values.len() < 4 {
                                data_values.push("0.0".to_string());
                            }
                            return format!(
                                "Color(Color_{}, array<f32, 4>({}))",
                                variant,
                                data_values.join(", ")
                            );
                        }

                        // Check if this is a Fill trait implementor variant
                        let fill_variants = ["solid", "linear", "radial", "angular", "pattern", "multilinear"];
                        if fill_variants.contains(&variant.as_str()) {
                            // Generate FillData instantiation
                            // Map variant name to struct name (capitalize first letter)
                            let struct_name = {
                                let mut chars = variant.chars();
                                match chars.next() {
                                    None => String::new(),
                                    Some(first) => {
                                        first.to_uppercase().chain(chars).collect::<String>()
                                    }
                                }
                            };

                            // Get type tag - struct names are like "fill::Solid" -> "FILL_TAG_FILL_SOLID"
                            let type_tag =
                                format!("FILL_TAG_FILL_{}", struct_name.to_uppercase());

                            // Generate field values flattened to f32s for FillData
                            let mut data_values: Vec<String> = Vec::new();
                            for (_, e) in fields {
                                data_values.extend(self.flatten_expr_to_f32s(e));
                            }
                            while data_values.len() < DEFAULT_MAX_DISPATCH_DATA_SIZE {
                                data_values.push("0.0".to_string());
                            }
                            return format!(
                                "FillData({}, 0u, array<f32, {}>({}))",
                                type_tag,
                                DEFAULT_MAX_DISPATCH_DATA_SIZE,
                                data_values.join(", ")
                            );
                        }
                    }
                }

                // Get the enum and its definition - check enum_id first, then ty
                let (enum_name, max_data_size) = if let Some(id) = enum_id {
                    let e = source_module.get_enum(*id);
                    let max_size = e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0);
                    (e.name.clone(), max_size)
                } else if let ResolvedType::Enum(id) = ty {
                    let e = source_module.get_enum(*id);
                    let max_size = e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0);
                    (e.name.clone(), max_size)
                } else if let ResolvedType::External { name, .. } = ty {
                    // External enum - use name and calculate from fields
                    let simple_name = simple_type_name(name);
                    let max_size = fields.len().max(4); // Use at least 4 for common enums like Color
                    (simple_name.to_string(), max_size)
                } else if let ResolvedType::TypeParam(name) = ty {
                    // TypeParam might be an enum type - look it up in imported modules
                    // Use max_by_key to prefer enum with fields (re-exported enums may be empty)
                    let simple_name = simple_type_name(name);
                    let max_size = self
                        .imported_modules
                        .values()
                        .flat_map(|m| m.enums.iter())
                        .filter(|e| e.name == simple_name)
                        .max_by_key(|e| e.variants.iter().map(|v| v.fields.len()).sum::<usize>())
                        .map(|e| e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0))
                        .unwrap_or(fields.len().max(4));
                    (simple_name.to_string(), max_size)
                } else {
                    ("Unknown".to_string(), fields.len().max(4))
                };

                if fields.is_empty() {
                    // Simple unit variant - reference the constant
                    format!("{}_{}", enum_name, variant)
                } else if max_data_size == 0 {
                    // Enum has data but max_size is 0 (shouldn't happen, but handle gracefully)
                    format!("{}_{}", enum_name, variant)
                } else {
                    // Generate wrapper struct with discriminant and data
                    let field_values: Vec<String> = fields
                        .iter()
                        .map(|(_, e)| self.gen_expr_from_foreign(e, source_module))
                        .collect();
                    // Pad with zeros to fill the data array
                    let mut data_values = field_values;
                    while data_values.len() < max_data_size {
                        data_values.push("0.0".to_string());
                    }
                    format!(
                        "{}({}_{}, array<f32, {}>({}))",
                        enum_name,
                        enum_name,
                        variant,
                        max_data_size,
                        data_values.join(", ")
                    )
                }
            }

            IrExpr::Tuple { fields, .. } => {
                // Tuples are struct-like in WGSL
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(n, e)| {
                        format!("{}: {}", n, self.gen_expr_from_foreign(e, source_module))
                    })
                    .collect();
                format!("({})", field_strs.join(", "))
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => {
                // Check if branches are nil/void - if so, hoist as if-statement
                let is_nil_result = Self::expr_is_nil(then_branch);

                // Check if branches have statements - if so, we can't use select()
                // because select() evaluates both branches but the let bindings
                // inside branches won't be properly scoped
                let has_statements = Self::branch_has_statements(then_branch)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::branch_has_statements(e));

                // Check if type is compatible with select()
                let can_select = self.can_use_select(ty, source_module);

                if is_nil_result {
                    // Hoist as if-statement for void/side-effect expressions
                    let mut hoisted = Vec::new();

                    // Generate condition first (may produce hoisted statements)
                    let cond = self.gen_expr_from_foreign(condition, source_module);
                    let cond_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();
                    hoisted.extend(cond_hoisted);

                    let then_str = self.gen_expr_from_foreign(then_branch, source_module);
                    let then_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();

                    if let Some(else_branch) = else_branch {
                        let else_str = self.gen_expr_from_foreign(else_branch, source_module);
                        let else_hoisted: Vec<String> =
                            self.hoisted_statements.borrow_mut().drain(..).collect();

                        if then_str != "/* nil */" || else_str != "/* nil */" {
                            let then_block = if then_hoisted.is_empty() {
                                if then_str == "/* nil */" {
                                    String::new()
                                } else {
                                    format!("{};", then_str)
                                }
                            } else {
                                format!(
                                    "{} {};",
                                    then_hoisted.iter().map(|s| format!("{};", s)).collect::<Vec<_>>().join(" "),
                                    if then_str == "/* nil */" { "" } else { &then_str }
                                )
                            };
                            let else_block = if else_hoisted.is_empty() {
                                if else_str == "/* nil */" {
                                    String::new()
                                } else {
                                    format!("{};", else_str)
                                }
                            } else {
                                format!(
                                    "{} {};",
                                    else_hoisted.iter().map(|s| format!("{};", s)).collect::<Vec<_>>().join(" "),
                                    if else_str == "/* nil */" { "" } else { &else_str }
                                )
                            };
                            hoisted.push(format!(
                                "if ({}) {{ {} }} else {{ {} }}",
                                cond, then_block, else_block
                            ));
                        }
                    } else if then_str != "/* nil */" {
                        let then_block = if then_hoisted.is_empty() {
                            format!("{};", then_str)
                        } else {
                            format!(
                                "{} {};",
                                then_hoisted.iter().map(|s| format!("{};", s)).collect::<Vec<_>>().join(" "),
                                then_str
                            )
                        };
                        hoisted.push(format!("if ({}) {{ {} }}", cond, then_block));
                    }
                    self.push_hoisted_statements(hoisted);
                    "/* void */".to_string()
                } else if has_statements || !can_select {
                    // Branches have statements or type doesn't support select()
                    // Hoist as if-statement with result variable
                    let result_var = self.gen_unique_name("if_result");
                    // Use the expression type, but if it looks suspicious, try to infer from branches
                    let type_str = {
                        let base_type_str = self.type_to_wgsl_from(ty, source_module);
                        // Check if the type looks invalid/defaulted (f32 as fallback, or other suspicious patterns)
                        // If the base type is f32 but the then branch returns a struct, use the struct type
                        // Also check for Never type (which maps to u32) - this happens when the IR couldn't
                        // infer the type of if-expressions inside match arms
                        let is_never_type = matches!(ty, ResolvedType::Primitive(PrimitiveType::Never));
                        let is_suspicious_type = base_type_str == "f32"
                            || base_type_str.contains("UnknownElement")
                            || base_type_str.contains('.')
                            || is_never_type;
                        if is_suspicious_type {
                            // Try to infer from the then branch result expression
                            let inferred = Self::infer_concrete_type_from_expr(then_branch);
                            // If inference failed but this is a method call, try looking up
                            // the method's return type from the source module's impl blocks
                            let inferred = if inferred.is_none() {
                                self.try_infer_type_from_method_call(then_branch, source_module)
                            } else {
                                inferred
                            };
                            if let Some(concrete_type) = inferred {
                                let concrete_str = self.type_to_wgsl_from(&concrete_type, source_module);
                                if concrete_str != "f32" && !concrete_str.contains("UnknownElement") {
                                    concrete_str
                                } else {
                                    base_type_str
                                }
                            } else {
                                base_type_str
                            }
                        } else {
                            base_type_str
                        }
                    };

                    let mut hoisted = Vec::new();

                    // Generate condition first (may produce hoisted statements)
                    let cond = self.gen_expr_from_foreign(condition, source_module);
                    // Collect any hoisted statements from condition - these go BEFORE the var decl
                    let cond_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();
                    hoisted.extend(cond_hoisted);

                    // Add the result variable declaration
                    hoisted.push(format!("var {}: {}", result_var, type_str));

                    // Generate then branch
                    let then_val = {
                        let val = self.gen_expr_from_foreign(then_branch, source_module);
                        // Fix UnknownElement in generated values by using the result type
                        if val.contains("UnknownElement") {
                            val.replace("UnknownElement", &type_str.trim_start_matches("array<").split(",").next().unwrap_or("f32"))
                        } else {
                            val
                        }
                    };
                    // Collect any hoisted statements from then branch
                    let then_hoisted: Vec<String> =
                        self.hoisted_statements.borrow_mut().drain(..).collect();

                    // Generate else branch if present
                    let (else_val, else_hoisted) = if let Some(else_branch) = else_branch {
                        let val = self.gen_expr_from_foreign(else_branch, source_module);
                        // Fix UnknownElement in generated values
                        let val = if val.contains("UnknownElement") {
                            val.replace("UnknownElement", &type_str.trim_start_matches("array<").split(",").next().unwrap_or("f32"))
                        } else {
                            val
                        };
                        let hoisted: Vec<String> =
                            self.hoisted_statements.borrow_mut().drain(..).collect();
                        (Some(val), hoisted)
                    } else {
                        (None, Vec::new())
                    };

                    // Build the if statement with hoisted inner statements
                    let then_stmts = if then_hoisted.is_empty() {
                        format!("{} = {};", result_var, then_val)
                    } else {
                        format!(
                            "{} {} = {};",
                            then_hoisted.iter().map(|s| format!("{};", s)).collect::<Vec<_>>().join(" "),
                            result_var,
                            then_val
                        )
                    };

                    if let Some(else_val) = else_val {
                        let else_stmts = if else_hoisted.is_empty() {
                            format!("{} = {};", result_var, else_val)
                        } else {
                            format!(
                                "{} {} = {};",
                                else_hoisted.iter().map(|s| format!("{};", s)).collect::<Vec<_>>().join(" "),
                                result_var,
                                else_val
                            )
                        };
                        hoisted.push(format!(
                            "if ({}) {{ {} }} else {{ {} }}",
                            cond, then_stmts, else_stmts
                        ));
                    } else {
                        hoisted.push(format!("if ({}) {{ {} }}", cond, then_stmts));
                    }

                    self.push_hoisted_statements(hoisted);
                    result_var
                } else {
                    let cond = self.gen_expr_from_foreign(condition, source_module);
                    let then_val = self.gen_expr_from_foreign(then_branch, source_module);
                    if let Some(else_branch) = else_branch {
                        let else_val = self.gen_expr_from_foreign(else_branch, source_module);
                        format!("select({}, {}, {})", else_val, then_val, cond)
                    } else {
                        format!("select({}, {}, {})", then_val, then_val, cond)
                    }
                }
            }

            IrExpr::Array { elements, ty } => {
                let elem_strs: Vec<String> = elements
                    .iter()
                    .map(|e| self.gen_expr_from_foreign(e, source_module))
                    .collect();
                // For empty arrays, we need to include type information since WGSL
                // can't infer the type of array()
                if elem_strs.is_empty() {
                    // Extract element type from the array type
                    if let ResolvedType::Array(inner) = ty {
                        let elem_ty = self.type_to_wgsl_from(inner, source_module);
                        // Empty arrays need a size - WGSL doesn't allow size 0
                        // Use default max size for arrays that will be populated later
                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                    } else {
                        // Fallback - shouldn't happen for well-typed arrays
                        format!("array<f32, {}>()", DEFAULT_MAX_ARRAY_SIZE)
                    }
                } else {
                    format!("array({})", elem_strs.join(", "))
                }
            }

            // Block expressions - hoist statements to accumulator
            IrExpr::Block {
                statements, result, ..
            } => {
                let (hoisted, result_expr) =
                    self.gen_block_with_hoisting_from_foreign(statements, result, source_module);

                if !hoisted.is_empty() {
                    self.push_hoisted_statements(hoisted);
                }

                result_expr
            }

            // For expression in expression position - hoist to statements
            IrExpr::For {
                var,
                collection,
                body,
                ty,
                ..
            } => {
                let (hoisted, result_var) =
                    self.gen_for_expr_hoisted_from_foreign(var, collection, body, ty, source_module);
                self.push_hoisted_statements(hoisted);
                result_var
            }

            // Match expression in expression position - hoist to statements
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
            } => {
                let (hoisted, result_var) =
                    self.gen_match_expr_hoisted_from_foreign(scrutinee, arms, ty, source_module);
                self.push_hoisted_statements(hoisted);
                result_var
            }

            // Array/Dictionary access - generate array indexing syntax
            IrExpr::DictAccess { dict, key, .. } => {
                let dict_str = self.gen_expr_from_foreign(dict, source_module);
                // For array indexing, integers must not have decimal points in WGSL
                let key_str = self.gen_array_index_expr(key, source_module);
                format!("{}[{}]", dict_str, key_str)
            }

            // Dictionary literal - not supported in WGSL
            IrExpr::DictLiteral { entries, .. } => {
                let entry_strs: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}: {}",
                            self.gen_expr_from_foreign(k, source_module),
                            self.gen_expr_from_foreign(v, source_module)
                        )
                    })
                    .collect();
                format!("/* dict literal: {{{}}} */", entry_strs.join(", "))
            }

            // EventMapping - metadata for runtime, not shader code
            IrExpr::EventMapping { variant, param, .. } => {
                let param_str = param.as_deref().unwrap_or("()");
                format!("/* event: {} -> .{} */", param_str, variant)
            }

            // Closure - when encountered directly (not via let binding), generate inline function
            IrExpr::Closure { params, body, .. } => {
                // Generate a closure function with a unique name
                let fn_name = self.gen_closure_fn_name("anon");
                let param_strs: Vec<String> = params
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.type_to_wgsl_from(ty, source_module)))
                    .collect();
                let return_ty = self.type_to_wgsl_from(body.ty(), source_module);
                let body_str = self.gen_expr_from_foreign(body, source_module);
                let fn_source = format!(
                    "fn {}({}) -> {} {{ return {}; }}",
                    fn_name,
                    param_strs.join(", "),
                    return_ty,
                    body_str
                );
                self.pending_closure_fns.borrow_mut().push(fn_source);
                // Return just the function name (caller will invoke it)
                fn_name
            }
        }
    }

    /// Convert a type to WGSL using a foreign module for ID lookups.
    fn type_to_wgsl_from(&self, ty: &ResolvedType, source_module: &IrModule) -> String {
        match ty {
            ResolvedType::Primitive(p) => self.primitive_to_wgsl(p),
            ResolvedType::Struct(id) => to_wgsl_identifier(&source_module.get_struct(*id).name),
            ResolvedType::Trait(id) => {
                format!(
                    "{}Data",
                    to_wgsl_identifier(&source_module.get_trait(*id).name)
                )
            }
            ResolvedType::Enum(id) => {
                let e = source_module.get_enum(*id);
                // Check if any variant has data - if so, use struct name
                let has_data = e.variants.iter().any(|v| !v.fields.is_empty());
                if has_data {
                    to_wgsl_identifier(&e.name)
                } else {
                    "u32".to_string()
                }
            }
            ResolvedType::Array(inner) => {
                format!(
                    "array<{}, 256>",
                    self.type_to_wgsl_from(inner, source_module)
                )
            }
            ResolvedType::Optional(inner) => {
                // WGSL optionals use wrapper structs: Optional_T { has_value: bool, value: T }
                let inner_name = self.type_to_wgsl_from(inner, source_module);
                format!("Optional_{}", inner_name)
            }
            ResolvedType::Generic { base, args } => {
                let base_name = to_wgsl_identifier(&source_module.get_struct(*base).name);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| self.type_to_wgsl_from(a, source_module))
                    .collect();
                format!("{}_{}", base_name, arg_strs.join("_"))
            }
            ResolvedType::TypeParam(name) => {
                let simple_name = simple_type_name(name);

                // First, check if this TypeParam matches a function parameter name
                // If so, use that parameter's actual type instead
                if let Some(param_ty) = self.current_function_params.get(simple_name) {
                    return self.type_to_wgsl_from(param_ty, source_module);
                }

                // Check if this TypeParam refers to a known trait from imported modules
                let is_trait = source_module
                    .traits
                    .iter()
                    .any(|t| simple_type_name(&t.name) == simple_name)
                    || self.imported_modules.values().any(|m| {
                        m.traits
                            .iter()
                            .any(|t| simple_type_name(&t.name) == simple_name)
                    });
                if is_trait {
                    format!("{}Data", to_wgsl_identifier(simple_name))
                } else {
                    // Check if this is a struct from the source module (need full qualified name)
                    if let Some(s) = source_module
                        .structs
                        .iter()
                        .find(|s| simple_type_name(&s.name) == simple_name)
                    {
                        to_wgsl_identifier(&s.name)
                    }
                    // Check if it's an enum from the source module
                    else if let Some(e) = source_module
                        .enums
                        .iter()
                        .find(|e| simple_type_name(&e.name) == simple_name)
                    {
                        // Enums with no data are u32, otherwise use the enum name
                        if e.variants.iter().any(|v| !v.fields.is_empty()) {
                            to_wgsl_identifier(&e.name)
                        } else {
                            "u32".to_string()
                        }
                    } else {
                        // Check if this is a placeholder type name from IR lowering
                        // These occur when type resolution fails:
                        // - "DictValue" - placeholder for dictionary value types
                        // - "UnknownElement" - placeholder for uninferable array element types
                        // - "<method>Result" - placeholder for method return types
                        let is_placeholder = simple_name == "DictValue"
                            || simple_name == "UnknownElement"
                            || (simple_name.ends_with("Result")
                                && simple_name
                                    .chars()
                                    .next()
                                    .map_or(false, |c| c.is_lowercase()));
                        // Check if this looks like a field access expression (e.g., "ideal.width")
                        // rather than a namespace path (e.g., "color::Color4")
                        let is_field_access = simple_name.contains('.')
                            && !simple_name.contains("::");
                        // Check if this looks like a variable/parameter name being used as a type
                        // Valid WGSL types start with uppercase (Color4, Size2D) or are builtin primitives
                        let is_wgsl_primitive = matches!(
                            simple_name,
                            "f32" | "f16" | "i32" | "u32" | "bool" | "vec2" | "vec3" | "vec4"
                                | "ivec2" | "ivec3" | "ivec4" | "uvec2" | "uvec3" | "uvec4"
                                | "mat2x2" | "mat2x3" | "mat2x4" | "mat3x2" | "mat3x3" | "mat3x4"
                                | "mat4x2" | "mat4x3" | "mat4x4"
                        );
                        let is_likely_variable = !is_wgsl_primitive
                            && simple_name
                                .chars()
                                .next()
                                .map_or(false, |c| c.is_lowercase())
                            && !simple_name.contains("::");
                        if is_placeholder || is_field_access || is_likely_variable {
                            // Fall back to f32 for unresolved placeholder types,
                            // field access patterns, and likely variable names
                            "f32".to_string()
                        } else {
                            to_wgsl_identifier(simple_name)
                        }
                    }
                }
            }
            ResolvedType::External { name, kind, .. } => self.external_type_to_wgsl(name, kind),

            ResolvedType::Closure { param_tys, return_ty } => {
                // WGSL doesn't have first-class functions
                // Closures are converted to named functions during codegen
                let params: Vec<String> = param_tys
                    .iter()
                    .map(|t| self.type_to_wgsl_from(t, source_module))
                    .collect();
                format!(
                    "/* closure({}) -> {} */",
                    params.join(", "),
                    self.type_to_wgsl_from(return_ty, source_module)
                )
            }

            _ => "f32".to_string(), // Fallback for unsupported types
        }
    }

    /// Generate WGSL constants for an enum type.
    ///
    /// WGSL doesn't have native enum support, so we represent enums as u32
    /// with named constants for each variant.
    /// The source_module parameter provides context for looking up nested EnumIds.
    fn gen_enum_constants(
        &mut self,
        e: &crate::ir::IrEnum,
        source_module: Option<&crate::ir::IrModule>,
    ) {
        // Skip enums with generic parameters (not supported in WGSL)
        if !e.generic_params.is_empty() {
            self.write_line(&format!("// Skipping generic enum {}", e.name));
            return;
        }

        // Skip if already generated (prevents duplicates from multiple import paths)
        if self.generated_enums.contains(&e.name) {
            return;
        }
        self.generated_enums.insert(e.name.clone());

        // Convert enum name to valid WGSL identifier
        let safe_enum_name = to_wgsl_identifier(&e.name);

        // Calculate max data size needed across all variants (in f32 units)
        let max_data_size = e
            .variants
            .iter()
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| self.type_size_in_f32(&f.ty, source_module))
                    .sum::<u32>()
            })
            .max()
            .unwrap_or(0);

        // Generate a constant for each variant
        for (idx, variant) in e.variants.iter().enumerate() {
            self.write_line(&format!(
                "const {}_{}: u32 = {}u;",
                safe_enum_name, variant.name, idx
            ));
        }

        // If any variant has data, generate a wrapper struct
        if max_data_size > 0 {
            self.write_blank_line();
            self.write_line(&format!("struct {} {{", safe_enum_name));
            self.indent += 1;
            self.write_line("discriminant: u32,");
            self.write_line(&format!("data: array<f32, {}>,", max_data_size));
            self.indent -= 1;
            self.write_line("};");
        }

        self.write_blank_line();
    }

    /// Calculate the f32 size of a type for WGSL data packing.
    ///
    /// Used for calculating enum data array sizes and binding load offsets.
    /// The source_module parameter provides context for looking up EnumIds,
    /// which are module-local and need the correct module to resolve.
    ///
    /// Note: No cycle detection needed here. Recursive types would cause infinite
    /// recursion, but the semantic analyzer detects circular type dependencies
    /// (see `detect_circular_type_dependencies` in semantic/mod.rs:3264) before
    /// codegen runs. WGSL also doesn't support recursive types.
    fn type_size_in_f32(&self, ty: &ResolvedType, source_module: Option<&IrModule>) -> u32 {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32
                | PrimitiveType::I32
                | PrimitiveType::U32
                | PrimitiveType::Bool
                | PrimitiveType::String => 1,
                PrimitiveType::Vec2 | PrimitiveType::IVec2 | PrimitiveType::UVec2 => 2,
                PrimitiveType::Vec3 | PrimitiveType::IVec3 | PrimitiveType::UVec3 => 3,
                PrimitiveType::Vec4 | PrimitiveType::IVec4 | PrimitiveType::UVec4 => 4,
                _ => 1,
            },
            ResolvedType::Enum(id) => {
                // First, try to look up in the source module (EnumIds are module-local)
                if let Some(src_mod) = source_module {
                    if (id.0 as usize) < src_mod.enums.len() {
                        let e = src_mod.get_enum(*id);
                        return 1 + self.enum_max_variant_size(e, source_module);
                    }
                }
                // Fallback: look up enum in imported modules (for cross-module references)
                for ir_mod in self.imported_modules.values() {
                    if (id.0 as usize) < ir_mod.enums.len() {
                        let e = ir_mod.get_enum(*id);
                        return 1 + self.enum_max_variant_size(e, Some(ir_mod));
                    }
                }
                1
            }
            ResolvedType::TypeParam(name) => {
                // Check source module first for enums (if provided)
                if let Some(src_mod) = source_module {
                    if let Some(e) = src_mod
                        .enums
                        .iter()
                        .find(|e| e.name == *name || e.name.ends_with(&format!("::{}", name)))
                    {
                        return 1 + self.enum_max_variant_size(e, source_module);
                    }
                }
                // Then check imported modules for enums
                for ir_mod in self.imported_modules.values() {
                    if let Some(e) = ir_mod
                        .enums
                        .iter()
                        .find(|e| e.name == *name || e.name.ends_with(&format!("::{}", name)))
                    {
                        return 1 + self.enum_max_variant_size(e, Some(ir_mod));
                    }
                }

                // Check source module first for structs (if provided)
                if let Some(src_mod) = source_module {
                    if let Some(s) = src_mod
                        .structs
                        .iter()
                        .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)))
                    {
                        return s
                            .fields
                            .iter()
                            .map(|f| self.type_size_in_f32(&f.ty, source_module))
                            .sum();
                    }
                }
                // Then check imported modules for structs
                for ir_mod in self.imported_modules.values() {
                    if let Some(s) = ir_mod
                        .structs
                        .iter()
                        .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)))
                    {
                        return s
                            .fields
                            .iter()
                            .map(|f| self.type_size_in_f32(&f.ty, Some(ir_mod)))
                            .sum();
                    }
                }

                1
            }
            _ => 1,
        }
    }

    /// Calculate the maximum variant data size for an enum (excluding discriminant).
    fn enum_max_variant_size(
        &self,
        e: &crate::ir::IrEnum,
        source_module: Option<&IrModule>,
    ) -> u32 {
        e.variants
            .iter()
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| self.type_size_in_f32(&f.ty, source_module))
                    .sum::<u32>()
            })
            .max()
            .unwrap_or(0)
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

        // Generate dispatch functions for external traits
        // (External trait data structs are generated earlier in generate())
        self.gen_external_trait_dispatch_functions();
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

        // Collect from main module structs
        for s in &self.module.structs {
            for field in &s.fields {
                Self::collect_external_traits(&field.ty, &mut external_traits);
            }
        }

        // Also collect from imported module structs (where Rect, Circle, etc. are defined)
        for imported in self.imported_modules.values() {
            for s in &imported.structs {
                for field in &s.fields {
                    Self::collect_external_traits_from(&field.ty, &mut external_traits, imported);
                }
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

    /// Generate struct definitions for imported trait implementors.
    ///
    /// When generating trait dispatch code, we need the actual struct definitions
    /// (e.g., `fill_Solid`, `fill_Pattern`) to exist. This function finds all
    /// structs from imported modules that implement traits and generates their
    /// WGSL struct definitions, along with any dependency structs they reference.
    fn gen_trait_implementor_structs(&mut self) {
        use std::collections::HashSet;

        // Collect all external trait names
        let mut external_traits: HashSet<String> = HashSet::new();
        for s in &self.module.structs {
            for field in &s.fields {
                Self::collect_external_traits(&field.ty, &mut external_traits);
            }
        }
        for imported in self.imported_modules.values() {
            for s in &imported.structs {
                for field in &s.fields {
                    Self::collect_external_traits_from(&field.ty, &mut external_traits, imported);
                }
            }
        }

        if external_traits.is_empty() {
            return;
        }

        // Track generated struct names to avoid duplicates
        let mut generated: HashSet<String> = HashSet::new();

        // Collect structs that implement traits
        let mut structs_to_generate: Vec<(IrStruct, IrModule)> = Vec::new();
        for trait_name in &external_traits {
            let simple_trait_name = simple_type_name(trait_name);

            // Search imported modules for implementors
            for imported_ir in self.imported_modules.values() {
                for (struct_idx, s) in imported_ir.structs.iter().enumerate() {
                    // Check if this struct implements the trait
                    let implements_trait = if simple_trait_name == "Fill" {
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        imported_ir.impls.iter().any(|imp| {
                            imp.struct_id() == Some(struct_id)
                                && imp.functions.iter().any(|f| f.name == "sample")
                        })
                    } else {
                        s.traits.iter().any(|trait_id| {
                            if (trait_id.0 as usize) < imported_ir.traits.len() {
                                let t = imported_ir.get_trait(*trait_id);
                                t.name == simple_trait_name
                            } else {
                                false
                            }
                        })
                    };

                    if implements_trait {
                        let safe_name = to_wgsl_identifier(&s.name);
                        if !generated.contains(&safe_name) {
                            generated.insert(safe_name);
                            structs_to_generate.push((s.clone(), imported_ir.clone()));
                        }
                    }
                }
            }
        }

        // Collect dependency structs and enums (referenced in fields)
        let mut all_structs: Vec<(IrStruct, IrModule)> = Vec::new();
        let mut all_enums: Vec<(crate::ir::IrEnum, IrModule)> = Vec::new();
        for (s, module) in &structs_to_generate {
            self.collect_dependency_structs(s, module, &mut generated, &mut all_structs);
            self.collect_dependency_enums(s, module, &mut all_enums);
        }

        // First generate dependency enums (PatternRepeat, etc.)
        for (e, source_module) in &all_enums {
            self.gen_enum_constants(e, Some(source_module));
        }

        // Then generate dependency structs (ColorStop, etc.)
        for (s, module) in &all_structs {
            self.gen_struct_from_imported(s, module);
            self.write_blank_line();
        }

        // Then generate trait implementor structs
        for (s, module) in &structs_to_generate {
            self.gen_struct_from_imported(&s, &module);
            self.write_blank_line();
        }
    }

    /// Collect enums that are referenced by a struct's fields.
    fn collect_dependency_enums(
        &self,
        s: &IrStruct,
        source_module: &IrModule,
        result: &mut Vec<(crate::ir::IrEnum, IrModule)>,
    ) {
        for field in &s.fields {
            if let ResolvedType::TypeParam(name) = &field.ty {
                let simple_name = simple_type_name(name);
                // Check if it's an enum in the source module
                if let Some(e) = source_module
                    .enums
                    .iter()
                    .find(|e| simple_type_name(&e.name) == simple_name)
                {
                    // Check if not already in result
                    if !result.iter().any(|(existing, _)| existing.name == e.name) {
                        result.push((e.clone(), source_module.clone()));
                    }
                }
            }
        }
    }

    /// Collect structs that are referenced by a struct's fields but aren't trait implementors.
    fn collect_dependency_structs(
        &self,
        s: &IrStruct,
        source_module: &IrModule,
        generated: &mut std::collections::HashSet<String>,
        result: &mut Vec<(IrStruct, IrModule)>,
    ) {
        for field in &s.fields {
            self.collect_struct_deps_from_type(&field.ty, source_module, generated, result);
        }
    }

    /// Recursively collect struct dependencies from a type.
    fn collect_struct_deps_from_type(
        &self,
        ty: &ResolvedType,
        source_module: &IrModule,
        generated: &mut std::collections::HashSet<String>,
        result: &mut Vec<(IrStruct, IrModule)>,
    ) {
        match ty {
            ResolvedType::TypeParam(name) => {
                let simple_name = simple_type_name(name);
                // Check if it's a struct in the source module (compare with simple_type_name since
                // struct names may have module prefixes like "fill::ColorStop")
                if let Some(s) = source_module
                    .structs
                    .iter()
                    .find(|s| simple_type_name(&s.name) == simple_name)
                {
                    let safe_name = to_wgsl_identifier(&s.name);
                    if !generated.contains(&safe_name) {
                        generated.insert(safe_name);
                        // Recursively collect dependencies first
                        for field in &s.fields {
                            self.collect_struct_deps_from_type(
                                &field.ty,
                                source_module,
                                generated,
                                result,
                            );
                        }
                        result.push((s.clone(), source_module.clone()));
                    }
                }
                // Also check if it's an enum that needs to be handled
                else if let Some(e) = source_module
                    .enums
                    .iter()
                    .find(|e| simple_type_name(&e.name) == simple_name)
                {
                    // Enums are handled separately, but we need to generate their constants
                    let safe_name = to_wgsl_identifier(&e.name);
                    if !generated.contains(&safe_name) {
                        generated.insert(safe_name);
                    }
                }
            }
            ResolvedType::Struct(id) => {
                let s = source_module.get_struct(*id);
                let safe_name = to_wgsl_identifier(&s.name);
                if !generated.contains(&safe_name) {
                    generated.insert(safe_name);
                    // Recursively collect dependencies first
                    for field in &s.fields {
                        self.collect_struct_deps_from_type(
                            &field.ty,
                            source_module,
                            generated,
                            result,
                        );
                    }
                    result.push((s.clone(), source_module.clone()));
                }
            }
            ResolvedType::Array(inner) => {
                self.collect_struct_deps_from_type(inner, source_module, generated, result);
            }
            ResolvedType::Optional(inner) => {
                self.collect_struct_deps_from_type(inner, source_module, generated, result);
            }
            _ => {}
        }
    }

    /// Generate a struct definition from an imported module.
    fn gen_struct_from_imported(&mut self, s: &IrStruct, source_module: &IrModule) {
        let safe_name = to_wgsl_identifier(&s.name);

        // Skip if already generated (prevents duplicates)
        if self.generated_structs.contains(&safe_name) {
            return;
        }
        self.generated_structs.insert(safe_name.clone());

        self.output.push_str(&format!("struct {} {{\n", safe_name));

        if s.fields.is_empty() {
            // WGSL doesn't allow empty structs; add a placeholder field
            self.output.push_str("    _placeholder: u32,\n");
        } else {
            for field in &s.fields {
                let field_type = self.type_to_wgsl_from(&field.ty, source_module);
                let field_name = Self::escape_wgsl_keyword(&field.name);
                self.output
                    .push_str(&format!("    {}: {},\n", field_name, field_type));
            }
        }

        self.output.push_str("}\n");
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

    /// Collect trait type names from an imported module's type.
    ///
    /// This handles the case where a trait type (like Fill) is defined in the same
    /// module as the struct using it. In this case, the type is `Trait(id)` not `External`.
    fn collect_external_traits_from(
        ty: &ResolvedType,
        traits: &mut std::collections::HashSet<String>,
        module: &IrModule,
    ) {
        match ty {
            ResolvedType::Trait(id) => {
                // Local trait - get name from module
                let trait_def = module.get_trait(*id);
                traits.insert(trait_def.name.clone());
            }
            ResolvedType::External {
                name,
                kind: crate::ir::ExternalKind::Trait,
                ..
            } => {
                traits.insert(name.clone());
            }
            ResolvedType::Optional(inner) => {
                Self::collect_external_traits_from(inner, traits, module);
            }
            ResolvedType::Array(inner) => {
                Self::collect_external_traits_from(inner, traits, module);
            }
            _ => {}
        }
    }

    /// Generate dispatch functions for external traits.
    ///
    /// For each external trait referenced in the module, generates dispatch functions
    /// for all methods defined on that trait's implementors. These functions switch
    /// on the type_tag in the trait data struct to call the appropriate implementor.
    fn gen_external_trait_dispatch_functions(&mut self) {
        use std::collections::{HashMap, HashSet};

        // Collect all trait names referenced in struct fields
        let mut external_traits: HashSet<String> = HashSet::new();

        // From main module
        for s in &self.module.structs {
            for field in &s.fields {
                Self::collect_external_traits(&field.ty, &mut external_traits);
            }
        }

        // From imported modules (where Rect, Circle, etc. with Fill fields are defined)
        for imported in self.imported_modules.values() {
            for s in &imported.structs {
                for field in &s.fields {
                    Self::collect_external_traits_from(&field.ty, &mut external_traits, imported);
                }
            }
        }

        if external_traits.is_empty() {
            return;
        }

        // For each external trait, collect implementors and their methods
        for trait_name in external_traits {
            let simple_trait_name = simple_type_name(&trait_name);

            // Collect implementors: struct_name -> (type_tag, impl methods)
            let mut implementors: Vec<(String, u32)> = Vec::new();
            let mut methods: HashMap<String, Vec<(String, String, Vec<(String, String)>)>> =
                HashMap::new(); // method_name -> [(struct_name, return_type, params)]
            let mut type_tag = 0u32;

            // Search imported modules for implementors
            // Sort module paths to ensure deterministic tag assignment order
            // Use string comparison for stable ordering across runs
            let mut sorted_module_paths: Vec<_> = self.imported_modules.keys().collect();
            sorted_module_paths.sort_by_key(|p| p.to_string_lossy().to_string());

            for module_path in sorted_module_paths {
                let imported_ir = &self.imported_modules[module_path];
                // Find structs that implement the trait
                // We check if the struct has a `sample` method for Fill trait (workaround for IR bug)

                // Sort structs by name within module for deterministic ordering
                // (IR lowering produces non-deterministic struct order due to HashMap usage)
                let mut sorted_struct_indices: Vec<usize> = (0..imported_ir.structs.len()).collect();
                sorted_struct_indices.sort_by_key(|&idx| &imported_ir.structs[idx].name);

                for struct_idx in sorted_struct_indices {
                    let s = &imported_ir.structs[struct_idx];
                    // For Fill trait, check if struct has a sample method
                    let implements_trait = if simple_trait_name == "Fill" {
                        // Check if this struct has an impl with a sample method
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        imported_ir.impls.iter().any(|imp| {
                            imp.struct_id() == Some(struct_id)
                                && imp.functions.iter().any(|f| f.name == "sample")
                        })
                    } else {
                        // For other traits, try the original trait ID check
                        s.traits.iter().any(|trait_id| {
                            if (trait_id.0 as usize) < imported_ir.traits.len() {
                                let t = imported_ir.get_trait(*trait_id);
                                t.name == simple_trait_name
                            } else {
                                false
                            }
                        })
                    };

                    if implements_trait {
                        let struct_name = s.name.clone();
                        implementors.push((struct_name.clone(), type_tag));

                        // Find impl block for this struct
                        let struct_id = crate::ir::StructId(struct_idx as u32);
                        for ir_impl in &imported_ir.impls {
                            if ir_impl.struct_id() == Some(struct_id) {
                                // Collect methods from this impl
                                for func in &ir_impl.functions {
                                    let return_type = func
                                        .return_type
                                        .as_ref()
                                        .map(|t| self.type_to_wgsl_from(t, imported_ir))
                                        .unwrap_or_else(|| "()".to_string());
                                    let params: Vec<(String, String)> = func
                                        .params
                                        .iter()
                                        .filter(|p| p.name != "self")
                                        .filter_map(|p| {
                                            p.ty.as_ref().map(|t| {
                                                (
                                                    p.name.clone(),
                                                    self.type_to_wgsl_from(t, imported_ir),
                                                )
                                            })
                                        })
                                        .collect();
                                    methods.entry(func.name.clone()).or_default().push((
                                        struct_name.clone(),
                                        return_type,
                                        params,
                                    ));
                                }
                            }
                        }

                        type_tag += 1;
                    }
                }
            }

            if implementors.is_empty() {
                continue;
            }

            // Generate type tag constants
            self.output.push_str(&format!(
                "// Type tags for {} implementors\n",
                simple_trait_name
            ));
            for (struct_name, tag) in &implementors {
                let safe_struct_name = to_wgsl_identifier(struct_name);
                self.output.push_str(&format!(
                    "const {}_TAG_{}: u32 = {}u;\n",
                    simple_trait_name.to_uppercase(),
                    safe_struct_name.to_uppercase(),
                    tag
                ));
            }
            self.write_blank_line();

            // Detect which implementors are recursive (have a field of the trait type)
            let recursive_implementors: HashSet<String> = implementors
                .iter()
                .filter(|(struct_name, _)| {
                    self.is_recursive_implementor(struct_name, &simple_trait_name)
                })
                .map(|(name, _)| name.clone())
                .collect();

            // Generate helper function for extracting nested trait data (if there are recursive implementors)
            if !recursive_implementors.is_empty() {
                self.gen_extract_nested_trait_data(&simple_trait_name);
            }

            // Generate dispatch functions for each method
            for (method_name, method_impls) in &methods {
                // Get return type and params from first impl (should be same for all)
                let (_, return_type, params) = &method_impls[0];

                // Check if any recursive implementor has this method
                let has_recursive_method = recursive_implementors.iter().any(|rname| {
                    method_impls.iter().any(|(sname, _, _)| sname == rname)
                });

                // Generate function signature
                let param_list: String =
                    std::iter::once(format!("self_data: {}Data", simple_trait_name))
                        .chain(params.iter().map(|(name, ty)| format!("{}: {}", name, ty)))
                        .collect::<Vec<_>>()
                        .join(", ");

                let return_clause = if return_type == "()" || return_type.is_empty() {
                    String::new()
                } else {
                    format!(" -> {}", return_type)
                };

                self.output.push_str(&format!(
                    "fn {}_{}({}){} {{\n",
                    simple_trait_name, method_name, param_list, return_clause
                ));

                // If there are recursive implementors, use iteration-based dispatch
                if has_recursive_method {
                    self.output
                        .push_str("    var current_data = self_data;\n");
                    // Add mutable copies of parameters that might change during iteration
                    for (param_name, _) in params {
                        self.output
                            .push_str(&format!("    var current_{} = {};\n", param_name, param_name));
                    }
                    self.output.push_str(&format!(
                        "    for (var _depth = 0u; _depth < {}u; _depth = _depth + 1u) {{\n",
                        MAX_TRAIT_DISPATCH_DEPTH
                    ));
                    self.output
                        .push_str("        switch current_data.type_tag {\n");

                    for (struct_name, tag) in &implementors {
                        let safe_struct_name = to_wgsl_identifier(struct_name);
                        let is_recursive = recursive_implementors.contains(struct_name);

                        if is_recursive {
                            // For recursive implementors, extract nested data and continue loop
                            self.output
                                .push_str(&format!("            case {}u: {{\n", tag));
                            self.gen_recursive_case_body(
                                &simple_trait_name,
                                struct_name,
                                &safe_struct_name,
                                params,
                            );
                            self.output.push_str("            }\n");
                        } else {
                            // For non-recursive implementors, call directly and return
                            let call_args: String = std::iter::once(format!(
                                "load_{}_{}(&current_data)",
                                simple_trait_name.to_lowercase(),
                                safe_struct_name.to_lowercase()
                            ))
                            .chain(params.iter().map(|(name, _)| format!("current_{}", name)))
                            .collect::<Vec<_>>()
                            .join(", ");

                            self.output.push_str(&format!(
                                "            case {}u: {{ return {}_{}({}); }}\n",
                                tag, safe_struct_name, method_name, call_args
                            ));
                        }
                    }

                    // Default case
                    self.gen_default_return("            ", return_type);
                    self.output.push_str("        }\n"); // end switch
                    self.output.push_str("    }\n"); // end for

                    // Return default if max depth exceeded
                    self.output.push_str("    // Max recursion depth exceeded\n");
                    self.gen_default_return_statement("    ", return_type);
                } else {
                    // No recursive implementors - use simple switch
                    self.output.push_str("    switch self_data.type_tag {\n");

                    for (struct_name, tag) in &implementors {
                        let safe_struct_name = to_wgsl_identifier(struct_name);
                        let call_args: String = std::iter::once(format!(
                            "load_{}_{}(&self_data)",
                            simple_trait_name.to_lowercase(),
                            safe_struct_name.to_lowercase()
                        ))
                        .chain(params.iter().map(|(name, _)| name.clone()))
                        .collect::<Vec<_>>()
                        .join(", ");

                        self.output.push_str(&format!(
                            "        case {}u: {{ return {}_{}({}); }}\n",
                            tag, safe_struct_name, method_name, call_args
                        ));
                    }

                    // Default case
                    self.gen_default_return("        ", return_type);
                    self.output.push_str("    }\n");
                }

                self.output.push_str("}\n\n");
            }

            // Generate load functions for each implementor
            for (struct_name, _) in &implementors {
                self.gen_external_trait_load_function(simple_trait_name, struct_name);
            }
        }
    }

    /// Generate a load function for a specific struct type from trait data.
    fn gen_external_trait_load_function(&mut self, trait_name: &str, struct_name: &str) {
        // Find the struct definition in imported modules
        let struct_def = self
            .imported_modules
            .values()
            .flat_map(|m| m.structs.iter())
            .find(|s| s.name == struct_name);

        if struct_def.is_none() {
            return;
        }
        let struct_def = struct_def.unwrap();

        let safe_struct_name = to_wgsl_identifier(struct_name);
        let fn_name = format!(
            "load_{}_{}",
            trait_name.to_lowercase(),
            safe_struct_name.to_lowercase()
        );
        let data_type = format!("{}Data", trait_name);

        self.output.push_str(&format!(
            "fn {}(data: ptr<function, {}>) -> {} {{\n",
            fn_name, data_type, safe_struct_name
        ));
        self.output
            .push_str(&format!("    var result: {};\n", safe_struct_name));

        let mut offset = 0u32;
        for field in &struct_def.fields {
            // Skip array fields - they can't be loaded from dispatch data
            // (would need a separate mechanism like storage buffer indices)
            if matches!(field.ty, ResolvedType::Array(_)) {
                continue;
            }

            let field_size = self.field_size_in_f32_external(&field.ty);
            let load_expr = self.gen_field_load_expr_external(&field.ty, "data", offset);
            let escaped_field_name = Self::escape_wgsl_keyword(&field.name);
            self.output.push_str(&format!(
                "    result.{} = {};\n",
                escaped_field_name, load_expr
            ));
            offset += field_size;
        }

        self.output.push_str("    return result;\n");
        self.output.push_str("}\n\n");
    }

    /// Check if an implementor struct has a recursive field (a field of the same trait type).
    fn is_recursive_implementor(&self, struct_name: &str, trait_name: &str) -> bool {
        // Find the struct in imported modules
        let struct_def = self
            .imported_modules
            .values()
            .flat_map(|m| m.structs.iter())
            .find(|s| s.name == struct_name);

        if let Some(s) = struct_def {
            // Check if any field has the trait type
            for field in &s.fields {
                match &field.ty {
                    ResolvedType::TypeParam(param_name) => {
                        if param_name == trait_name {
                            return true;
                        }
                    }
                    // Also check for External trait types (e.g., `source: Fill` in Pattern)
                    ResolvedType::External { name, kind, .. }
                        if matches!(kind, crate::ir::ExternalKind::Trait) =>
                    {
                        // Compare simple names (e.g., "Fill" vs "fill::Fill")
                        if simple_type_name(name) == trait_name || name == trait_name {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Generate the extract_nested_<trait>_data helper function.
    ///
    /// This function extracts nested trait data from a parent's data array at a given offset.
    fn gen_extract_nested_trait_data(&mut self, trait_name: &str) {
        let data_struct = format!("{}Data", trait_name);
        let fn_name = format!("extract_nested_{}_data", trait_name.to_lowercase());

        self.output.push_str(&format!(
            "// Extract nested {} from parent data array\n",
            data_struct
        ));
        self.output.push_str(&format!(
            "fn {}(parent: ptr<function, {}>, offset: u32) -> {} {{\n",
            fn_name, data_struct, data_struct
        ));
        self.output
            .push_str(&format!("    var result: {};\n", data_struct));
        self.output
            .push_str("    result.type_tag = u32(bitcast<u32>((*parent).data[offset]));\n");
        self.output
            .push_str("    result.element_index = u32(bitcast<u32>((*parent).data[offset + 1u]));\n");
        // Only copy NESTED_TRAIT_STORED_SIZE elements to match the offset calculation
        // The rest of result.data remains zeroed (default-initialized)
        self.output.push_str(&format!(
            "    for (var i = 0u; i < {}u; i = i + 1u) {{\n",
            NESTED_TRAIT_STORED_SIZE
        ));
        self.output
            .push_str("        result.data[i] = (*parent).data[offset + 2u + i];\n");
        self.output.push_str("    }\n");
        self.output.push_str("    return result;\n");
        self.output.push_str("}\n\n");
    }

    /// Generate the body of a recursive case in the dispatch loop.
    ///
    /// For Pattern-like structs, this extracts the nested trait data and transforms
    /// any relevant parameters (like UV coordinates).
    fn gen_recursive_case_body(
        &mut self,
        trait_name: &str,
        struct_name: &str,
        safe_struct_name: &str,
        _params: &[(String, String)],
    ) {
        // Load the recursive struct
        self.output.push_str(&format!(
            "                let _recursive_struct = load_{}_{}(&current_data);\n",
            trait_name.to_lowercase(),
            safe_struct_name.to_lowercase()
        ));

        // Find the struct to get field offsets
        let struct_def = self
            .imported_modules
            .values()
            .flat_map(|m| m.structs.iter())
            .find(|s| s.name == struct_name);

        if let Some(s) = struct_def {
            // Find the trait field and calculate its offset
            let mut offset = 0u32;
            let mut trait_field_offset = 0u32;
            let mut found_trait_field = false;

            for field in &s.fields {
                // Check for TypeParam or External trait type
                let is_trait_field = match &field.ty {
                    ResolvedType::TypeParam(param_name) => param_name == trait_name,
                    ResolvedType::External { name, kind, .. }
                        if matches!(kind, crate::ir::ExternalKind::Trait) =>
                    {
                        simple_type_name(name) == trait_name || name == trait_name
                    }
                    _ => false,
                };

                if is_trait_field {
                    trait_field_offset = offset;
                    found_trait_field = true;
                    break;
                }
                offset += self.field_size_in_f32_external(&field.ty);
            }

            // Special handling for Fill trait's Pattern struct - transform UV
            if trait_name == "Fill" && struct_name.contains("Pattern") {
                self.gen_pattern_uv_transform();
            }

            // Extract nested trait data (only if we found the trait field)
            if found_trait_field {
                self.output.push_str(&format!(
                    "                current_data = extract_nested_{}_data(&current_data, {}u);\n",
                    trait_name.to_lowercase(),
                    trait_field_offset
                ));
            }
        }

        self.output.push_str("                continue;\n");
    }

    /// Generate a default value for a type using the main module context.
    ///
    /// Used when struct fields are not provided in a struct instantiation
    /// for types in the main module.
    fn gen_default_value_for_type_local(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32 | PrimitiveType::I32 | PrimitiveType::U32 => "0.0".to_string(),
                PrimitiveType::Bool => "false".to_string(),
                PrimitiveType::Vec2 | PrimitiveType::IVec2 | PrimitiveType::UVec2 => {
                    "vec2<f32>(0.0, 0.0)".to_string()
                }
                PrimitiveType::Vec3 | PrimitiveType::IVec3 | PrimitiveType::UVec3 => {
                    "vec3<f32>(0.0, 0.0, 0.0)".to_string()
                }
                PrimitiveType::Vec4 | PrimitiveType::IVec4 | PrimitiveType::UVec4 => {
                    "vec4<f32>(0.0, 0.0, 0.0, 1.0)".to_string()
                }
                PrimitiveType::Mat2 | PrimitiveType::Mat3 | PrimitiveType::Mat4 => {
                    "mat4x4<f32>()".to_string()
                }
                _ => "0.0".to_string(),
            },
            ResolvedType::External { name, .. } => {
                let simple = simple_type_name(name);
                if simple == "Color4" {
                    "Color4(0.0, 0.0, 0.0, 1.0)".to_string()
                } else if simple == "Color" {
                    "Color_transparent()".to_string()
                } else if simple == "Dimension" {
                    // Dimension enum - use auto variant
                    "Dimension(Dimension_auto, array<f32, 1>(0.0))".to_string()
                } else {
                    format!("{}()", simple)
                }
            }
            ResolvedType::TypeParam(name) => {
                // Check if it's a known trait
                if self.is_known_trait(name) {
                    format!("{}Data()", name)
                } else {
                    // Check main module for struct
                    let struct_def = self.module.structs.iter().find(|s| &s.name == name);
                    if let Some(s) = struct_def {
                        let field_defaults: Vec<String> = s
                            .fields
                            .iter()
                            .map(|f| self.gen_default_value_for_type_local(&f.ty))
                            .collect();
                        format!("{}({})", name, field_defaults.join(", "))
                    } else {
                        // Check imported modules
                        for imported in self.imported_modules.values() {
                            if let Some(s) = imported.structs.iter().find(|s| &s.name == name) {
                                let field_defaults: Vec<String> = s
                                    .fields
                                    .iter()
                                    .map(|f| self.gen_default_value_for_type(&f.ty, imported))
                                    .collect();
                                return format!("{}({})", name, field_defaults.join(", "));
                            }
                        }
                        "0u".to_string()
                    }
                }
            }
            ResolvedType::Struct(id) => {
                if (id.0 as usize) < self.module.structs.len() {
                    let s = self.module.get_struct(*id);
                    let field_defaults: Vec<String> = s
                        .fields
                        .iter()
                        .map(|f| self.gen_default_value_for_type_local(&f.ty))
                        .collect();
                    format!("{}({})", s.name, field_defaults.join(", "))
                } else {
                    format!("{}()", self.type_to_wgsl(ty))
                }
            }
            ResolvedType::Enum(_) => {
                // Enums are u32 discriminants in WGSL, default to 0
                "0u".to_string()
            }
            ResolvedType::Optional(_) => {
                let type_str = self.type_to_wgsl(ty);
                format!("{}()", type_str)
            }
            _ => "0.0".to_string(),
        }
    }

    /// Generate a default value for a type.
    ///
    /// Used when struct fields are not provided in a struct instantiation.
    fn gen_default_value_for_type(&self, ty: &ResolvedType, source_module: &IrModule) -> String {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32 | PrimitiveType::I32 | PrimitiveType::U32 => "0.0".to_string(),
                PrimitiveType::Bool => "false".to_string(),
                PrimitiveType::Vec2 | PrimitiveType::IVec2 | PrimitiveType::UVec2 => {
                    "vec2<f32>(0.0, 0.0)".to_string()
                }
                PrimitiveType::Vec3 | PrimitiveType::IVec3 | PrimitiveType::UVec3 => {
                    "vec3<f32>(0.0, 0.0, 0.0)".to_string()
                }
                PrimitiveType::Vec4 | PrimitiveType::IVec4 | PrimitiveType::UVec4 => {
                    "vec4<f32>(0.0, 0.0, 0.0, 1.0)".to_string()
                }
                PrimitiveType::Mat2 | PrimitiveType::Mat3 | PrimitiveType::Mat4 => {
                    "mat4x4<f32>()".to_string()
                }
                _ => "0.0".to_string(),
            },
            ResolvedType::External { name, .. } => {
                let simple = simple_type_name(name);
                // Check if it's a known type with a default constructor
                if simple == "Color4" {
                    "Color4(0.0, 0.0, 0.0, 1.0)".to_string()
                } else if simple == "Color" {
                    // Color enum - use transparent
                    "Color_transparent()".to_string()
                } else if simple == "Dimension" {
                    // Dimension enum - use auto variant
                    "Dimension(Dimension_auto, array<f32, 1>(0.0))".to_string()
                } else {
                    // Try to find struct and generate zero-initialized version
                    format!("{}()", simple)
                }
            }
            ResolvedType::TypeParam(name) => {
                // Check if it's a trait - use zero-initialized trait data
                if self.is_known_trait(name) {
                    format!("{}Data()", name)
                } else {
                    // Check if it's a known struct
                    let struct_def = source_module.structs.iter().find(|s| &s.name == name);
                    if let Some(s) = struct_def {
                        // Generate a default struct instantiation
                        let field_defaults: Vec<String> = s
                            .fields
                            .iter()
                            .map(|f| self.gen_default_value_for_type(&f.ty, source_module))
                            .collect();
                        format!("{}({})", name, field_defaults.join(", "))
                    } else {
                        "0u".to_string()
                    }
                }
            }
            ResolvedType::Struct(_) | ResolvedType::Enum(_) => {
                let type_str = self.type_to_wgsl_from(ty, source_module);
                format!("{}()", type_str)
            }
            ResolvedType::Optional(_) => {
                // Optional defaults to nil/none - generate has_value=false
                let type_str = self.type_to_wgsl_from(ty, source_module);
                format!("{}()", type_str)
            }
            _ => "0.0".to_string(),
        }
    }

    /// Generate UV transformation for Pattern struct.
    fn gen_pattern_uv_transform(&mut self) {
        // Pattern has: source (Fill), width (f32), height (f32), repeat (PatternRepeat enum)
        // We need to transform UV based on repeat mode
        self.output.push_str(
            "                // Transform UV based on pattern repeat mode\n",
        );
        self.output.push_str(
            "                let _pattern_width = _recursive_struct.width;\n",
        );
        self.output.push_str(
            "                let _pattern_height = _recursive_struct.height;\n",
        );
        // Note: 'repeat' is not a WGSL keyword, so it's not escaped
        self.output.push_str(
            "                let _repeat_mode = _recursive_struct.repeat;\n",
        );
        // PatternRepeat enum: repeat=0, repeatX=1, repeatY=2, noRepeat=3
        self.output.push_str("                switch _repeat_mode {\n");
        self.output.push_str("                    case 0u: { // repeat\n");
        self.output.push_str("                        current_uv = vec2<f32>(\n");
        self.output.push_str(
            "                            fract(current_uv.x * _pattern_width),\n",
        );
        self.output.push_str(
            "                            fract(current_uv.y * _pattern_height)\n",
        );
        self.output.push_str("                        );\n");
        self.output.push_str("                    }\n");
        self.output.push_str("                    case 1u: { // repeatX\n");
        self.output.push_str("                        current_uv = vec2<f32>(\n");
        self.output
            .push_str("                            fract(current_uv.x * _pattern_width),\n");
        self.output.push_str("                            current_uv.y\n");
        self.output.push_str("                        );\n");
        self.output.push_str("                    }\n");
        self.output.push_str("                    case 2u: { // repeatY\n");
        self.output.push_str("                        current_uv = vec2<f32>(\n");
        self.output.push_str("                            current_uv.x,\n");
        self.output.push_str(
            "                            fract(current_uv.y * _pattern_height)\n",
        );
        self.output.push_str("                        );\n");
        self.output.push_str("                    }\n");
        self.output.push_str("                    default: { // noRepeat\n");
        self.output.push_str("                        current_uv = vec2<f32>(\n");
        self.output
            .push_str("                            current_uv.x * _pattern_width,\n");
        self.output
            .push_str("                            current_uv.y * _pattern_height\n");
        self.output.push_str("                        );\n");
        self.output.push_str("                    }\n");
        self.output.push_str("                }\n");
    }

    /// Generate default case for switch statement.
    fn gen_default_return(&mut self, indent: &str, return_type: &str) {
        if return_type == "()" || return_type.is_empty() {
            self.output
                .push_str(&format!("{}default: {{ }}\n", indent));
        } else if return_type == "Color4" {
            self.output.push_str(&format!(
                "{}default: {{ return Color4(0.0, 0.0, 0.0, 1.0); }}\n",
                indent
            ));
        } else if return_type.contains("vec4") {
            self.output.push_str(&format!(
                "{}default: {{ return vec4<f32>(0.0, 0.0, 0.0, 1.0); }}\n",
                indent
            ));
        } else if return_type.contains("vec3") {
            self.output.push_str(&format!(
                "{}default: {{ return vec3<f32>(0.0); }}\n",
                indent
            ));
        } else if return_type.contains("vec2") {
            self.output.push_str(&format!(
                "{}default: {{ return vec2<f32>(0.0); }}\n",
                indent
            ));
        } else if return_type == "f32" || return_type == "i32" || return_type == "u32" {
            self.output
                .push_str(&format!("{}default: {{ return 0.0; }}\n", indent));
        } else if return_type == "bool" {
            self.output
                .push_str(&format!("{}default: {{ return false; }}\n", indent));
        } else {
            self.output.push_str(&format!(
                "{}default: {{ return {}(); }}\n",
                indent, return_type
            ));
        }
    }

    /// Generate a default return statement.
    fn gen_default_return_statement(&mut self, indent: &str, return_type: &str) {
        if return_type == "()" || return_type.is_empty() {
            self.output.push_str(&format!("{}return;\n", indent));
        } else if return_type == "Color4" {
            self.output.push_str(&format!(
                "{}return Color4(0.0, 0.0, 0.0, 1.0);\n",
                indent
            ));
        } else if return_type.contains("vec4") {
            self.output.push_str(&format!(
                "{}return vec4<f32>(0.0, 0.0, 0.0, 1.0);\n",
                indent
            ));
        } else if return_type.contains("vec3") {
            self.output
                .push_str(&format!("{}return vec3<f32>(0.0);\n", indent));
        } else if return_type.contains("vec2") {
            self.output
                .push_str(&format!("{}return vec2<f32>(0.0);\n", indent));
        } else if return_type == "f32" || return_type == "i32" || return_type == "u32" {
            self.output.push_str(&format!("{}return 0.0;\n", indent));
        } else if return_type == "bool" {
            self.output.push_str(&format!("{}return false;\n", indent));
        } else {
            self.output
                .push_str(&format!("{}return {}();\n", indent, return_type));
        }
    }

    /// Calculate field size in f32 units for external types.
    fn field_size_in_f32_external(&self, ty: &ResolvedType) -> u32 {
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
            ResolvedType::External { name, .. } => {
                // Look up struct or enum in imported modules
                let simple_name = simple_type_name(name);

                // Try struct first
                if let Some(s) = self
                    .imported_modules
                    .values()
                    .flat_map(|m| m.structs.iter())
                    .find(|s| s.name == simple_name)
                {
                    return s
                        .fields
                        .iter()
                        .map(|f| self.field_size_in_f32_external(&f.ty))
                        .sum();
                }

                // Try enum - for enums, size is 1 (discriminant) + max variant field count
                // Use max_by_key to prefer enum with fields (re-exported enums may be empty)
                if let Some(e) = self
                    .imported_modules
                    .values()
                    .flat_map(|m| m.enums.iter())
                    .filter(|e| e.name == simple_name)
                    .max_by_key(|e| e.variants.iter().map(|v| v.fields.len()).sum::<usize>())
                {
                    let max_variant_size = e
                        .variants
                        .iter()
                        .map(|v| v.fields.len() as u32)
                        .max()
                        .unwrap_or(0);
                    return 1 + max_variant_size; // discriminant + data
                }

                1
            }
            ResolvedType::Struct(id) => {
                // Try main module first
                if (*id).0 < self.module.structs.len() as u32 {
                    let s = self.module.get_struct(*id);
                    return s
                        .fields
                        .iter()
                        .map(|f| self.field_size_in_f32_external(&f.ty))
                        .sum();
                }
                1
            }
            ResolvedType::TypeParam(name) => {
                // Check if this TypeParam is a known trait
                if self.is_known_trait(name) {
                    // Nested trait data: type_tag (1) + element_index (1) + stored data
                    // Use NESTED_TRAIT_STORED_SIZE to leave room for other fields in parent
                    (2 + NESTED_TRAIT_STORED_SIZE) as u32
                } else {
                    // Look up as struct in imported modules
                    if let Some(s) = self
                        .imported_modules
                        .values()
                        .flat_map(|m| m.structs.iter())
                        .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)))
                    {
                        return s
                            .fields
                            .iter()
                            .map(|f| self.field_size_in_f32_external(&f.ty))
                            .sum();
                    }

                    // Look up as enum in imported modules
                    // Use max_by_key to prefer enum with fields (re-exported enums may be empty)
                    if let Some(e) = self
                        .imported_modules
                        .values()
                        .flat_map(|m| m.enums.iter())
                        .filter(|e| e.name == *name || e.name.ends_with(&format!("::{}", name)))
                        .max_by_key(|e| e.variants.iter().map(|v| v.fields.len()).sum::<usize>())
                    {
                        let max_variant_size = e
                            .variants
                            .iter()
                            .map(|v| v.fields.len() as u32)
                            .max()
                            .unwrap_or(0);
                        return 1 + max_variant_size; // discriminant + data
                    }

                    1
                }
            }
            _ => 1,
        }
    }

    /// Check if a name refers to a known trait in imported modules.
    fn is_known_trait(&self, name: &str) -> bool {
        // Check all imported modules for a trait with this name
        for imported in self.imported_modules.values() {
            for t in &imported.traits {
                if t.name == name || t.name.ends_with(&format!("::{}", name)) {
                    return true;
                }
            }
        }
        // Also check main module
        for t in &self.module.traits {
            if t.name == name {
                return true;
            }
        }
        false
    }

    /// Generate load expression for reading a field from trait data array.
    fn gen_field_load_expr_external(
        &self,
        ty: &ResolvedType,
        data_ptr: &str,
        offset: u32,
    ) -> String {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32 => format!("(*{}).data[{}]", data_ptr, offset),
                PrimitiveType::I32 => {
                    format!("i32(bitcast<i32>((*{}).data[{}]))", data_ptr, offset)
                }
                PrimitiveType::U32 => {
                    format!("u32(bitcast<u32>((*{}).data[{}]))", data_ptr, offset)
                }
                PrimitiveType::Bool => format!("(*{}).data[{}] != 0.0", data_ptr, offset),
                PrimitiveType::Vec2 => format!(
                    "vec2<f32>((*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1
                ),
                PrimitiveType::Vec3 => format!(
                    "vec3<f32>((*{}).data[{}], (*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1,
                    data_ptr,
                    offset + 2
                ),
                PrimitiveType::Vec4 => format!(
                    "vec4<f32>((*{}).data[{}], (*{}).data[{}], (*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1,
                    data_ptr,
                    offset + 2,
                    data_ptr,
                    offset + 3
                ),
                _ => format!("(*{}).data[{}]", data_ptr, offset),
            },
            ResolvedType::External { name, kind, .. } => {
                // Check if this is a trait - traits are loaded using extract_nested_<trait>_data
                if matches!(kind, crate::ir::ExternalKind::Trait) {
                    let simple_name = simple_type_name(name);
                    return format!(
                        "extract_nested_{}_data({}, {}u)",
                        simple_name.to_lowercase(),
                        data_ptr,
                        offset
                    );
                }

                // Load a nested struct or enum from the data array
                let simple_name = simple_type_name(name);

                // Try struct first
                let struct_def = self
                    .imported_modules
                    .values()
                    .flat_map(|m| m.structs.iter())
                    .find(|s| simple_type_name(&s.name) == simple_name || s.name == simple_name);

                if let Some(s) = struct_def {
                    let mut field_loads = Vec::new();
                    let mut field_offset = offset;
                    for field in &s.fields {
                        field_loads.push(self.gen_field_load_expr_external(
                            &field.ty,
                            data_ptr,
                            field_offset,
                        ));
                        field_offset += self.field_size_in_f32_external(&field.ty);
                    }
                    return format!("{}({})", simple_name, field_loads.join(", "));
                }

                // Try enum - load discriminant + data array
                // Use max_by_key to prefer the definition with the most variant fields,
                // since re-exported enums in parent modules may have empty field lists
                let enum_def = self
                    .imported_modules
                    .values()
                    .flat_map(|m| m.enums.iter())
                    .filter(|e| simple_type_name(&e.name) == simple_name || e.name == simple_name)
                    .max_by_key(|e| {
                        e.variants.iter().map(|v| v.fields.len()).sum::<usize>()
                    });

                if let Some(e) = enum_def {
                    let max_variant_size = e
                        .variants
                        .iter()
                        .map(|v| v.fields.len())
                        .max()
                        .unwrap_or(0);

                    // Load discriminant as u32 - use u32() conversion since discriminant
                    // is stored as float value (e.g. 1.0 for discriminant 1)
                    let discriminant = format!(
                        "u32((*{}).data[{}])",
                        data_ptr, offset
                    );

                    // WGSL arrays must have size > 0, so use at least 1
                    let array_size = max_variant_size.max(1);

                    // Load data array
                    let data_loads: Vec<String> = (0..array_size)
                        .map(|i| {
                            if i < max_variant_size {
                                format!("(*{}).data[{}]", data_ptr, offset + 1 + i as u32)
                            } else {
                                // Padding for enums with no variant data
                                "0.0".to_string()
                            }
                        })
                        .collect();

                    return format!(
                        "{}({}, array<f32, {}>({}))",
                        simple_name,
                        discriminant,
                        array_size,
                        data_loads.join(", ")
                    );
                }

                format!("(*{}).data[{}]", data_ptr, offset)
            }
            ResolvedType::TypeParam(name) => {
                if self.is_known_trait(name) {
                    // For trait fields, we extract nested trait data
                    // This returns a call to extract_nested_<Trait>_data helper
                    format!(
                        "extract_nested_{}_data({}, {}u)",
                        name.to_lowercase(),
                        data_ptr,
                        offset
                    )
                } else {
                    // Try to load as struct
                    let struct_def = self
                        .imported_modules
                        .values()
                        .flat_map(|m| m.structs.iter())
                        .find(|s| s.name == *name || s.name.ends_with(&format!("::{}", name)));

                    if let Some(s) = struct_def {
                        let safe_name = to_wgsl_identifier(&s.name);
                        let mut field_loads = Vec::new();
                        let mut field_offset = offset;
                        for field in &s.fields {
                            field_loads.push(self.gen_field_load_expr_external(
                                &field.ty,
                                data_ptr,
                                field_offset,
                            ));
                            field_offset += self.field_size_in_f32_external(&field.ty);
                        }
                        return format!("{}({})", safe_name, field_loads.join(", "));
                    }

                    // Try to load as enum
                    // Use max_by_key to prefer the definition with the most variant fields,
                    // since re-exported enums in parent modules may have empty field lists
                    let enum_def = self
                        .imported_modules
                        .values()
                        .flat_map(|m| m.enums.iter())
                        .filter(|e| e.name == *name || e.name.ends_with(&format!("::{}", name)))
                        .max_by_key(|e| {
                            e.variants.iter().map(|v| v.fields.len()).sum::<usize>()
                        });

                    if let Some(e) = enum_def {
                        let safe_name = to_wgsl_identifier(&e.name);
                        let max_variant_size = e
                            .variants
                            .iter()
                            .map(|v| v.fields.len())
                            .max()
                            .unwrap_or(0);

                        // Load discriminant as u32 - use u32() conversion since discriminant
                        // is stored as float value (e.g. 1.0 for discriminant 1)
                        let discriminant = format!(
                            "u32((*{}).data[{}])",
                            data_ptr, offset
                        );

                        // For unit enums (no variant data), just return the discriminant
                        if max_variant_size == 0 {
                            return discriminant;
                        }

                        // Load data array for enums with variant data
                        let data_loads: Vec<String> = (0..max_variant_size)
                            .map(|i| format!("(*{}).data[{}]", data_ptr, offset + 1 + i as u32))
                            .collect();

                        return format!(
                            "{}({}, array<f32, {}>({}))",
                            safe_name,
                            discriminant,
                            max_variant_size,
                            data_loads.join(", ")
                        );
                    }

                    format!("(*{}).data[{}]", data_ptr, offset)
                }
            }
            _ => format!("(*{}).data[{}]", data_ptr, offset),
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

    /// Flatten an expression to f32 values for FillData packing.
    ///
    /// For Color enum variants (rgba, rgb, hsla), this returns the variant tag
    /// followed by the field values. For simple literals, returns the value.
    fn flatten_expr_to_f32s(&self, expr: &IrExpr) -> Vec<String> {
        match expr {
            IrExpr::Literal { value, .. } => {
                match value {
                    Literal::Number(n) => vec![n.to_string()],
                    Literal::Boolean(b) => vec![if *b { "1.0" } else { "0.0" }.to_string()],
                    _ => vec!["0.0".to_string()],
                }
            }
            IrExpr::EnumInst { variant, fields, ty, .. } => {
                // Handle InferredEnum variants
                if let ResolvedType::TypeParam(param_name) = ty {
                    if param_name == "InferredEnum" {
                        // Color variants
                        let color_variants = ["rgba", "rgb", "hsla", "hex"];
                        if color_variants.contains(&variant.as_str()) {
                            // Map variant to tag value as float - will be converted with u32() on load
                            let tag_value = match variant.as_str() {
                                "rgb" => "0.0",
                                "rgba" => "1.0",
                                "hsla" => "2.0",
                                "hex" => "3.0",
                                _ => "0.0",
                            };

                            let mut result = vec![tag_value.to_string()];

                            // Add field values - for rgba: r, g, b, a
                            for (_, field_expr) in fields {
                                result.extend(self.flatten_expr_to_f32s(field_expr));
                            }

                            // Pad to 5 values (1 tag + 4 data)
                            while result.len() < 5 {
                                result.push("0.0".to_string());
                            }

                            return result;
                        }

                        // Fill variants (InferredEnum with fill-like variant names)
                        let fill_variants = [
                            "solid", "linear", "radial", "angular", "pattern", "multilinear",
                        ];
                        if fill_variants.contains(&variant.as_str()) {
                            // Capitalize first letter to get struct name
                            let struct_name = {
                                let mut chars = variant.chars();
                                match chars.next() {
                                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                                    None => variant.clone(),
                                }
                            };

                            // Generate: type_tag (as f32(TAG)), element_index (0.0), field data
                            let tag_name = format!("FILL_TAG_FILL_{}", struct_name.to_uppercase());
                            let mut result = vec![
                                format!("f32({})", tag_name),
                                "0.0".to_string(), // element_index
                            ];

                            // Add field values
                            for (_, field_expr) in fields {
                                result.extend(self.flatten_expr_to_f32s(field_expr));
                            }

                            return result;
                        }
                    }
                }
                // For other enums, just return the variant tag
                vec!["0.0".to_string()]
            }
            IrExpr::StructInst {
                fields,
                ty,
                ..
            } => {
                // Check if this is a trait implementor (Fill type)
                // If so, we need to serialize: type_tag, element_index, nested data
                let struct_name = match ty {
                    ResolvedType::TypeParam(name) => name.clone(),
                    ResolvedType::Struct(id) => {
                        // Look up struct name from local module first
                        if (id.0 as usize) < self.module.structs.len() {
                            self.module.get_struct(*id).name.clone()
                        } else {
                            // Search in imported modules
                            let mut found_name = None;
                            for imported in self.imported_modules.values() {
                                if (id.0 as usize) < imported.structs.len() {
                                    found_name = Some(imported.get_struct(*id).name.clone());
                                    break;
                                }
                            }
                            match found_name {
                                Some(name) => name,
                                None => return vec![self.gen_expr(expr)],
                            }
                        }
                    }
                    _ => return vec![self.gen_expr(expr)],
                };
                let simple_name = simple_type_name(&struct_name);

                // Look for this struct in imported modules to check if it implements Fill
                // Fill implementors are identified by having a `sample` method in their impl block
                let is_fill_implementor = self.imported_modules.values().any(|m| {
                    m.structs.iter().enumerate().any(|(struct_idx, s)| {
                        simple_type_name(&s.name) == simple_name && {
                            let struct_id = crate::ir::StructId(struct_idx as u32);
                            m.impls.iter().any(|imp| {
                                imp.struct_id() == Some(struct_id)
                                    && imp.functions.iter().any(|f| f.name == "sample")
                            })
                        }
                    })
                });

                if is_fill_implementor {
                    // Generate FillData-compatible serialization:
                    // First: type_tag as f32
                    // Second: element_index (0)
                    // Then: nested field values
                    let tag_name = format!("FILL_TAG_FILL_{}", simple_name.to_uppercase());

                    // Check if this tag constant exists, else use a numeric fallback
                    let tag_value = format!("f32({})", tag_name);

                    let mut result = vec![tag_value, "0.0".to_string()]; // type_tag, element_index

                    // Flatten all fields
                    for (_, field_expr) in fields {
                        result.extend(self.flatten_expr_to_f32s(field_expr));
                    }

                    result
                } else {
                    // Not a Fill implementor - but might still need to flatten fields
                    // This handles structs like ColorStop that are used in Fill data arrays
                    let mut result = Vec::new();
                    for (_, field_expr) in fields {
                        result.extend(self.flatten_expr_to_f32s(field_expr));
                    }
                    if result.is_empty() {
                        // If no fields flattened, fall back to gen_expr
                        vec![self.gen_expr(expr)]
                    } else {
                        result
                    }
                }
            }
            IrExpr::Array { elements, .. } => {
                // Flatten all array elements
                let mut result = Vec::new();
                for elem in elements {
                    result.extend(self.flatten_expr_to_f32s(elem));
                }
                result
            }
            _ => {
                // For other expressions, generate normally (might not be correct for complex types)
                vec![self.gen_expr(expr)]
            }
        }
    }

    /// Generate monomorphized versions of generic structs.
    fn gen_monomorphized_structs(&mut self) {
        // Track generated names to avoid duplicates
        let mut generated_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Collect from local module
        let mut mono = Monomorphizer::new(self.module);
        mono.collect_instantiations();

        for (key, mono_struct) in mono.generate_monomorphized_structs() {
            if generated_names.insert(mono_struct.name.clone()) {
                self.gen_monomorphized_struct(&mono_struct);
                self.monomorph_names.entry(key).or_insert(mono_struct.name);
            }
        }

        // Collect from imported modules (P2 fix)
        for imported_ir in self.imported_modules.values() {
            let mut imported_mono = Monomorphizer::new(imported_ir);
            imported_mono.collect_instantiations();

            for (key, mono_struct) in imported_mono.generate_monomorphized_structs() {
                // Use the imported module for mangling, but check for duplicates
                let mangled_name = key.mangled_name(imported_ir);
                if generated_names.insert(mangled_name.clone()) {
                    self.gen_monomorphized_struct_from(&mono_struct, imported_ir);
                    self.monomorph_names.entry(key).or_insert(mono_struct.name);
                }
            }
        }
    }

    /// Generate a single monomorphized struct definition.
    fn gen_monomorphized_struct(&mut self, mono_struct: &IrStruct) {
        let safe_name = to_wgsl_identifier(&mono_struct.name);
        self.write_line(&format!("struct {} {{", safe_name));
        self.indent += 1;

        for field in &mono_struct.fields {
            let ty = self.type_to_wgsl(&field.ty);
            let field_name = Self::escape_wgsl_keyword(&field.name);
            self.write_line(&format!("{}: {},", field_name, ty));
        }

        self.indent -= 1;
        self.write_line("}");
        self.write_blank_line();
    }

    /// Generate a monomorphized struct using a foreign module for type lookups.
    fn gen_monomorphized_struct_from(&mut self, mono_struct: &IrStruct, source_module: &IrModule) {
        let safe_name = to_wgsl_identifier(&mono_struct.name);
        self.write_line(&format!("struct {} {{", safe_name));
        self.indent += 1;

        for field in &mono_struct.fields {
            let ty = self.type_to_wgsl_from(&field.ty, source_module);
            let field_name = Self::escape_wgsl_keyword(&field.name);
            self.write_line(&format!("{}: {},", field_name, ty));
        }

        self.indent -= 1;
        self.write_line("}");
        self.write_blank_line();
    }

    /// Generate WGSL code for a struct definition.
    ///
    /// Creates a WGSL struct with all fields typed according to WGSL conventions.
    fn gen_struct(&mut self, s: &IrStruct) {
        // Track struct start in source map
        let safe_name = to_wgsl_identifier(&s.name);

        // Skip if already generated (prevents duplicates from imports)
        if self.generated_structs.contains(&safe_name) {
            return;
        }
        self.generated_structs.insert(safe_name.clone());

        self.write_line_struct(&format!("struct {} {{", safe_name), &s.name);
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
    /// Impl blocks become standalone functions with the struct/enum name as prefix.
    /// Skips generating impl functions for generic structs/enums that aren't monomorphized.
    fn gen_impl(&mut self, i: &IrImpl) {
        // Skip generating impl for generic types that won't be emitted to WGSL
        if self.is_impl_target_generic(i, self.module) {
            return;
        }

        let type_name = self.get_impl_type_name(i, self.module);

        for func in &i.functions {
            self.gen_function(&type_name, func);
            self.write_blank_line();
        }
    }

    /// Check if an impl target is a generic struct/enum (has non-empty generic_params).
    fn is_impl_target_generic(&self, ir_impl: &crate::ir::IrImpl, module: &IrModule) -> bool {
        use crate::ir::ImplTarget;
        match ir_impl.target {
            ImplTarget::Struct(id) => !module.get_struct(id).generic_params.is_empty(),
            ImplTarget::Enum(id) => !module.get_enum(id).generic_params.is_empty(),
        }
    }

    /// Generate WGSL code for a function definition.
    ///
    /// Creates a WGSL function with proper signature and body. The struct_name
    /// is used as a prefix for the function name (e.g., `Vec2_length`).
    fn gen_function(&mut self, struct_name: &str, func: &IrFunction) {
        // Clear any hoisted statements from previous function generation
        self.hoisted_statements.borrow_mut().clear();

        // Generate function signature
        let return_type = func
            .return_type
            .as_ref()
            .map(|t| format!(" -> {}", self.type_to_wgsl(t)))
            .unwrap_or_default();

        // Generate parameters (replacing 'self' with typed parameter, escaping keywords)
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                if p.name == "self" {
                    format!("self_: {}", struct_name)
                } else {
                    let param_name = Self::escape_wgsl_keyword(&p.name);
                    let ty =
                        p.ty.as_ref()
                            .map(|t| self.type_to_wgsl(t))
                            .unwrap_or_else(|| "f32".to_string());
                    format!("{}: {}", param_name, ty)
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

            // Nil body - void function with no operations
            IrExpr::Literal {
                value: Literal::Nil,
                ..
            } => {
                // Nil function bodies mean "do nothing" - generate empty body for void functions
                if return_type.is_some() {
                    self.write_line("return;");
                }
                // For void functions, an empty body is valid
            }

            // If expressions with block branches need statement-level handling
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => {
                // Only use statement-level if when branches actually have statements
                if Self::branch_has_statements(then_branch)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::branch_has_statements(e))
                {
                    self.gen_if_body(condition, then_branch, else_branch, ty, return_type);
                } else {
                    // Simple if-else without statements - use select()
                    let expr_str = self.gen_expr(body);
                    if return_type.is_some() {
                        self.write_line(&format!("return {};", expr_str));
                    } else {
                        // For void returns, skip bare variable refs and nil placeholders
                        // WGSL doesn't allow bare identifiers as statements
                        self.write_expr_as_statement(&expr_str);
                    }
                }
            }

            // Other expressions can be returned directly
            _ => {
                let expr_str = self.gen_expr(body);
                // Flush any hoisted statements before the expression
                self.flush_hoisted_statements();
                if return_type.is_some() {
                    self.write_line(&format!("return {};", expr_str));
                } else {
                    // For void returns, skip bare variable refs and nil placeholders
                    // WGSL doesn't allow bare identifiers as statements
                    self.write_expr_as_statement(&expr_str);
                }
            }
        }
    }

    /// Generate if/else body for the main module.
    ///
    /// Handles if-else expressions at statement level, properly preserving
    /// let bindings in block branches that would be lost with select().
    fn gen_if_body(
        &mut self,
        condition: &IrExpr,
        then_branch: &IrExpr,
        else_branch: &Option<Box<IrExpr>>,
        _ty: &ResolvedType,
        return_type: Option<&ResolvedType>,
    ) {
        let cond_str = self.gen_expr(condition);

        // Declare result variable if we have a return type
        if let Some(ret_ty) = return_type {
            let ret_ty_str = self.type_to_wgsl(ret_ty);
            self.write_line(&format!("var if_result: {};", ret_ty_str));
        }

        // Generate if statement
        self.write_line(&format!("if ({}) {{", cond_str));
        self.indent += 1;

        // Generate then branch
        self.gen_branch_body(then_branch, return_type.is_some());

        self.indent -= 1;

        // Generate else branch if present
        if let Some(else_expr) = else_branch {
            self.write_line("} else {");
            self.indent += 1;
            self.gen_branch_body(else_expr, return_type.is_some());
            self.indent -= 1;
        }

        self.write_line("}");

        if return_type.is_some() {
            self.write_line("return if_result;");
        }
    }

    /// Generate branch body for the main module.
    ///
    /// Handles block expressions by emitting statements, or simple expressions
    /// by assigning to if_result.
    fn gen_branch_body(&mut self, branch: &IrExpr, has_return: bool) {
        use crate::ir::IrBlockStatement;

        match branch {
            IrExpr::Block {
                statements, result, ..
            } => {
                // Generate statements first
                for stmt in statements {
                    match stmt {
                        IrBlockStatement::Let { name, value, .. } => {
                            // Handle closure values: generate a function instead of a let binding
                            if let IrExpr::Closure { params, body, .. } = value {
                                let fn_name = self.gen_closure_fn_name(name);
                                let param_strs: Vec<String> = params
                                    .iter()
                                    .map(|(pname, pty)| format!("{}: {}", pname, self.type_to_wgsl(pty)))
                                    .collect();
                                let return_ty = self.type_to_wgsl(body.ty());
                                let body_str = self.gen_expr(body);
                                let fn_source = format!(
                                    "fn {}({}) -> {} {{ return {}; }}",
                                    fn_name,
                                    param_strs.join(", "),
                                    return_ty,
                                    body_str
                                );
                                self.closure_functions.borrow_mut().insert(name.clone(), fn_name);
                                self.pending_closure_fns.borrow_mut().push(fn_source);
                                continue;
                            }
                            let value_str = self.gen_expr(value);
                            self.write_line(&format!("let {} = {};", name, value_str));
                        }
                        IrBlockStatement::Assign { target, value } => {
                            let target_str = self.gen_expr(target);
                            let value_str = self.gen_expr(value);
                            self.write_line(&format!("{} = {};", target_str, value_str));
                        }
                        IrBlockStatement::Expr(expr) => {
                            let expr_str = self.gen_expr(expr);
                            self.write_line(&format!("{};", expr_str));
                        }
                    }
                }
                // Generate result expression
                if has_return {
                    let result_str = self.gen_expr(result);
                    self.write_line(&format!("if_result = {};", result_str));
                }
            }
            // Nested if - generate inline if/else without redeclaring if_result
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // For nested ifs, we reuse the same if_result variable
                let cond_str = self.gen_expr(condition);
                self.write_line(&format!("if ({}) {{", cond_str));
                self.indent += 1;
                self.gen_branch_body(then_branch, has_return);
                self.indent -= 1;
                if let Some(else_expr) = else_branch {
                    self.write_line("} else {");
                    self.indent += 1;
                    self.gen_branch_body(else_expr, has_return);
                    self.indent -= 1;
                }
                self.write_line("}");
            }
            // Simple expression
            _ => {
                if has_return {
                    let expr_str = self.gen_expr(branch);
                    self.write_line(&format!("if_result = {};", expr_str));
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

    /// Infer the compile-time size of an array expression from a foreign module.
    fn infer_array_size_from_foreign(
        &self,
        collection: &IrExpr,
        source_module: &IrModule,
    ) -> Option<usize> {
        match collection {
            // Literal array - size is known
            IrExpr::Array { elements, .. } => Some(elements.len()),

            // Reference - check if it's a constant array or function parameter
            IrExpr::Reference { path, .. } => {
                // Try to look up the let binding in source module
                if path.len() == 1 {
                    let name = &path[0];
                    if let Some(let_binding) = source_module.get_let(name) {
                        return self
                            .infer_array_size_from_foreign(&let_binding.value, source_module);
                    }
                    // Check if it's a function parameter with array type
                    if let Some(param_ty) = self.current_function_params.get(name) {
                        // Arrays have a fixed size DEFAULT_MAX_ARRAY_SIZE
                        if matches!(param_ty, ResolvedType::Array(_)) {
                            return Some(DEFAULT_MAX_ARRAY_SIZE);
                        }
                    }
                }
                None
            }

            // Self field reference - check if it's an array field
            // All arrays in WGSL have fixed size DEFAULT_MAX_ARRAY_SIZE
            IrExpr::SelfFieldRef { ty, .. } => {
                // If the field has array type, return the default max size
                if matches!(ty, ResolvedType::Array(_)) {
                    Some(DEFAULT_MAX_ARRAY_SIZE)
                } else {
                    None
                }
            }

            // Let reference - look up the binding in source module
            IrExpr::LetRef { name, .. } => {
                if let Some(let_binding) = source_module.get_let(name) {
                    return self.infer_array_size_from_foreign(&let_binding.value, source_module);
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
            IrExpr::Literal { value, ty } => self.gen_literal(value, ty),

            IrExpr::Reference { path, .. } => {
                // Escape reserved keywords in reference paths
                let escaped_path: Vec<String> =
                    path.iter().map(|p| Self::escape_wgsl_keyword(p)).collect();
                escaped_path.join(".")
            }

            IrExpr::SelfFieldRef { field, .. } => {
                format!("self_.{}", Self::escape_wgsl_keyword(field))
            }

            IrExpr::FieldAccess { object, field, .. } => {
                let object_str = self.gen_expr(object);
                format!("{}.{}", object_str, Self::escape_wgsl_keyword(field))
            }

            IrExpr::LetRef { name, .. } => name.clone(),

            IrExpr::BinaryOp {
                left, op, right, ..
            } => {
                // Handle nil comparisons specially (x == nil, x != nil)
                if matches!(op, BinaryOperator::Eq | BinaryOperator::Ne) {
                    if let Some(nil_cmp) = self.gen_nil_comparison_local(left, op, right) {
                        return nil_cmp;
                    }
                }
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
                    .map(|id| to_wgsl_identifier(&self.module.get_struct(id).name))
                    .unwrap_or_else(|| "Unknown".to_string());

                // Check if this struct implements Fill trait (by having a `sample` method)
                // This enables direct struct instantiation like `fill::relative::Linear(...)`
                // to automatically wrap in FillData for trait dispatch
                // Compare WGSL-mangled names since `name` is already mangled
                let is_fill_implementor = self.imported_modules.values().any(|m| {
                    m.structs.iter().enumerate().any(|(struct_idx, s)| {
                        to_wgsl_identifier(&s.name) == name && {
                            let struct_id = crate::ir::StructId(struct_idx as u32);
                            m.impls.iter().any(|imp| {
                                imp.struct_id() == Some(struct_id)
                                    && imp.functions.iter().any(|f| f.name == "sample")
                            })
                        }
                    })
                }) || self.module.structs.iter().enumerate().any(|(struct_idx, s)| {
                    to_wgsl_identifier(&s.name) == name && {
                        let sid = crate::ir::StructId(struct_idx as u32);
                        self.module.impls.iter().any(|imp| {
                            imp.struct_id() == Some(sid)
                                && imp.functions.iter().any(|f| f.name == "sample")
                        })
                    }
                });

                if is_fill_implementor {
                    // Generate FillData wrapping for trait dispatch
                    let type_tag = format!("FILL_TAG_{}", name.to_uppercase());

                    // Flatten fields to f32s for FillData array
                    let mut data_values: Vec<String> = Vec::new();
                    for (_, field_expr) in fields {
                        data_values.extend(self.flatten_expr_to_f32s(field_expr));
                    }
                    while data_values.len() < DEFAULT_MAX_DISPATCH_DATA_SIZE {
                        data_values.push("0.0".to_string());
                    }

                    return format!(
                        "FillData({}, 0u, array<f32, {}>({}))",
                        type_tag,
                        DEFAULT_MAX_DISPATCH_DATA_SIZE,
                        data_values.join(", ")
                    );
                }

                // WGSL struct constructors use positional arguments.
                // We need to reorder fields to match struct definition order.
                let field_strs: Vec<String> = if let Some(id) = struct_id {
                    // Get struct field order from definition
                    let struct_def = self.module.get_struct(*id);
                    let field_map: std::collections::HashMap<&str, &IrExpr> = fields
                        .iter()
                        .map(|(name, expr)| (name.as_str(), expr))
                        .collect();

                    // Emit values in struct field order
                    struct_def
                        .fields
                        .iter()
                        .map(|field| {
                            if let Some(expr) = field_map.get(field.name.as_str()) {
                                let value = self.gen_expr(expr);
                                // If field type is Optional, wrap value in Optional wrapper
                                if let ResolvedType::Optional(inner) = &field.ty {
                                    let inner_type = self.type_to_wgsl(inner);
                                    format!("Optional_{}(true, {})", inner_type, value)
                                } else {
                                    value
                                }
                            } else {
                                // Generate default value for missing field
                                self.gen_default_value_for_type_local(&field.ty)
                            }
                        })
                        .collect()
                } else {
                    // For builtin types without struct_id, use order as-is
                    fields.iter().map(|(_, e)| self.gen_expr(e)).collect()
                };

                format!("{}({})", name, field_strs.join(", "))
            }

            IrExpr::FunctionCall { path, args, .. } => {
                // Check if this is a closure call
                if let Some(closure_fn_name) = self.get_closure_fn_name(path) {
                    let arg_strs: Vec<String> =
                        args.iter().map(|(_, expr)| self.gen_expr(expr)).collect();
                    return format!("{}({})", closure_fn_name, arg_strs.join(", "));
                }

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

                // Method calls need mangled names: TypeName_method
                // Check if receiver is "self" - use current_impl_type for mangling
                let is_self_receiver = matches!(
                    receiver.as_ref(),
                    IrExpr::Reference { path, .. } if path.len() == 1 && path[0] == "self"
                );

                let mangled_name = if is_self_receiver {
                    // Use current impl type for self method calls
                    if let Some(ref impl_type) = self.current_impl_type {
                        format!("{}_{}", impl_type, method)
                    } else {
                        method.clone()
                    }
                } else {
                    // Determine the type name from the receiver's type
                    let receiver_ty = receiver.ty();
                    Self::get_method_type_name(receiver_ty, self.module)
                        .map(|type_name| format!("{}_{}", type_name, method))
                        .unwrap_or_else(|| method.clone())
                };

                // Generate as a function call with receiver as first arg
                let all_args = std::iter::once(recv)
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({})", mangled_name, all_args)
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

            IrExpr::Array { elements, ty } => {
                let elem_strs: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                // For empty arrays, we need to include type information
                if elem_strs.is_empty() {
                    if let ResolvedType::Array(inner) = ty {
                        let elem_ty = self.type_to_wgsl(inner);
                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                    } else {
                        format!("array<f32, {}>()", DEFAULT_MAX_ARRAY_SIZE)
                    }
                } else {
                    format!("array({})", elem_strs.join(", "))
                }
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
                // Handle InferredEnum (TypeParam("InferredEnum")) specially
                if let ResolvedType::TypeParam(param_name) = ty {
                    if param_name == "InferredEnum" {
                        // Check if this is a Color enum variant
                        let color_variants = ["rgba", "rgb", "hsla", "hex"];
                        if color_variants.contains(&variant.as_str()) {
                            // Generate Color enum instantiation
                            let field_values: Vec<String> =
                                fields.iter().map(|(_, e)| self.gen_expr(e)).collect();
                            let mut data_values = field_values;
                            while data_values.len() < 4 {
                                data_values.push("0.0".to_string());
                            }
                            return format!(
                                "Color(Color_{}, array<f32, 4>({}))",
                                variant,
                                data_values.join(", ")
                            );
                        }

                        // Check if this is a Fill trait implementor variant
                        let fill_variants = ["solid", "linear", "radial", "angular", "pattern", "multilinear"];
                        if fill_variants.contains(&variant.as_str()) {
                            // Generate FillData instantiation
                            // Map variant name to struct name (capitalize first letter)
                            let struct_name = {
                                let mut chars = variant.chars();
                                match chars.next() {
                                    None => String::new(),
                                    Some(first) => {
                                        first.to_uppercase().chain(chars).collect::<String>()
                                    }
                                }
                            };

                            // Get type tag - struct names are like "fill::Solid" -> "FILL_TAG_FILL_SOLID"
                            let type_tag =
                                format!("FILL_TAG_FILL_{}", struct_name.to_uppercase());

                            // Generate field values flattened to f32s for FillData
                            let mut data_values: Vec<String> = Vec::new();
                            for (_, e) in fields {
                                data_values.extend(self.flatten_expr_to_f32s(e));
                            }
                            while data_values.len() < DEFAULT_MAX_DISPATCH_DATA_SIZE {
                                data_values.push("0.0".to_string());
                            }
                            return format!(
                                "FillData({}, 0u, array<f32, {}>({}))",
                                type_tag,
                                DEFAULT_MAX_DISPATCH_DATA_SIZE,
                                data_values.join(", ")
                            );
                        }
                    }
                }

                // Get the enum and its definition
                let (enum_name, max_data_size) = if let Some(id) = enum_id {
                    let e = self.module.get_enum(*id);
                    let max_size = e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0);
                    (e.name.clone(), max_size)
                } else if let ResolvedType::Enum(id) = ty {
                    let e = self.module.get_enum(*id);
                    let max_size = e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0);
                    (e.name.clone(), max_size)
                } else {
                    ("UnknownEnum".to_string(), 0)
                };

                if fields.is_empty() {
                    // Simple unit variant - reference the constant
                    format!("{}_{}", enum_name, variant)
                } else if max_data_size == 0 {
                    // Enum has data but max_size is 0 (shouldn't happen)
                    format!("{}_{}", enum_name, variant)
                } else {
                    // Generate wrapper struct with discriminant and data
                    let field_values: Vec<String> =
                        fields.iter().map(|(_, e)| self.gen_expr(e)).collect();
                    // Pad with zeros to fill the data array
                    let mut data_values = field_values;
                    while data_values.len() < max_data_size {
                        data_values.push("0.0".to_string());
                    }
                    format!(
                        "{}({}_{}, array<f32, {}>({}))",
                        enum_name,
                        enum_name,
                        variant,
                        max_data_size,
                        data_values.join(", ")
                    )
                }
            }

            IrExpr::EventMapping { variant, param, .. } => {
                // Event mappings are metadata for the runtime, not WGSL code
                // Generate a comment placeholder
                let param_str = param.as_deref().unwrap_or("()");
                format!("/* event: {} -> .{} */", param_str, variant)
            }

            // Closure - when encountered directly (not via let binding), generate inline function
            IrExpr::Closure { params, body, .. } => {
                // Generate a closure function with a unique name
                let fn_name = self.gen_closure_fn_name("anon");
                let param_strs: Vec<String> = params
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.type_to_wgsl(ty)))
                    .collect();
                let return_ty = self.type_to_wgsl(body.ty());
                let body_str = self.gen_expr(body);
                let fn_source = format!(
                    "fn {}({}) -> {} {{ return {}; }}",
                    fn_name,
                    param_strs.join(", "),
                    return_ty,
                    body_str
                );
                self.pending_closure_fns.borrow_mut().push(fn_source);
                // Return just the function name (caller will invoke it)
                fn_name
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

                if !hoisted.is_empty() {
                    // Push hoisted statements to the accumulator.
                    // They will be flushed at statement-level context.
                    self.push_hoisted_statements(hoisted);
                }

                // Return just the result expression
                result_expr
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
                    // Handle closure values: generate a function instead of a let binding
                    if let IrExpr::Closure { params, body, .. } = value {
                        // Register the closure and generate its function
                        let fn_name = self.gen_closure_fn_name(name);
                        let param_strs: Vec<String> = params
                            .iter()
                            .map(|(pname, pty)| format!("{}: {}", pname, self.type_to_wgsl(pty)))
                            .collect();
                        let return_ty = self.type_to_wgsl(body.ty());
                        let body_str = self.gen_expr(body);
                        let fn_source = format!(
                            "fn {}({}) -> {} {{ return {}; }}",
                            fn_name,
                            param_strs.join(", "),
                            return_ty,
                            body_str
                        );
                        self.closure_functions
                            .borrow_mut()
                            .insert(name.clone(), fn_name);
                        self.pending_closure_fns.borrow_mut().push(fn_source);
                        continue;
                    }

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
                // Handle nil comparisons specially (x == nil, x != nil)
                if matches!(op, BinaryOperator::Eq | BinaryOperator::Ne) {
                    if let Some(nil_cmp) =
                        self.gen_nil_comparison_with_renames(left, op, right, renames)
                    {
                        return nil_cmp;
                    }
                }
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
                // Check if this is a closure call
                if let Some(closure_fn_name) = self.get_closure_fn_name(path) {
                    let arg_strs: Vec<String> = args
                        .iter()
                        .map(|(_, e)| self.gen_expr_with_renames(e, renames))
                        .collect();
                    return format!("{}({})", closure_fn_name, arg_strs.join(", "));
                }

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

                // Method calls need mangled names: TypeName_method
                let is_self_receiver = matches!(
                    receiver.as_ref(),
                    IrExpr::Reference { path, .. } if path.len() == 1 && path[0] == "self"
                );

                let mangled_name = if is_self_receiver {
                    if let Some(ref impl_type) = self.current_impl_type {
                        format!("{}_{}", impl_type, method)
                    } else {
                        method.clone()
                    }
                } else {
                    let receiver_ty = receiver.ty();
                    Self::get_method_type_name(receiver_ty, self.module)
                        .map(|type_name| format!("{}_{}", type_name, method))
                        .unwrap_or_else(|| method.clone())
                };

                let all_args = std::iter::once(recv)
                    .chain(arg_strs)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", mangled_name, all_args)
            }

            IrExpr::Array { elements, ty } => {
                let elem_strs: Vec<String> = elements
                    .iter()
                    .map(|e| self.gen_expr_with_renames(e, renames))
                    .collect();
                if elem_strs.is_empty() {
                    if let ResolvedType::Array(inner) = ty {
                        let elem_ty = self.type_to_wgsl(inner);
                        format!("array<{}, {}>()", elem_ty, DEFAULT_MAX_ARRAY_SIZE)
                    } else {
                        format!("array<f32, {}>()", DEFAULT_MAX_ARRAY_SIZE)
                    }
                } else {
                    format!("array({})", elem_strs.join(", "))
                }
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
                    // Handle closure values: generate a function instead of a let binding
                    if let IrExpr::Closure { params, body, .. } = value {
                        let fn_name = self.gen_closure_fn_name(name);
                        let param_strs: Vec<String> = params
                            .iter()
                            .map(|(pname, pty)| format!("{}: {}", pname, self.type_to_wgsl(pty)))
                            .collect();
                        let return_ty = self.type_to_wgsl(body.ty());
                        let body_str = self.gen_expr(body);
                        let fn_source = format!(
                            "fn {}({}) -> {} {{ return {}; }}",
                            fn_name,
                            param_strs.join(", "),
                            return_ty,
                            body_str
                        );
                        self.closure_functions
                            .borrow_mut()
                            .insert(name.clone(), fn_name);
                        self.pending_closure_fns.borrow_mut().push(fn_source);
                        continue;
                    }

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
    /// The `ty` parameter is used to determine the correct WGSL suffix for numeric literals.
    fn gen_literal(&self, lit: &Literal, ty: &ResolvedType) -> String {
        match lit {
            Literal::Number(n) => {
                // Use type information to generate correct WGSL suffix
                match ty {
                    ResolvedType::Primitive(PrimitiveType::U32) => {
                        // Unsigned integer: 3u
                        format!("{}u", *n as u32)
                    }
                    ResolvedType::Primitive(PrimitiveType::I32) => {
                        // Signed integer: 3i
                        format!("{}i", *n as i32)
                    }
                    ResolvedType::Primitive(PrimitiveType::F32)
                    | ResolvedType::Primitive(PrimitiveType::Number) => {
                        // Float: 3.0
                        if n.fract() == 0.0 {
                            format!("{}.0", n)
                        } else {
                            format!("{}", n)
                        }
                    }
                    _ => {
                        // Default to float format for unknown types
                        if n.fract() == 0.0 {
                            format!("{}.0", n)
                        } else {
                            format!("{}", n)
                        }
                    }
                }
            }
            Literal::UnsignedInt(n) => format!("{}u", n),
            Literal::SignedInt(n) => format!("{}i", n),
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

    /// Generate an expression that can be used as an array index.
    ///
    /// In WGSL, array indices must be integers (u32 or i32), not floats.
    /// This function ensures numeric literals are generated as integers.
    fn gen_array_index_expr(&self, expr: &IrExpr, source_module: &IrModule) -> String {
        match expr {
            IrExpr::Literal { value: Literal::Number(n), .. } => {
                // Generate as integer if no fractional part
                if n.fract() == 0.0 {
                    format!("{}u", *n as u32)
                } else {
                    // Shouldn't happen for valid array indices, but handle gracefully
                    format!("u32({})", n)
                }
            }
            _ => {
                // For other expressions, generate normally and let WGSL type checking handle it
                self.gen_expr_from_foreign(expr, source_module)
            }
        }
    }

    /// Convert a resolved type to its WGSL type name.
    ///
    /// Maps FormaLang types to their WGSL equivalents, handling structs,
    /// primitives, arrays, generics, and external types.
    fn type_to_wgsl(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => self.primitive_to_wgsl(p),

            ResolvedType::Struct(id) => to_wgsl_identifier(&self.module.get_struct(*id).name),

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
                // WGSL optionals use wrapper structs: Optional_T { has_value: bool, value: T }
                let inner_name = self.type_to_wgsl(inner);
                format!("Optional_{}", inner_name)
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

            ResolvedType::Enum(id) => {
                let e = self.module.get_enum(*id);
                // Check if any variant has data - if so, use struct name
                let has_data = e.variants.iter().any(|v| !v.fields.is_empty());
                if has_data {
                    to_wgsl_identifier(&e.name)
                } else {
                    "u32".to_string()
                }
            }

            ResolvedType::Trait(id) => {
                // Map trait types to their dispatch data struct (e.g., Fill -> FillData)
                let trait_def = self.module.get_trait(*id);
                format!("{}Data", to_wgsl_identifier(&trait_def.name))
            }

            ResolvedType::External { name, kind, .. } => self.external_type_to_wgsl(name, kind),

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

            ResolvedType::Closure { param_tys, return_ty } => {
                // WGSL doesn't have first-class functions
                // Closures are converted to named functions during codegen
                let params: Vec<String> = param_tys.iter().map(|t| self.type_to_wgsl(t)).collect();
                format!(
                    "/* closure({}) -> {} */",
                    params.join(", "),
                    self.type_to_wgsl(return_ty)
                )
            }
        }
    }

    /// Convert an external type to its WGSL representation.
    ///
    /// Handles External struct, trait, and enum types consistently.
    /// This is the single source of truth for external type conversion.
    fn external_type_to_wgsl(
        &self,
        name: &str,
        kind: &crate::ir::ExternalKind,
    ) -> String {
        use crate::ir::ExternalKind;
        let simple_name = simple_type_name(name);
        let safe_name = to_wgsl_identifier(simple_name);
        match kind {
            ExternalKind::Struct => safe_name,
            ExternalKind::Trait => format!("{}Data", safe_name),
            ExternalKind::Enum => {
                // Check if the enum has data variants - look in imported modules
                // Compare simple names since enum.name may be qualified (e.g., "fill::PatternRepeat")
                // Use max_by_key to prefer the definition with the most variant fields,
                // since re-exported enums in parent modules may have empty field lists
                let has_data = self
                    .imported_modules
                    .values()
                    .flat_map(|m| m.enums.iter())
                    .filter(|e| simple_type_name(&e.name) == simple_name)
                    .max_by_key(|e| {
                        e.variants.iter().map(|v| v.fields.len()).sum::<usize>()
                    })
                    .map(|e| e.variants.iter().any(|v| !v.fields.is_empty()))
                    .unwrap_or(false);
                if has_data {
                    safe_name
                } else {
                    "u32".to_string()
                }
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

    /// Check if an expression is a nil literal.
    fn is_nil_literal(expr: &IrExpr) -> bool {
        matches!(
            expr,
            IrExpr::Literal {
                value: Literal::Nil,
                ..
            }
        )
    }

    /// Generate WGSL for a nil comparison (== nil or != nil).
    ///
    /// Returns Some(wgsl) if one operand is nil, None otherwise.
    /// For nil comparisons, we check the `has_value` field of the Optional wrapper.
    fn gen_nil_comparison(
        &self,
        left: &IrExpr,
        op: &BinaryOperator,
        right: &IrExpr,
        source_module: &IrModule,
    ) -> Option<String> {
        let (non_nil_expr, is_eq) = if Self::is_nil_literal(left) {
            (right, matches!(op, BinaryOperator::Eq))
        } else if Self::is_nil_literal(right) {
            (left, matches!(op, BinaryOperator::Eq))
        } else {
            return None;
        };

        // Generate the expression for the non-nil operand
        let expr_str = self.gen_expr_from_foreign(non_nil_expr, source_module);

        // For `x == nil`, generate `!x.has_value`
        // For `x != nil`, generate `x.has_value`
        if is_eq {
            Some(format!("(!{}.has_value)", expr_str))
        } else {
            Some(format!("{}.has_value", expr_str))
        }
    }

    /// Generate WGSL for a nil comparison using gen_expr (for main module).
    fn gen_nil_comparison_local(
        &self,
        left: &IrExpr,
        op: &BinaryOperator,
        right: &IrExpr,
    ) -> Option<String> {
        let (non_nil_expr, is_eq) = if Self::is_nil_literal(left) {
            (right, matches!(op, BinaryOperator::Eq))
        } else if Self::is_nil_literal(right) {
            (left, matches!(op, BinaryOperator::Eq))
        } else {
            return None;
        };

        // Generate the expression for the non-nil operand
        let expr_str = self.gen_expr(non_nil_expr);

        // For `x == nil`, generate `!x.has_value`
        // For `x != nil`, generate `x.has_value`
        if is_eq {
            Some(format!("(!{}.has_value)", expr_str))
        } else {
            Some(format!("{}.has_value", expr_str))
        }
    }

    /// Generate WGSL for a nil comparison using gen_expr_with_renames.
    fn gen_nil_comparison_with_renames(
        &self,
        left: &IrExpr,
        op: &BinaryOperator,
        right: &IrExpr,
        renames: &HashMap<String, String>,
    ) -> Option<String> {
        let (non_nil_expr, is_eq) = if Self::is_nil_literal(left) {
            (right, matches!(op, BinaryOperator::Eq))
        } else if Self::is_nil_literal(right) {
            (left, matches!(op, BinaryOperator::Eq))
        } else {
            return None;
        };

        // Generate the expression for the non-nil operand (with renames)
        let expr_str = self.gen_expr_with_renames(non_nil_expr, renames);

        // For `x == nil`, generate `!x.has_value`
        // For `x != nil`, generate `x.has_value`
        if is_eq {
            Some(format!("(!{}.has_value)", expr_str))
        } else {
            Some(format!("{}.has_value", expr_str))
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
