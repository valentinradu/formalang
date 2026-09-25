//! The linker: it puts the IR of each imported module into the module
//! that imports it.
//!
//! Each module lowers on top of the modules that it imports. The
//! lowering of a module starts with the prelude. Then the linker copies
//! each item of each imported module into the new module, once. Then
//! the module's own statements lower. The lowering thus sees every
//! imported struct, enum, trait, impl, function and `let` as a real
//! item with a real id.
//!
//! A name of a module-level item must be unique in one `IrModule`, but
//! two modules can each have a private item of one name. So the linker
//! gives each own item of an imported module a hidden name: the id of
//! its module, a `@` in front, then the name, for example `@2::helper`.
//! A program cannot write a `@`, so a hidden name never collides with
//! a name in the source. While a module lowers, each item that it
//! imports has its short name, and the lowering finds the item by that
//! name. After the lowering, the item gets its hidden name back.
//!
//! When the entry module is complete, [`finish_entry`] replaces each
//! hidden prefix with the module path that the source wrote, for
//! example `utils::helpers::double`.

mod names;
mod remap;
mod rename;

pub(crate) use names::{finish_entry, module_view};

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::IrModule;
use crate::location::Span;

use remap::IdMaps;

/// The hidden prefix of the own items of module `id`.
pub(crate) fn hidden_prefix(id: u32) -> String {
    format!("@{id}")
}

/// The hidden name of the own item `name` of module `id`.
pub(crate) fn hidden_name(id: u32, name: &str) -> String {
    format!("@{id}::{name}")
}

/// The number of items of each kind in a module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    structs: usize,
    enums: usize,
    traits: usize,
    impls: usize,
    functions: usize,
    lets: usize,
}

impl Counts {
    pub(crate) fn of(module: &IrModule) -> Self {
        Self {
            structs: module.structs.len(),
            enums: module.enums.len(),
            traits: module.traits.len(),
            impls: module.impls.len(),
            functions: module.functions.len(),
            lets: module.lets.len(),
        }
    }

    const fn get(&self, kind: Kind) -> usize {
        match kind {
            Kind::Struct => self.structs,
            Kind::Enum => self.enums,
            Kind::Trait => self.traits,
            Kind::Impl => self.impls,
            Kind::Function => self.functions,
            Kind::Let => self.lets,
        }
    }
}

/// A kind of module-level item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Struct,
    Enum,
    Trait,
    Impl,
    Function,
    Let,
}

impl Kind {
    const ALL: [Self; 6] = [
        Self::Struct,
        Self::Enum,
        Self::Trait,
        Self::Impl,
        Self::Function,
        Self::Let,
    ];
}

/// The module that defines an item, and the place of the item among
/// the own items of its kind in that module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Origin {
    module: u32,
    ordinal: u32,
}

/// The origin of each item after the prelude, one list for each kind.
/// Index `i` of a list is the item at `prelude + i` of that kind.
#[derive(Clone, Debug, Default)]
pub(crate) struct Origins {
    structs: Vec<Origin>,
    enums: Vec<Origin>,
    traits: Vec<Origin>,
    impls: Vec<Origin>,
    functions: Vec<Origin>,
    lets: Vec<Origin>,
}

impl Origins {
    const fn list(&self, kind: Kind) -> &Vec<Origin> {
        match kind {
            Kind::Struct => &self.structs,
            Kind::Enum => &self.enums,
            Kind::Trait => &self.traits,
            Kind::Impl => &self.impls,
            Kind::Function => &self.functions,
            Kind::Let => &self.lets,
        }
    }

    const fn list_mut(&mut self, kind: Kind) -> &mut Vec<Origin> {
        match kind {
            Kind::Struct => &mut self.structs,
            Kind::Enum => &mut self.enums,
            Kind::Trait => &mut self.traits,
            Kind::Impl => &mut self.impls,
            Kind::Function => &mut self.functions,
            Kind::Let => &mut self.lets,
        }
    }
}

/// The lowered IR of one module, with its imports linked in.
#[derive(Clone, Debug)]
pub(crate) struct LinkedModule {
    pub(crate) ir: IrModule,
    prelude: Counts,
    origins: Origins,
}

impl LinkedModule {
    pub(crate) const fn new(ir: IrModule, prelude: Counts, origins: Origins) -> Self {
        Self {
            ir,
            prelude,
            origins,
        }
    }
}

fn internal(detail: String) -> CompilerError {
    CompilerError::InternalError {
        detail,
        span: Span::default(),
    }
}

