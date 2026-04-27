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
    let mut analyzer =
        SemanticAnalyzer::new_with_file(resolver, std::path::PathBuf::from("<root>"));
    analyzer.analyze_and_classify(&mut file)?;
    Ok((file, analyzer))
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

/// Compile `FormaLang` source code to IR with a custom module resolver.
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation or IR lowering fails.
pub fn compile_to_ir_with_resolver<R>(
    source: &str,
    resolver: R,
) -> Result<IrModule, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let (ast, analyzer) = compile_with_analyzer_and_resolver(source, resolver)?;
    ir::lower_to_ir(&ast, analyzer.symbols())
}
