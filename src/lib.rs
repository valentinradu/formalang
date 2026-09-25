//! # `FormaLang`
//!
//! A compiler frontend library for the `FormaLang` declarative language.
//! Parsing, semantic analysis, and IR lowering are built-in; code generation
//! is the responsibility of embedders via the plugin system.
//!
//! ## Entry points
//!
//! - [`compile_to_ir`]: compile source to a resolved [`IrModule`].
//! - [`compile_to_ir_with_path`]: the same, with the source path in the
//!   file table of the module.
//! - [`compile_to_ir_with_resolver`]: compile a program of several
//!   modules through a [`semantic::module_resolver::ModuleResolver`],
//!   then run [`ir::MonomorphisePass`].
//! - [`compile_to_ir_with_path_and_resolver`]: the same as
//!   [`compile_to_ir_with_resolver`], with the source path. It also runs
//!   [`ir::MonomorphisePass`].
//! - [`compile_with_analyzer`] and [`compile_with_analyzer_and_resolver`]:
//!   the AST and the [`SemanticAnalyzer`], for LSP-style use.
//! - [`parse_only`]: lex and parse, with no semantic analysis.
//! - [`compile_and_report`]: [`compile_to_ir`], with the errors as a
//!   report that a person can read.
//!
//! ## Plugin system
//!
//! Embedders compose [`IrPass`] transforms and a [`Backend`] via [`Pipeline`].
//! The built-in passes are in [`ir`]: [`ir::MonomorphisePass`],
//! [`ir::ResolveReferencesPass`], [`ir::ClosureConversionPass`],
//! [`ir::DefunctionalisePass`], [`ir::DeadCodeEliminationPass`] and
//! [`ir::ConstantFoldingPass`].
//!
//! ## Cargo features
//!
//! - `serde` (off by default): derive `serde::Serialize` and
//!   `serde::Deserialize` on [`IrModule`] and on every type in it. The
//!   JSON form of the IR is not a stable format. The AST has no
//!   serialized form.

pub mod ast;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod location;
pub mod parser;
pub mod pipeline;
pub mod reporting;
pub mod semantic;

/// Compiler-shipped prelude source. It declares the built-in generic
/// types (`Optional`, `Array`, `Seq`, `Dictionary`, `Range`), their
/// methods and the methods of `String` as `extern impl` blocks, and the
/// `assert` function. The compiler puts its statements before the
/// statements of each module, the entry module and each imported
/// module, so its names need no `use`.
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
    // The parse and the analysis are two functions. In a debug build a
    // frame holds every local of its function from the start, so an
    // analyzer declared here would sit on the stack under the parser,
    // which recurses once for each level of nesting. The user source
    // keeps 0-based spans on its own bytes.
    let file = parse_only(source)?;
    analyze_with_prelude(file, resolver)
}

/// Prepend the prelude to `file` and run the semantic analysis.
fn analyze_with_prelude<R>(
    mut file: File,
    resolver: R,
) -> Result<(File, SemanticAnalyzer<R>), Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
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

/// The parsed prelude, computed once per process.
///
/// [`PRELUDE_SOURCE`] is fixed text, so its AST is fixed too. Every
/// entry point used to re-lex and re-parse it, and that dominated the
/// cost of compiling a small program.
static PRELUDE_AST: std::sync::OnceLock<Result<File, Vec<CompilerError>>> =
    std::sync::OnceLock::new();

/// Parse the compiler-shipped prelude (`src/prelude.fv`) into a `File`
/// AST. The prelude is fixed source so its parse should always
/// succeed; surface any unexpected failure as `CompilerError`.
///
/// The result is cached, so the parse runs once per process and every
/// later call clones it. Cloning an AST costs far less than parsing
/// one, and the caller needs its own copy: it appends the user's
/// statements to the prelude's.
pub(crate) fn parse_prelude_file() -> Result<File, Vec<CompilerError>> {
    PRELUDE_AST.get_or_init(parse_prelude_file_uncached).clone()
}

/// The number of statements in the prelude. The prelude's statements
/// come first in the AST of each module.
fn prelude_len() -> Result<usize, Vec<CompilerError>> {
    PRELUDE_AST
        .get_or_init(parse_prelude_file_uncached)
        .as_ref()
        .map(|file| file.statements.len())
        .map_err(Clone::clone)
}

/// The names of the prelude's definitions, computed once per process.
static PRELUDE_NAMES: std::sync::OnceLock<std::collections::HashSet<String>> =
    std::sync::OnceLock::new();

