//! # `FormaLang`
//!
//! A compiler frontend library for the `FormaLang` declarative language.
//! Parsing, semantic analysis, and IR lowering are built-in; code generation
//! is the responsibility of embedders via the plugin system.
//!
//! ## Entry points
//!
//! - [`compile_to_ir`] — compile source to a resolved [`IrModule`].
//! - [`compile_to_ir_with_resolver`] — same, with a custom [`semantic::module_resolver::ModuleResolver`].
//! - [`compile_with_analyzer`] — returns the AST plus [`SemanticAnalyzer`] for LSP-style use.
//! - [`parse_only`] — lex + parse without semantic analysis.
//! - [`compile_and_report`] — convenience wrapper that formats errors as a
//!   human-readable report.
//!
//! ## Plugin system
//!
//! Embedders compose [`IrPass`] transforms and a [`Backend`] via [`Pipeline`].
//! Built-in passes live in [`ir::DeadCodeEliminationPass`] and
//! [`ir::ConstantFoldingPass`].

pub mod ast;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod location;
pub mod parser;
pub mod pipeline;
pub mod reporting;
pub mod semantic;

/// Compiler-shipped prelude source. Contains `extern impl <Primitive>`
/// declarations for the built-in method surface (e.g., `String::len`,
/// `String::slice`). Prepended to every user source at the entry-point
/// compile functions so its declarations are visible without an
/// explicit `use`.
pub(crate) const PRELUDE_SOURCE: &str = include_str!("prelude.fv");

// Re-export commonly used types
pub use ast::{Definition, Expr, File, Ident, Statement, Type};
pub use error::CompilerError;
pub use ir::{
    simple_type_name, EnumId, FunctionId, GenericBase, ImportedKind, IrFunction, IrFunctionParam,
    IrFunctionSig, IrImport, IrImportItem, IrModule, ResolvedType, StructId, TraitId,
};
pub use lexer::{Lexer, Token};
pub use location::{Location, Span};
pub use parser::{parse_file, parse_file_with_source};
pub use pipeline::{Backend, IrPass, Pipeline, PipelineError};
pub use reporting::{report_error, report_errors};
pub use semantic::module_resolver::FileSystemResolver;
pub use semantic::SemanticAnalyzer;

/// Compile and return both the AST and the semantic analyzer.
///
/// Useful for LSP implementations that need access to the symbol table for
/// completion, hover, and go-to-definition.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if lexing, parsing, or semantic analysis fails.
pub fn compile_with_analyzer(
    source: &str,
) -> Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>> {
    compile_with_analyzer_and_resolver(
        source,
        FileSystemResolver::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
    )
}

/// Compile with a custom resolver, returning both AST and analyzer.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if lexing, parsing, or semantic analysis fails.
pub fn compile_with_analyzer_and_resolver<R>(
    source: &str,
    resolver: R,
) -> Result<(File, SemanticAnalyzer<R>), Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    // Parse the user source first — its spans stay 0-based on the
    // user's bytes, so error messages and IDE tooling report the
    // correct line/column.
    let (tokens, lex_errors) = Lexer::tokenize_all_with_errors(source);
    let parse_result = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    });
    let mut file = match parse_result {
        Ok(f) if lex_errors.is_empty() => f,
        Ok(_) => return Err(lex_errors),
        Err(mut parse_errors) => {
            let mut all = lex_errors;
            all.append(&mut parse_errors);
            return Err(all);
        }
    };

    // Parse the compiler-shipped prelude separately, then prepend its
    // top-level statements to the user file. User-source spans are
    // preserved; only prelude statements carry prelude-relative spans
    // (which the lowerer flags via FileId::SYNTHETIC if/when file-id
    // wiring is enabled).
    let prelude_file = parse_prelude_file()?;
    let mut merged_statements = prelude_file.statements;
    merged_statements.append(&mut file.statements);
    file.statements = merged_statements;

    let mut analyzer =
        SemanticAnalyzer::new_with_file(resolver, std::path::PathBuf::from("<root>"));
    analyzer.analyze_and_classify(&mut file)?;
    Ok((file, analyzer))
}

/// Parse the compiler-shipped prelude (`src/prelude.fv`) into a `File`
/// AST. The prelude is fixed source so its parse should always
/// succeed; surface any unexpected failure as `CompilerError`.
fn parse_prelude_file() -> Result<File, Vec<CompilerError>> {
    let (tokens, lex_errors) = Lexer::tokenize_all_with_errors(PRELUDE_SOURCE);
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    parse_file_with_source(&tokens, PRELUDE_SOURCE).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })
}

/// Compile to IR, formatting errors as a human-readable report on failure.
///
/// # Errors
///
/// Returns a formatted error string if compilation or IR lowering fails.
///
/// # Example
///
/// ```no_run
/// use formalang::compile_and_report;
///
/// let source = std::fs::read_to_string("example.fv").unwrap();
/// match compile_and_report(&source, "example.fv") {
///     Ok(_module) => println!("OK"),
///     Err(report) => eprintln!("{report}"),
/// }
/// ```
pub fn compile_and_report(source: &str, filename: &str) -> Result<IrModule, String> {
    compile_to_ir(source).map_err(|errors| report_errors(&errors, source, filename))
}

