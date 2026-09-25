# Visitor Pattern

The IR provides a visitor trait for traversal, allowing code generators
to process nodes without implementing manual traversal logic.

## IrVisitor Trait

```rust
pub trait IrVisitor {
    /// Visit entire module (default walks all children)
    fn visit_module(&mut self, module: &IrModule) {
        walk_module_children(self, module);
    }

    /// Visit a struct definition
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {}

    /// Visit a trait definition
    fn visit_trait(&mut self, _id: TraitId, _t: &IrTrait) {}

    /// Visit an enum definition
    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {}

    /// Visit an enum variant
    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {}

    /// Visit an impl block
    fn visit_impl(&mut self, _i: &IrImpl) {}

    /// Visit a function: a standalone function or an impl method
    fn visit_function(&mut self, _f: &IrFunction) {}

    /// Visit a module-level let binding
    fn visit_let(&mut self, _l: &IrLet) {}

    /// Visit an import record
    fn visit_import(&mut self, _i: &IrImport) {}

    /// Visit a field definition
    fn visit_field(&mut self, _f: &IrField) {}

    /// Visit an expression (default walks children)
    fn visit_expr(&mut self, e: &IrExpr) {
        walk_expr_children(self, e);
    }
}
```

`walk_module_children` visits, in this order:

1. each struct, then each of its fields and the default of each field;
2. each trait, then each of its fields;
3. each enum, then each variant and each field of the variant;
4. each impl block, then each of its methods and the body of each;
5. each standalone function and its body;
6. each module-level `let` and its value;
7. each import record.

The walk does not enter the defaults of trait fields, variant fields
or parameters. Read them in the visitor when a backend needs them.

The module starts with the prelude, so the walk also visits the
prelude definitions (`Array`, `Seq`, `Dictionary`, `Range`,
`Optional`, their impl blocks and `assert`). Use
`IrModule::is_prelude_struct` and `IrModule::is_prelude_enum` to skip
them.

## Walking Functions

```rust
/// Walk an entire IR module
pub fn walk_module<V: IrVisitor + ?Sized>(visitor: &mut V, module: &IrModule);

/// Walk children of a module (called by default visit_module)
pub fn walk_module_children<V: IrVisitor + ?Sized>(visitor: &mut V, module: &IrModule);

/// Walk an expression tree
pub fn walk_expr<V: IrVisitor + ?Sized>(visitor: &mut V, expr: &IrExpr);

/// Walk children of an expression (called by default visit_expr)
pub fn walk_expr_children<V: IrVisitor + ?Sized>(visitor: &mut V, expr: &IrExpr);

/// Walk the expressions of one block statement
pub fn walk_block_statement<V: IrVisitor + ?Sized>(visitor: &mut V, stmt: &IrBlockStatement);
```

## Example: Type Counter

```rust
use formalang::compile_to_ir;
use formalang::ir::{walk_module, EnumId, IrEnum, IrModule, IrStruct, IrVisitor, StructId};

struct TypeCounter<'m> {
    module: &'m IrModule,
    struct_count: usize,
    enum_count: usize,
}

impl IrVisitor for TypeCounter<'_> {
    fn visit_struct(&mut self, id: StructId, _s: &IrStruct) {
        if !self.module.is_prelude_struct(id) {
            self.struct_count += 1;
        }
    }

    fn visit_enum(&mut self, id: EnumId, _e: &IrEnum) {
        if !self.module.is_prelude_enum(id) {
            self.enum_count += 1;
        }
    }
}

let source = r#"
pub struct User { name: String }
pub enum Status { active, inactive }
"#;
let module = compile_to_ir(source).unwrap();
let mut counter = TypeCounter { module: &module, struct_count: 0, enum_count: 0 };
walk_module(&mut counter, &module);

assert_eq!(counter.struct_count, 1);
assert_eq!(counter.enum_count, 1);
```