/// The names that the prelude defines. Each module has the prelude, so
/// a module cannot import one of these names from another module.
pub(crate) fn prelude_names() -> &'static std::collections::HashSet<String> {
    PRELUDE_NAMES.get_or_init(|| {
        let Ok(file) = PRELUDE_AST.get_or_init(parse_prelude_file_uncached) else {
            return std::collections::HashSet::new();
        };
        file.statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Definition(def) => match &**def {
                    Definition::Trait(t) => Some(t.name.name.clone()),
                    Definition::Struct(s) => Some(s.name.name.clone()),
                    Definition::Enum(e) => Some(e.name.name.clone()),
                    Definition::Module(m) => Some(m.name.name.clone()),
                    Definition::Function(f) => Some(f.name.name.clone()),
                    Definition::Impl(_) => None,
                },
                Statement::Use(_) | Statement::Let(_) => None,
            })
            .collect()
    })
}

/// The lowered prelude, computed once per process.
static PRELUDE_IR: std::sync::OnceLock<Result<IrModule, Vec<CompilerError>>> =
    std::sync::OnceLock::new();

/// The prelude, analysed and lowered on its own. The lowering of each
/// module starts from a copy of it; see [`ir::link`].
pub(crate) fn prelude_ir() -> Result<&'static IrModule, Vec<CompilerError>> {
    PRELUDE_IR
        .get_or_init(|| {
            let mut file = parse_prelude_file()?;
            let mut analyzer = SemanticAnalyzer::new_with_file(
                FileSystemResolver::new(".".into()),
                "<prelude>".into(),
            );
            analyzer.analyze_and_classify(&mut file)?;
            ir::lower_to_ir(&file, analyzer.symbols())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Parse the prelude without consulting the cache.
fn parse_prelude_file_uncached() -> Result<File, Vec<CompilerError>> {
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
/// assert_eq!(module.user_structs().count(), 1);
/// let user = module.user_structs().next().unwrap();
/// assert_eq!(user.name, "User");
/// ```
pub fn compile_to_ir(source: &str) -> Result<IrModule, Vec<CompilerError>> {
    let (ast, analyzer) = compile_with_analyzer(source)?;
    analyzer.lower_entry(&ast, prelude_len()?, None)
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
    analyzer.lower_entry(&ast, prelude_len()?, Some(path))
}

/// Compile `FormaLang` source code to IR with a custom module resolver.
///
/// Each imported module lowers on top of the modules that it imports,
/// and the entry module lowers on top of all of them. The result is one
/// self-contained module. Each item of an imported module is in it once
/// under the path of its module, for example `geom::Point`: its public
/// items and its private ones. A reference to an imported item names
/// that item, so a private helper of a module is the one that the
/// module's code calls, whatever the importer defines. An imported
/// struct keeps its methods, and an imported trait keeps the impls that
/// satisfy it, so an imported trait satisfies a local generic bound.
/// `tests/suite/cross_module.rs` and `tests/suite/mined_modules.rs` hold
/// the acceptance tests.
///
/// Then [`ir::MonomorphisePass`] runs. For a single file with no import,
/// this gives the same program as [`compile_to_ir`] followed by that
/// pass.
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
    let module = analyzer.lower_entry(&ast, prelude_len()?, None)?;
    Pipeline::new()
        .pass(ir::MonomorphisePass::default())
        .run(module)
}

/// Compile `FormaLang` source to IR with both a custom resolver and a known
/// source-file path.
///
/// The imported modules link into the result as in
/// [`compile_to_ir_with_resolver`], and the `file_table` starts with
/// `path` as in [`compile_to_ir_with_path`]. Then
/// [`ir::MonomorphisePass`] runs, as in [`compile_to_ir_with_resolver`].
///
/// # Errors
///
/// Returns a vector of [`CompilerError`] if compilation, IR lowering, or
/// monomorphisation fails.
pub fn compile_to_ir_with_path_and_resolver<R>(
    source: &str,
    path: std::path::PathBuf,
    resolver: R,
) -> Result<IrModule, Vec<CompilerError>>
where
    R: semantic::module_resolver::ModuleResolver,
{
    let (ast, analyzer) = compile_with_analyzer_and_resolver(source, resolver)?;
    let module = analyzer.lower_entry(&ast, prelude_len()?, Some(path))?;
    Pipeline::new()
        .pass(ir::MonomorphisePass::default())
        .run(module)
}