fn len_of(module: &IrModule, kind: Kind) -> usize {
    match kind {
        Kind::Struct => module.structs.len(),
        Kind::Enum => module.enums.len(),
        Kind::Trait => module.traits.len(),
        Kind::Impl => module.impls.len(),
        Kind::Function => module.functions.len(),
        Kind::Let => module.lets.len(),
    }
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; the callers push it into a Vec"
)]
fn id_of(index: usize) -> Result<u32, CompilerError> {
    u32::try_from(index).map_err(|_| CompilerError::TooManyDefinitions {
        kind: "item",
        span: Span::default(),
    })
}

/// Give each span of `module` in the synthetic file the file `file`.
pub(crate) fn move_to_file(module: &mut IrModule, file: crate::ir::FileId) {
    let maps = IdMaps {
        files: vec![file.0],
        ..IdMaps::default()
    };
    for s in &mut module.structs {
        maps.structure_def(s);
    }
    for e in &mut module.enums {
        maps.enum_def(e);
    }
    for t in &mut module.traits {
        maps.trait_def(t);
    }
    for i in &mut module.impls {
        maps.impl_def(i);
    }
    for f in &mut module.functions {
        maps.function_def(f);
    }
    for l in &mut module.lets {
        maps.let_def(l);
    }
}

/// Copy each item of `dep` that `target` does not hold yet into
/// `target`, and translate the ids in each copy.
///
/// Both modules start with the same prelude: `prelude` counts its
/// items. An item of the prelude keeps its id. Each other item is found
/// by its origin, so an item that two imports share arrives once.
///
/// # Errors
///
/// An `InternalError` when `dep` does not start with the same prelude,
/// or when its origin lists do not match its items. Both are defects of
/// the compiler.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; the callers push it into a Vec"
)]
pub(crate) fn link_into(
    target: &mut IrModule,
    origins: &mut Origins,
    prelude: &Counts,
    dep: &LinkedModule,
) -> Result<(), CompilerError> {
    if dep.prelude != *prelude {
        return Err(internal(
            "link: an imported module does not start with the prelude of its importer".to_string(),
        ));
    }
    let mut maps = IdMaps::default();
    let mut new_items: Vec<(Kind, usize)> = Vec::new();
    for kind in Kind::ALL {
        let known: HashMap<Origin, usize> = origins
            .list(kind)
            .iter()
            .enumerate()
            .map(|(i, o)| (*o, prelude.get(kind).saturating_add(i)))
            .collect();
        let mut next = len_of(target, kind);
        let mut table = Vec::with_capacity(len_of(&dep.ir, kind));
        for index in 0..len_of(&dep.ir, kind) {
            let Some(own) = index.checked_sub(prelude.get(kind)) else {
                table.push(id_of(index)?);
                continue;
            };
            let origin = *dep.origins.list(kind).get(own).ok_or_else(|| {
                internal(format!(
                    "link: item {index} of an imported module has no origin"
                ))
            })?;
            if let Some(&existing) = known.get(&origin) {
                table.push(id_of(existing)?);
            } else {
                table.push(id_of(next)?);
                next = next.saturating_add(1);
                new_items.push((kind, index));
                origins.list_mut(kind).push(origin);
            }
        }
        match kind {
            Kind::Struct => maps.structs = table,
            Kind::Enum => maps.enums = table,
            Kind::Trait => maps.traits = table,
            Kind::Impl => maps.impls = table,
            Kind::Function => maps.functions = table,
            Kind::Let => maps.lets = table,
        }
    }
    maps.files.push(0);
    for path in &dep.ir.file_table {
        maps.files.push(target.register_file(path.clone()).0);
    }
    copy_items(target, dep, &new_items, &maps);
    target.rebuild_indices();
    Ok(())
}

/// Append the items `new_items` of `dep` to `target`, in order, with
/// their ids translated by `maps`.
fn copy_items(
    target: &mut IrModule,
    dep: &LinkedModule,
    new_items: &[(Kind, usize)],
    maps: &IdMaps,
) {
    for &(kind, index) in new_items {
        match kind {
            Kind::Struct => {
                if let Some(item) = dep.ir.structs.get(index) {
                    let mut item = item.clone();
                    maps.structure_def(&mut item);
                    target.structs.push(item);
                }
            }
            Kind::Enum => {
                if let Some(item) = dep.ir.enums.get(index) {
                    let mut item = item.clone();
                    maps.enum_def(&mut item);
                    target.enums.push(item);
                }
            }
            Kind::Trait => {
                if let Some(item) = dep.ir.traits.get(index) {
                    let mut item = item.clone();
                    maps.trait_def(&mut item);
                    target.traits.push(item);
                }
            }
            Kind::Impl => {
                if let Some(item) = dep.ir.impls.get(index) {
                    let mut item = item.clone();
                    maps.impl_def(&mut item);
                    target.impls.push(item);
                }
            }
            Kind::Function => {
                if let Some(item) = dep.ir.functions.get(index) {
                    let mut item = item.clone();
                    maps.function_def(&mut item);
                    target.functions.push(item);
                }
            }
            Kind::Let => {
                if let Some(item) = dep.ir.lets.get(index) {
                    let mut item = item.clone();
                    maps.let_def(&mut item);
                    target.lets.push(item);
                }
            }
        }
    }
}

