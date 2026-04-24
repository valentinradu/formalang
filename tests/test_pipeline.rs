//! Integration tests for the pipeline module.
//!
//! Covers `IrPass`, Backend, Pipeline, and `PipelineError`.

use formalang::error::CompilerError;
use formalang::ir::{
    walk_module, ConstantFoldingPass, DeadCodeEliminationPass, IrModule, IrVisitor, StructId,
};
use formalang::{compile_to_ir, Backend, IrPass, Pipeline, PipelineError};

// =============================================================================
// Backend trait implementations
// =============================================================================

struct StructNameCollector;

impl Backend for StructNameCollector {
    type Output = Vec<String>;
    type Error = std::convert::Infallible;

    fn generate(&self, module: &IrModule) -> Result<Vec<String>, Self::Error> {
        Ok(module.structs.iter().map(|s| s.name.clone()).collect())
    }
}

struct EnumCounter;

impl Backend for EnumCounter {
    type Output = usize;
    type Error = std::convert::Infallible;

    fn generate(&self, module: &IrModule) -> Result<usize, Self::Error> {
        Ok(module.enums.len())
    }
}

#[derive(Debug)]
struct BackendErr(String);

impl std::fmt::Display for BackendErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "backend error: {}", self.0)
    }
}

impl std::error::Error for BackendErr {}

struct FailingBackend;

impl Backend for FailingBackend {
    type Output = String;
    type Error = BackendErr;

    fn generate(&self, _module: &IrModule) -> Result<String, BackendErr> {
        Err(BackendErr("intentional failure".to_string()))
    }
}

// =============================================================================
// IrPass implementations
// =============================================================================

struct KeepPublicStructsPass;

impl IrPass for KeepPublicStructsPass {
    fn name(&self) -> &'static str {
        "keep-public-structs"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        use formalang::ast::Visibility;
        module
            .structs
            .retain(|s| s.visibility == Visibility::Public);
        module.rebuild_indices();
        Ok(module)
    }
}

struct FailingPass;

impl IrPass for FailingPass {
    fn name(&self) -> &'static str {
        "always-fails"
    }

    fn run(&mut self, _module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        Err(vec![CompilerError::ParseError {
            message: "intentional pass failure".to_string(),
            span: formalang::location::Span::default(),
        }])
    }
}

// =============================================================================
// Pipeline::new and Pipeline::default
// =============================================================================

#[test]
fn pipeline_new_creates_empty_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct User { name: String }";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    // Empty pipeline: no passes, just emit
    let names = Pipeline::new()
        .emit(ir, &StructNameCollector)
        .map_err(|e| format!("emit should succeed: {e:?}"))?;
    if names != vec!["User"] {
        return Err(format!("expected {:?} but got {:?}", vec!["User"], names).into());
    }
    Ok(())
}

#[test]
fn pipeline_default_is_same_as_new() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct Foo { x: Number }";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut p = Pipeline::default();
    let result = p
        .run(ir)
        .map_err(|e| format!("run should succeed: {e:?}"))?;
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

// =============================================================================
// Pipeline::run
// =============================================================================

#[test]
fn pipeline_run_returns_transformed_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct Visible { x: Number }
        struct Hidden { y: Number }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut pipeline = Pipeline::new().pass(KeepPublicStructsPass);
    let result = pipeline
        .run(ir)
        .map_err(|e| format!("run should succeed: {e:?}"))?;

    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    let first_struct = result.structs.first().ok_or("index out of bounds")?;
    if first_struct.name != "Visible" {
        return Err(format!("expected {:?} but got {:?}", "Visible", first_struct.name).into());
    }
    Ok(())
}

#[test]
fn pipeline_run_with_multiple_passes_applies_in_order() -> Result<(), Box<dyn std::error::Error>> {
    // Keep Config alive through a standalone function parameter, so
    // DeadCodeEliminationPass (which now removes unused types) does not drop it.
    let source = r"
        pub struct Config { scale: Number = 2 * 3 }
        pub fn use_config(c: Config) -> Number { c.scale }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut pipeline = Pipeline::new()
        .pass(ConstantFoldingPass::new())
        .pass(DeadCodeEliminationPass::new());

    let result = pipeline
        .run(ir)
        .map_err(|e| format!("run should succeed: {e:?}"))?;
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }

    let field = result
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    let default = field.default.as_ref().ok_or("field should have default")?;

    if let formalang::ir::IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = default
    {
        if (n - 6.0_f64).abs().total_cmp(&f64::EPSILON) != std::cmp::Ordering::Less {
            return Err(format!("Expected folded 6.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded literal number, got {default:?}").into());
    }
    Ok(())
}

#[test]
fn pipeline_run_fails_when_pass_fails() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct A { x: Number }";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut pipeline = Pipeline::new().pass(FailingPass);
    let result = pipeline.run(ir);

    if result.is_ok() {
        return Err("Expected pass failure".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if errors.is_empty() {
        return Err("Expected at least one error".into());
    }
    Ok(())
}

// =============================================================================
// Pipeline::emit
// =============================================================================

#[test]
fn pipeline_emit_runs_passes_then_backend() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct Public { name: String }
        struct Private { x: Number }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let result = Pipeline::new()
        .pass(KeepPublicStructsPass)
        .emit(ir, &StructNameCollector)
        .map_err(|e| format!("emit should succeed: {e:?}"))?;

    if result != vec!["Public"] {
        return Err(format!("expected {:?} but got {:?}", vec!["Public"], result).into());
    }
    Ok(())
}

