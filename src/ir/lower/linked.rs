//! The lowering of one module on top of the modules that it imports.
//!
//! See [`crate::ir::link`] for the plan. This file drives the steps:
//! the prelude lowers first, then the linker copies in each imported
//! module, then the module's own statements lower, and then the linker
//! gives the names their final form.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::{File, Statement};
use crate::error::CompilerError;
use crate::ir::link::{self, Counts, Import, LinkedModule, Origins};
use crate::semantic::SymbolTable;

use super::IrLowerer;

/// What the lowering of one module needs.
pub(crate) struct LinkedLowering<'a> {
    /// The module's AST. The prelude's statements come first.
    pub(crate) ast: &'a File,
    /// The number of prelude statements at the start of `ast`.
    pub(crate) prelude_len: usize,
    /// The module's symbol table.
    pub(crate) symbols: &'a SymbolTable,
    /// The module's source file, when it has one.
    pub(crate) path: Option<PathBuf>,
    /// The id of the module when another module imports it, `None` for
    /// the entry module.
    pub(crate) own: Option<u32>,
    /// The lowered modules that this module imports from.
    pub(crate) deps: Vec<&'a LinkedModule>,
    /// Each name that this module imports, with the id of its module.
    pub(crate) imports: Vec<Import>,
}

fn part(file: &File, statements: &[Statement]) -> File {
    File {
        doc: file.doc.clone(),
        statements: statements.to_vec(),
        span: file.span,
    }
}

/// Lower a module on top of the modules that it imports.
///
/// # Errors
///
/// The errors of the lowering, and an `InternalError` when the linker
/// finds that two modules disagree on the prelude.
pub(crate) fn lower_linked(input: LinkedLowering<'_>) -> Result<LinkedModule, Vec<CompilerError>> {
    let (_, own) = input
        .ast
        .statements
        .split_at_checked(input.prelude_len)
        .ok_or_else(|| {
            vec![CompilerError::InternalError {
                detail: "IR lowering: the module has fewer statements than the prelude".to_string(),
                span: input.ast.span,
            }]
        })?;
    let mut lowerer = IrLowerer::new(input.symbols);
    lowerer.linked = true;
    lowerer.module = crate::prelude_ir()?.clone();
    let prelude_counts = Counts::of(&lowerer.module);
    if let Some(path) = input.path {
        lowerer.current_file = lowerer.module.register_file(path);
        // The prelude items take the file of the module, as they do
        // when the prelude lowers with the module.
        link::move_to_file(&mut lowerer.module, lowerer.current_file);
    }

    let mut origins = Origins::default();
    for dep in &input.deps {
        link::link_into(&mut lowerer.module, &mut origins, &prelude_counts, dep)
            .map_err(|e| vec![e])?;
    }
    let linked = Counts::of(&lowerer.module);
    let aliases = link::alias_imports(&mut lowerer.module, &input.imports);

    // A reference to a module `let` names the `let` by the name that it
    // has in the finished module.
    let mut let_names: HashMap<String, String> = aliases.lets.clone();
    if let Some(id) = input.own {
        for statement in own {
            if let Statement::Let(binding) = statement {
                for name in
                    crate::semantic::helpers::collect_bindings_from_pattern(&binding.pattern)
                {
                    let hidden = link::hidden_name(id, &name.name);
                    let_names.insert(name.name, hidden);
                }
            }
        }
    }
    lowerer.linked_let_names = let_names;

    lowerer.lower_file(&part(input.ast, own))?;
    let origins = link::finish_module(&mut lowerer.module, origins, &linked, aliases, input.own)
        .map_err(|e| vec![e])?;
    Ok(LinkedModule::new(lowerer.module, prelude_counts, origins))
}