/// A name that a module imports: the short name and the id of the
/// module that defines the item.
#[derive(Clone, Debug)]
pub(crate) struct Import {
    pub(crate) name: String,
    pub(crate) origin: u32,
}

/// The items that got their short import names, so that
/// [`finish_module`] can give them their hidden names back.
#[derive(Debug, Default)]
pub(crate) struct Aliases {
    renamed: Vec<(Kind, usize, String)>,
    /// Short name to hidden name, for each imported `let`.
    pub(crate) lets: HashMap<String, String>,
}

/// Give each item that the module imports its short name.
///
/// `use lib::open` gives `@3::open` the name `open`. An import of an
/// inline module, `use shapes::fill`, gives `@3::fill::Solid` the name
/// `fill::Solid`.
pub(crate) fn alias_imports(module: &mut IrModule, imports: &[Import]) -> Aliases {
    let mut aliases = Aliases::default();
    let short_of = |name: &str| -> Option<String> {
        imports.iter().find_map(|import| {
            let hidden = hidden_name(import.origin, &import.name);
            if name == hidden {
                Some(import.name.clone())
            } else {
                name.strip_prefix(&hidden)
                    .and_then(|rest| rest.strip_prefix("::"))
                    .map(|rest| format!("{}::{rest}", import.name))
            }
        })
    };
    let mut rename = |kind: Kind, index: usize, name: &mut String| {
        if let Some(short) = short_of(name) {
            let hidden = std::mem::replace(name, short);
            if kind == Kind::Let {
                aliases.lets.insert(name.clone(), hidden.clone());
            }
            aliases.renamed.push((kind, index, hidden));
        }
    };
    for (i, s) in module.structs.iter_mut().enumerate() {
        rename(Kind::Struct, i, &mut s.name);
    }
    for (i, e) in module.enums.iter_mut().enumerate() {
        rename(Kind::Enum, i, &mut e.name);
    }
    for (i, t) in module.traits.iter_mut().enumerate() {
        rename(Kind::Trait, i, &mut t.name);
    }
    for (i, f) in module.functions.iter_mut().enumerate() {
        rename(Kind::Function, i, &mut f.name);
    }
    for (i, l) in module.lets.iter_mut().enumerate() {
        rename(Kind::Let, i, &mut l.name);
    }
    module.rebuild_indices();
    aliases
}

/// Finish the lowering of one module.
///
/// `linked` counts the items before the module's own items. The items
/// that [`alias_imports`] renamed get their hidden names back. When the
/// module is imported (`own` is its id), each own item gets its hidden
/// name, and each call in the module's own code names the function by
/// the name that it now has.
///
/// Returns the origins of the module's items.
///
/// # Errors
///
/// `TooManyDefinitions` when a count does not fit in a `u32`.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; the callers push it into a Vec"
)]
pub(crate) fn finish_module(
    module: &mut IrModule,
    mut origins: Origins,
    linked: &Counts,
    aliases: Aliases,
    own: Option<u32>,
) -> Result<Origins, CompilerError> {
    // The name each function has before this step, for the calls that
    // lowering left without an id.
    let before: Vec<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    for (kind, index, hidden) in aliases.renamed {
        let slot = match kind {
            Kind::Struct => module.structs.get_mut(index).map(|s| &mut s.name),
            Kind::Enum => module.enums.get_mut(index).map(|e| &mut e.name),
            Kind::Trait => module.traits.get_mut(index).map(|t| &mut t.name),
            Kind::Function => module.functions.get_mut(index).map(|f| &mut f.name),
            Kind::Let => module.lets.get_mut(index).map(|l| &mut l.name),
            Kind::Impl => None,
        };
        if let Some(name) = slot {
            *name = hidden;
        }
    }
    let module_id = own.unwrap_or(u32::MAX);
    for kind in Kind::ALL {
        let start = linked.get(kind);
        for (ordinal, index) in (start..len_of(module, kind)).enumerate() {
            origins.list_mut(kind).push(Origin {
                module: module_id,
                ordinal: id_of(ordinal)?,
            });
            if let Some(id) = own {
                rename::hide_own_name(module, kind, index, id);
            }
        }
    }
    rename::retarget_calls(module, linked, &before);
    module.rebuild_indices();
    Ok(origins)
}