#[test]
fn pipeline_emit_with_no_passes_goes_straight_to_backend() -> Result<(), Box<dyn std::error::Error>>
{
    let source = r"
        pub enum Status { active, inactive }
        pub enum Color { red, green, blue }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let count = Pipeline::new()
        .emit(ir, &EnumCounter)
        .map_err(|e| format!("emit should succeed: {e:?}"))?;
    if count != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, count).into());
    }
    Ok(())
}

#[test]
fn pipeline_emit_wraps_pass_errors_as_pipeline_pass_errors(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct A { x: Number }";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let result = Pipeline::new()
        .pass(FailingPass)
        .emit(ir, &StructNameCollector);

    match result {
        Err(PipelineError::PassErrors(errors)) => {
            if errors.is_empty() {
                return Err("Expected compiler errors from failing pass".into());
            }
        }
        other => return Err(format!("Expected PipelineError::PassErrors, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn pipeline_emit_wraps_backend_errors_as_pipeline_backend_error(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct A { x: Number }";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let result = Pipeline::new().emit(ir, &FailingBackend);

    match result {
        Err(PipelineError::BackendError(e)) => {
            if !e.0.contains("intentional") {
                return Err(format!("Expected intentional error message, got {}", e.0).into());
            }
        }
        other => return Err(format!("Expected PipelineError::BackendError, got {other:?}").into()),
    }
    Ok(())
}

// =============================================================================
// PipelineError display and source
// =============================================================================

#[test]
fn pipeline_error_pass_errors_display() -> Result<(), Box<dyn std::error::Error>> {
    let err: PipelineError<BackendErr> =
        PipelineError::PassErrors(vec![CompilerError::ParseError {
            message: "display test".to_string(),
            span: formalang::location::Span::default(),
        }]);

    let display = format!("{err}");
    if !display.contains("display test") {
        return Err(format!("Display should contain error message, got: {display}").into());
    }
    Ok(())
}

#[test]
fn pipeline_error_backend_error_display() -> Result<(), Box<dyn std::error::Error>> {
    let err: PipelineError<BackendErr> =
        PipelineError::BackendError(BackendErr("oops".to_string()));
    let display = format!("{err}");
    if !display.contains("oops") {
        return Err(format!("Display should contain backend error, got: {display}").into());
    }
    Ok(())
}

#[test]
fn pipeline_error_backend_error_source_is_some() -> Result<(), Box<dyn std::error::Error>> {
    use std::error::Error;
    let err: PipelineError<BackendErr> = PipelineError::BackendError(BackendErr("src".to_string()));
    if err.source().is_none() {
        return Err("BackendError should expose its source".into());
    }
    Ok(())
}

#[test]
fn pipeline_error_pass_errors_source_is_none() -> Result<(), Box<dyn std::error::Error>> {
    use std::error::Error;
    let err: PipelineError<BackendErr> = PipelineError::PassErrors(vec![]);
    if err.source().is_some() {
        return Err("PassErrors should have no error source".into());
    }
    Ok(())
}

// =============================================================================
// IrPass trait: name() and run() coverage
// =============================================================================

#[test]
fn irpass_name_returns_correct_string() -> Result<(), Box<dyn std::error::Error>> {
    let pass = KeepPublicStructsPass;
    if pass.name() != "keep-public-structs" {
        return Err(format!("expected 'keep-public-structs', got '{}'", pass.name()).into());
    }
    Ok(())
}

#[test]
fn irpass_dce_name() -> Result<(), Box<dyn std::error::Error>> {
    let pass = DeadCodeEliminationPass::new();
    if pass.name() != "dead-code-elimination" {
        return Err(format!("expected 'dead-code-elimination', got '{}'", pass.name()).into());
    }
    Ok(())
}

#[test]
fn irpass_fold_name() -> Result<(), Box<dyn std::error::Error>> {
    let pass = ConstantFoldingPass::new();
    if pass.name() != "constant-folding" {
        return Err(format!("expected 'constant-folding', got '{}'", pass.name()).into());
    }
    Ok(())
}

// =============================================================================
// Pipeline chaining and rebuild_indices
// =============================================================================

#[test]
fn pipeline_pass_chaining_returns_self() -> Result<(), Box<dyn std::error::Error>> {
    // Verify pass() builder pattern works correctly with multiple passes
    let source = r"
        pub struct A { x: Number = 1 + 1 }
        pub struct B { y: Boolean = true && false }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let count = Pipeline::new()
        .pass(ConstantFoldingPass::new())
        .pass(DeadCodeEliminationPass::new())
        .emit(ir, &EnumCounter) // 0 enums, but proves chaining compiles and runs
        .map_err(|e| format!("emit should succeed: {e:?}"))?;
    if count != 0 {
        return Err(format!("expected {:?} but got {:?}", 0, count).into());
    }
    Ok(())
}

#[test]
fn irpass_rebuild_indices_keeps_lookups_consistent() -> Result<(), Box<dyn std::error::Error>> {
    // After filtering structs, rebuild_indices should keep name lookups correct
    let source = r"
        pub struct Keep { x: Number }
        struct Drop { y: Number }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut pipeline = Pipeline::new().pass(KeepPublicStructsPass);
    let result = pipeline
        .run(ir)
        .map_err(|e| format!("run should succeed: {e:?}"))?;

    // Name lookup should still work after rebuild
    let id = result
        .struct_id("Keep")
        .ok_or("Keep should be found by name after rebuild")?;
    let keep_name = &result.get_struct(id).ok_or("struct not found")?.name;
    if keep_name != "Keep" {
        return Err(format!("expected {:?} but got {:?}", "Keep", keep_name).into());
    }

    // Dropped struct must not be found
    if result.struct_id("Drop").is_some() {
        return Err("Dropped struct should not be findable after rebuild".into());
    }
    Ok(())
}

// =============================================================================
// Backend and Pipeline doc-example patterns
// =============================================================================

#[test]
fn backend_generate_called_with_correct_module() -> Result<(), Box<dyn std::error::Error>> {
    // The backend should see the module exactly as the pipeline produced it
    struct FieldCounter;

    impl Backend for FieldCounter {
        type Output = usize;
        type Error = std::convert::Infallible;

        fn generate(&self, module: &IrModule) -> Result<usize, Self::Error> {
            Ok(module.structs.iter().map(|s| s.fields.len()).sum())
        }
    }

    let source = r"
        struct A { x: Number, y: Number }
        struct B { z: String }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let total = Pipeline::new()
        .emit(ir, &FieldCounter)
        .map_err(|e| format!("emit should succeed: {e:?}"))?;
    if total != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, total).into());
    }
    Ok(())
}

