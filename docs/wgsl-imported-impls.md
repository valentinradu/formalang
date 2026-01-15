# WGSL Imported Impl Blocks

## Purpose

When FormaLang imports a struct via `use stdlib::shapes::Rect`, the WGSL codegen
must also generate methods from `impl Rect { ... }` defined in the imported module.
Previously, only local impl blocks were generated, breaking method calls on imported types.

## Architecture

```
SemanticAnalyzer                    WgslGenerator
       |                                  |
       v                                  v
parse_and_analyze_module() -----> module_ir_cache (HashMap<PathBuf, IrModule>)
       |                                  |
       v                                  v
   symbols + IR cached            gen_imported_impls()
                                         |
                                         v
                                  For each imported struct/enum:
                                  - Find IR in cache via source_file
                                  - gen_impl_from_foreign(impl, source_module)
                                  - Use source_module for all ID lookups
```

**Key insight**: IDs (StructId, EnumId) are module-local. Instead of ID remapping,
each imported module's IR is kept separate. At codegen, resolve IDs using the
source module, not the main module.

## Key Decisions

| Decision | Alternative | Rationale |
|----------|-------------|-----------|
| Cache full IrModule per import | Cache only impl blocks | Simpler; needed for type lookups |
| Name-based resolution at codegen | ID remapping visitor | Lower complexity; WGSL uses names anyway |
| Separate `_from_foreign` methods | Unified `_from` with module param | Faster implementation; P4 refactor deferred |
| `ImplTarget` enum for struct/enum | Separate fields | Cleaner model; both supported uniformly |

## Usage

```rust
// With imports (recommended)
let (ast, analyzer) = compile_with_analyzer(source)?;
let ir = lower_to_ir(&ast, analyzer.symbols())?;
let wgsl = generate_wgsl_with_imports(&ir, analyzer.imported_ir_modules());

// Convenience function
let wgsl = compile_to_wgsl(source)?;
```

## Testing

| Coverage | Location |
|----------|----------|
| IR caching | `tests/wgsl_imported_impls.rs` (10 tests) |
| Imported impl generation | `tests/wgsl_imported_impls.rs` |
| Enum impl support | `src/ir/types.rs:114-150` |
| For/Match/Block codegen | `src/codegen/wgsl.rs:438-580` |

Run: `cargo test --test wgsl_imported_impls`

## Maintenance

**Limitations:**

- P4 deferred: `gen_expr_from_foreign` duplicates logic from `gen_expr`. A future
  refactor could unify via `gen_expr_impl(expr, module)` wrapper pattern.

**Key locations:**

- IR caching: `src/semantic/mod.rs` - `module_ir_cache`, `imported_ir_modules()`
- Import tracking: `src/ir/lower.rs:725-780` - `lower_impl`, `try_track_external_import`
- Impl target: `src/ir/types.rs:114-150` - `ImplTarget` enum
- Foreign codegen: `src/codegen/wgsl.rs:220-580` - `gen_imported_impls`, `gen_*_from_foreign`

**Watch for:**

- New expression types in IR need handling in both `gen_expr` and `gen_expr_from_foreign`
- Monomorphization runs on imported modules in `gen_monomorphized_structs()`
