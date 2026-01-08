pub mod ast;
pub mod builtins;
pub mod codegen;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod location;
pub mod parser;
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
pub use reporting::{report_error, report_errors};
pub use semantic::module_resolver::FileSystemResolver;
pub use semantic::SemanticAnalyzer;

/// Compile FormaLang source code into a validated AST
///
/// # Pipeline
///
/// 1. **Lexer**: Tokenizes the source code
/// 2. **Parser**: Builds an Abstract Syntax Tree (AST)
/// 3. **Semantic Analyzer**: Validates the AST (5 passes)
///    - Pass 0: Module resolution (use statements, imports)
///    - Pass 1: Symbol table building (all definitions)
///    - Pass 2: Type resolution (type references)
///    - Pass 3: Expression validation (operators, control flow)
///    - Pass 4: Trait validation (implementations)
///    - Pass 5: Circular dependency detection
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
///
/// # Returns
///
/// * `Ok(File)` - The validated AST if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
///
/// # Example
///
/// ```
/// use formalang::compile;
///
/// let source = r#"
/// pub model User(
///   name: String,
///   age: Number
/// )
/// "#;
///
/// match compile(source) {
///     Ok(file) => println!("Compilation successful!"),
///     Err(errors) => {
///         for error in errors {
///             eprintln!("Error: {}", error);
///         }
///     }
/// }
/// ```
pub fn compile(source: &str) -> Result<File, Vec<CompilerError>> {
    compile_with_resolver(
        source,
        FileSystemResolver::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
    )
}

/// Compile FormaLang source code with a custom module resolver
///
/// This function allows you to provide a custom module resolver for testing
/// or non-filesystem-based module resolution.
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
/// * `resolver` - A module resolver implementing the ModuleResolver trait
///
/// # Returns
///
/// * `Ok(File)` - The validated AST if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
pub fn compile_with_resolver<R>(source: &str, resolver: R) -> Result<File, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    // Phase 1: Lex
    let tokens = Lexer::tokenize_all(source);

    // Phase 2: Parse (with source text for accurate error positions)
    let mut file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;

    // Phase 3: Semantic analysis (validation only)
    let mut analyzer = SemanticAnalyzer::new(resolver);
    analyzer.analyze_and_classify(&mut file)?;

    Ok(file)
}

/// Compile FormaLang source code and return both the AST and the semantic analyzer
///
/// This function is useful for LSP implementations that need access to the symbol
/// table for features like completion, hover, and go-to-definition.
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
///
/// # Returns
///
/// * `Ok((File, SemanticAnalyzer))` - The validated AST and analyzer if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
pub fn compile_with_analyzer(
    source: &str,
) -> Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>> {
    compile_with_analyzer_and_resolver(
        source,
        FileSystemResolver::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
    )
}

/// Compile FormaLang source code with a custom resolver, returning both AST and analyzer
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
/// * `resolver` - A module resolver implementing the ModuleResolver trait
///
/// # Returns
///
/// * `Ok((File, SemanticAnalyzer))` - The validated AST and analyzer if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
pub fn compile_with_analyzer_and_resolver<R>(
    source: &str,
    resolver: R,
) -> Result<(File, SemanticAnalyzer<R>), Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    // Phase 1: Lex
    let tokens = Lexer::tokenize_all(source);

    // Phase 2: Parse (with source text for accurate error positions)
    let mut file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;

    // Phase 3: Semantic analysis (validation only)
    let mut analyzer = SemanticAnalyzer::new(resolver);
    analyzer.analyze_and_classify(&mut file)?;

    Ok((file, analyzer))
}

/// Compile and report errors with beautiful formatting
///
/// This is a convenience function that compiles the source code and,
/// if there are errors, formats them using ariadne for display.
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
/// * `filename` - The filename to use in error reports
///
/// # Returns
///
/// * `Ok(File)` - The validated AST if compilation succeeds
/// * `Err(String)` - A formatted error report if compilation fails
///
/// # Example
///
/// ```no_run
/// use formalang::compile_and_report;
///
/// let source = std::fs::read_to_string("example.fv").unwrap();
///
/// match compile_and_report(&source, "example.fv") {
///     Ok(file) => println!("Compilation successful!"),
///     Err(report) => eprintln!("{}", report),
/// }
/// ```
pub fn compile_and_report(source: &str, filename: &str) -> Result<File, String> {
    compile(source).map_err(|errors| report_errors(&errors, source, filename))
}

/// Parse FormaLang source code without semantic analysis
///
/// This function performs only lexing and parsing, without running semantic validation.
/// Useful for syntax checking or when you want to inspect the raw AST.
///
/// # Arguments
///
/// * `source` - The FormaLang source code to parse
///
/// # Returns
///
/// * `Ok(File)` - The unvalidated AST if parsing succeeds
/// * `Err(Vec<CompilerError>)` - A list of parse errors if parsing fails
///
/// # Example
///
/// ```
/// use formalang::parse_only;
///
/// let source = r#"
/// pub model User(
///   name: String,
///   age: Number
/// )
/// "#;
///
/// match parse_only(source) {
///     Ok(file) => println!("Parsing successful!"),
///     Err(errors) => {
///         for error in errors {
///             eprintln!("Parse error: {}", error);
///         }
///     }
/// }
/// ```
pub fn parse_only(source: &str) -> Result<File, Vec<CompilerError>> {
    // Phase 1: Lex
    let tokens = Lexer::tokenize_all(source);

    // Phase 2: Parse (with source text for accurate error positions)
    let file = parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;

    Ok(file)
}

/// Compile FormaLang source code into an IR module
///
/// This is the recommended entry point for code generators. The IR provides
/// resolved types, linked references, and is optimized for generating
/// TypeScript, Swift, and Kotlin code.
///
/// # Pipeline
///
/// 1. **Lexer**: Tokenizes the source code
/// 2. **Parser**: Builds an Abstract Syntax Tree (AST)
/// 3. **Semantic Analyzer**: Validates the AST
/// 4. **IR Lowering**: Converts AST to IR with resolved types
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
///
/// # Returns
///
/// * `Ok(IrModule)` - The IR module if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
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
/// match compile_to_ir(source) {
///     Ok(module) => {
///         assert_eq!(module.structs.len(), 1);
///         assert_eq!(module.structs[0].name, "User");
///     }
///     Err(errors) => {
///         for error in errors {
///             eprintln!("Error: {}", error);
///         }
///     }
/// }
/// ```
pub fn compile_to_ir(source: &str) -> Result<IrModule, Vec<CompilerError>> {
    let (ast, analyzer) = compile_with_analyzer(source)?;
    ir::lower_to_ir(&ast, analyzer.symbols())
}

/// Compile FormaLang source code to IR with a custom module resolver
///
/// # Arguments
///
/// * `source` - The FormaLang source code to compile
/// * `resolver` - A module resolver implementing the ModuleResolver trait
///
/// # Returns
///
/// * `Ok(IrModule)` - The IR module if compilation succeeds
/// * `Err(Vec<CompilerError>)` - A list of compilation errors if compilation fails
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