/// Parse `FormaLang` source without semantic analysis.
///
/// Performs only lexing and parsing. Useful for syntax checking or raw AST
/// inspection.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if lexing or parsing fails.
///
/// # Example
///
/// ```
/// use formalang::parse_only;
///
/// let source = "pub struct User { name: String }";
/// let _file = parse_only(source).unwrap();
/// ```
pub fn parse_only(source: &str) -> Result<File, Vec<CompilerError>> {
    let (tokens, lex_errors) = Lexer::tokenize_all_with_errors(source);
    let parse_result = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    });
    match parse_result {
        Ok(f) if lex_errors.is_empty() => Ok(f),
        Ok(_) => Err(lex_errors),
        Err(mut parse_errors) => {
            let mut all = lex_errors;
            all.append(&mut parse_errors);
            Err(all)
        }
    }
}

/// Compile `FormaLang` source code into an IR module.
///
/// This is the recommended entry point for code generators. The IR provides
/// resolved types, ID-based references, and a flat structure optimised for
/// traversal and emission.
///
/// Attach a [`Backend`] via [`Pipeline`] to emit code from the returned module.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation or IR lowering fails.
///
/// # Example
///
/// ```
/// use formalang::compile_to_ir;
///
/// let source = r#"
/// pub struct User {
///     name: String,
///     age: I32
/// }
/// "#;
///
/// let module = compile_to_ir(source).unwrap();
/// assert_eq!(module.structs.len(), 1);
/// assert_eq!(module.structs[0].name, "User");
/// ```
pub fn compile_to_ir(source: &str) -> Result<IrModule, Vec<CompilerError>> {
    let (ast, analyzer) = compile_with_analyzer(source)?;
    ir::lower_to_ir(&ast, analyzer.symbols())
}

/// Compile `FormaLang` source to IR with a known source-file path.
///
/// The path is registered in `IrModule.file_table` and threaded into
/// every lowered IR node's `IrSpan.file`. Use this entry point when
/// emitting DWARF / source maps / line tables — backends resolve
/// every span to a real path via `IrModule.file_path(span.file)`.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation or IR lowering fails.
pub fn compile_to_ir_with_path(
    source: &str,
    path: std::path::PathBuf,
) -> Result<IrModule, Vec<CompilerError>> {
    let (ast, analyzer) = compile_with_analyzer(source)?;
    ir::lower_to_ir_with_path(&ast, analyzer.symbols(), path)
}

/// Compile `FormaLang` source code to IR with a custom module resolver.
///
/// Runs [`ir::MonomorphisePass`] after lowering with an `imports_map` built
/// from the analyzer's per-import IR cache, so generic `External` references
/// to imported types are specialised into local clones before the IR is
/// returned. Non-generic `External` references stay opaque until the
/// cross-module inline pass extends to them (see
/// `plans/cross-module-codegen.md`).
///
/// Single-file consumers should prefer [`compile_to_ir`] — that path skips
/// the pipeline since there are no imports to inline.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation, IR lowering, or
/// monomorphisation fails.
pub fn compile_to_ir_with_resolver<R>(
    source: &str,
    resolver: R,
) -> Result<IrModule, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let (ast, analyzer) = compile_with_analyzer_and_resolver(source, resolver)?;
    let module = ir::lower_to_ir(&ast, analyzer.symbols())?;

    // Build the imports map keyed by logical module path (matching
    // `ResolvedType::External::module_path`). Each entry pairs a path with
    // the cached IR of the module that path resolves to. Driven off the
    // entry-point module's `imports[*]`: only modules that actually
    // contributed at least one symbol are forwarded to the pass.
    let imported_ir = analyzer.imported_ir_modules();
    let mut imports_map: std::collections::HashMap<Vec<String>, IrModule> =
        std::collections::HashMap::with_capacity(module.imports.len());
    for imp in &module.imports {
        if let Some(ir_mod) = imported_ir.get(&imp.source_file) {
            imports_map.insert(imp.module_path.clone(), ir_mod.clone());
        }
    }

    Pipeline::new()
        .pass(ir::MonomorphisePass::default().with_imports(imports_map))
        .run(module)
}

/// Compile `FormaLang` source to IR with both a custom resolver and
/// a known source-file path. Combines the contracts of
/// [`compile_to_ir_with_resolver`] (cross-module imports via the
/// resolver) and [`compile_to_ir_with_path`] (file_table seeded with
/// the entry-point path so spans carry a real `FileId`).
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation or IR lowering fails.
pub fn compile_to_ir_with_path_and_resolver<R>(
    source: &str,
    path: std::path::PathBuf,
    resolver: R,
) -> Result<IrModule, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let (ast, analyzer) = compile_with_analyzer_and_resolver(source, resolver)?;
    ir::lower_to_ir_with_path(&ast, analyzer.symbols(), path)
}