#[test]
fn pipeline_emit_with_visitor_backend() -> Result<(), Box<dyn std::error::Error>> {
    // Use IrVisitor inside a backend to count structs and traits
    struct TypeCountBackend;

    impl Backend for TypeCountBackend {
        type Output = (usize, usize);
        type Error = std::convert::Infallible;

        fn generate(&self, module: &IrModule) -> Result<(usize, usize), Self::Error> {
            use formalang::ir::{EnumId, IrEnum, IrStruct, TraitId};

            struct Counter {
                structs: usize,
                traits: usize,
            }
            impl IrVisitor for Counter {
                fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
                    self.structs += 1;
                }
                fn visit_trait(&mut self, _id: TraitId, _t: &formalang::ir::IrTrait) {
                    self.traits += 1;
                }
                fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {}
            }

            let mut counter = Counter {
                structs: 0,
                traits: 0,
            };
            walk_module(&mut counter, module);
            Ok((counter.structs, counter.traits))
        }
    }

    let source = r"
        trait Named { name: String }
        struct User { name: String }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let (struct_count, trait_count) = Pipeline::new()
        .emit(ir, &TypeCountBackend)
        .map_err(|e| format!("emit should succeed: {e:?}"))?;
    if struct_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, struct_count).into());
    }
    if trait_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, trait_count).into());
    }
    Ok(())
}

// =============================================================================
// Monomorphisation pass
// =============================================================================

#[test]
fn test_monomorphise_removes_unused_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    // A defined-but-never-instantiated generic has no specialisations to
    // produce, so it is simply dropped from the module.
    let source = r"
        pub struct Box<T> { value: T }
    ";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass);
    let result = pipeline
        .run(module)
        .map_err(|e| format!("monomorphise should accept an uninstantiated generic, got: {e:?}"))?;
    if result.structs.iter().any(|s| s.name == "Box") {
        return Err("generic `Box<T>` should have been removed after monomorphisation".into());
    }
    Ok(())
}

#[test]
fn test_monomorphise_specialises_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    let source = r"
        pub struct Box<T> { value: T }
        pub let b: Box<Number> = Box<Number>(value: 1)
    ";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass);
    let result = pipeline
        .run(module)
        .map_err(|e| format!("monomorphise should specialise Box<Number>, got: {e:?}"))?;
    // The original `Box<T>` generic definition should be gone.
    if result.structs.iter().any(|s| s.name == "Box") {
        return Err(
            "generic definition `Box` should have been replaced by a specialised clone".into(),
        );
    }
    // A specialised clone whose name starts with `Box__` should exist.
    if !result.structs.iter().any(|s| s.name.starts_with("Box__")) {
        return Err(format!(
            "expected a `Box__...` specialisation, got structs: {:?}",
            result
                .structs
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_monomorphise_accepts_concrete_module() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    let source = r"
        pub struct User { name: String, age: Number }
        pub fn greet(user: User) -> String { user.name }
    ";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass);
    pipeline
        .run(module)
        .map_err(|e| format!("expected pass to accept concrete module, got: {e:?}"))?;
    Ok(())
}
