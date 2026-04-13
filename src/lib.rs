pub mod ast;
pub mod builtins;
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
    simple_type_name, EnumId, ExternalKind, FunctionId, IrFunction, IrFunctionParam, IrImport,
    IrImportItem, IrModule, ResolvedType, StructId, TraitId,
};
pub use lexer::{Lexer, Token};
pub use location::{Location, Span};
pub use parser::{parse_file, parse_file_with_source};
pub use pipeline::{Backend, IrPass, Pipeline, PipelineError};
pub use reporting::{report_error, report_errors};
pub use semantic::module_resolver::FileSystemResolver;
pub use semantic::SemanticAnalyzer;

/// Compile FormaLang source code into a validated AST.
///
/// # Pipeline
///
/// 1. **Lexer**: Tokenizes the source code
/// 2. **Parser**: Builds an Abstract Syntax Tree (AST)
/// 3. **Semantic Analyzer**: Validates the AST (6 passes)
///    - Pass 0: Module resolution (use statements, imports)
///    - Pass 1: Symbol table building (all definitions)
///    - Pass 2: Type resolution (type references)
///    - Pass 3: Expression validation (operators, control flow)
///    - Pass 4: Trait validation (implementations)
///    - Pass 5: Circular dependency detection
///
/// # Returns
///
/// * `Ok(File)` - The validated AST
/// * `Err(Vec<CompilerError>)` - Compilation errors
///
/// # Example
///
/// ```
/// use formalang::compile;
///
/// let source = r#"
/// pub struct User {
///     name: String,
///     age: Number
/// }
/// "#;
///
/// match compile(source) {
///     Ok(_file) => println!("OK"),
///     Err(errors) => {
///         for e in errors { eprintln!("{e}"); }
///     }
/// }
/// ```
pub fn compile(source: &str) -> Result<File, Vec<CompilerError>> {
    compile_with_resolver(
        source,
        FileSystemResolver::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
    )
}

/// Compile FormaLang source code with a custom module resolver.
///
/// # Example
///
/// ```
/// use formalang::{compile_with_resolver, FileSystemResolver};
/// use std::path::PathBuf;
///
/// let resolver = FileSystemResolver::new(PathBuf::from("."));
/// let source = "pub struct Point { x: Number, y: Number }";
/// let _file = compile_with_resolver(source, resolver).unwrap();
/// ```
pub fn compile_with_resolver<R>(source: &str, resolver: R) -> Result<File, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let tokens = Lexer::tokenize_all(source);
    let mut file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    let mut analyzer = SemanticAnalyzer::new(resolver);
    analyzer.analyze_and_classify(&mut file)?;
    Ok(file)
}

/// Compile and return both the AST and the semantic analyzer.
///
/// Useful for LSP implementations that need access to the symbol table for
/// completion, hover, and go-to-definition.
pub fn compile_with_analyzer(
    source: &str,
) -> Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>> {
    compile_with_analyzer_and_resolver(
        source,
        FileSystemResolver::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
    )
}

/// Compile with a custom resolver, returning both AST and analyzer.
pub fn compile_with_analyzer_and_resolver<R>(
    source: &str,
    resolver: R,
) -> Result<(File, SemanticAnalyzer<R>), Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let tokens = Lexer::tokenize_all(source);
    let mut file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    let mut analyzer = SemanticAnalyzer::new(resolver);
    analyzer.analyze_and_classify(&mut file)?;
    Ok((file, analyzer))
}

/// Compile and format errors for display.
///
/// # Example
///
/// ```no_run
/// use formalang::compile_and_report;
///
/// let source = std::fs::read_to_string("example.fv").unwrap();
/// match compile_and_report(&source, "example.fv") {
///     Ok(_file) => println!("OK"),
///     Err(report) => eprintln!("{report}"),
/// }
/// ```
pub fn compile_and_report(source: &str, filename: &str) -> Result<File, String> {
    compile(source).map_err(|errors| report_errors(&errors, source, filename))
}

/// Parse FormaLang source without semantic analysis.
///
/// Performs only lexing and parsing. Useful for syntax checking or raw AST
/// inspection.
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
    let tokens = Lexer::tokenize_all(source);
    let file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    Ok(file)
}

/// Compile FormaLang source code into an IR module.
///
/// This is the recommended entry point for code generators. The IR provides
/// resolved types, ID-based references, and a flat structure optimised for
/// traversal and emission.
///
/// Attach a [`Backend`] via [`Pipeline`] to emit code from the returned module.
///
/// # Example
///
/// ```
/// use formalang::compile_to_ir;
///
/// let source = r#"
/// pub struct User {
///     name: String,
///     age: Number
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

/// Compile FormaLang source code to IR with a custom module resolver.
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
