//! The IR decode surface.
//!
//! `IrModule` JSON on disk is a compatibility contract with external
//! consumers (see `AGENTS.md`). A backend that reads an `IrModule` a
//! different tool wrote is reading untrusted input: the ids inside it
//! can point anywhere.
//!
//! This target decodes arbitrary bytes as an `IrModule` and, on
//! success, runs the accessors and the built-in passes over it. The
//! passes must reject or tolerate a malformed module, never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::IrPass;

fuzz_target!(|data: &[u8]| {
    let Ok(mut module) = serde_json::from_slice::<IrModule>(data) else {
        return;
    };

    // The name maps are `#[serde(skip)]`; a decoded module has none
    // until the consumer rebuilds them.
    module.rebuild_indices();

    // Accessors must be total over any decoded module.
    for s in &module.structs {
        let _ = module
            .struct_id(&s.name)
            .and_then(|id| module.get_struct(id));
    }
    for t in &module.traits {
        let _ = module.trait_id(&t.name).and_then(|id| module.get_trait(id));
    }
    for e in &module.enums {
        let _ = module.enum_id(&e.name).and_then(|id| module.get_enum(id));
    }
    let _ = module.user_structs().count();
    let _ = module.user_enums().count();

    // Each pass gets its own copy: a pass that fails must not be able
    // to corrupt the input for the next one.
    let _ = ConstantFoldingPass::new().run(module.clone());
    let _ = DeadCodeEliminationPass::new().run(module.clone());
    let _ = ResolveReferencesPass::new().run(module.clone());
    let _ = ClosureConversionPass::new().run(module.clone());
    let _ = DefunctionalisePass::new().run(module.clone());
    let _ = MonomorphisePass::default().run(module);
});
