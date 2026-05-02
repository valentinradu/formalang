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

    /// Visit a field definition
    fn visit_field(&mut self, _f: &IrField) {}

    /// Visit an expression (default walks children)
    fn visit_expr(&mut self, e: &IrExpr) {
        walk_expr_children(self, e);
    }
}
```

## Walking Functions

```rust
/// Walk an entire IR module
pub fn walk_module<V: IrVisitor>(visitor: &mut V, module: &IrModule);

/// Walk children of a module (called by default visit_module)
pub fn walk_module_children<V: IrVisitor>(visitor: &mut V, module: &IrModule);

/// Walk an expression tree
pub fn walk_expr<V: IrVisitor>(visitor: &mut V, expr: &IrExpr);

/// Walk children of an expression (called by default visit_expr)
pub fn walk_expr_children<V: IrVisitor>(visitor: &mut V, expr: &IrExpr);
```

## Example: Type Counter

```rust
use formalang::compile_to_ir;
use formalang::ir::{
    IrVisitor, IrStruct, IrEnum, StructId, EnumId, walk_module
};

struct TypeCounter {
    struct_count: usize,
    enum_count: usize,
}

impl IrVisitor for TypeCounter {
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.struct_count += 1;
    }

    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
        self.enum_count += 1;
    }
}

let source = r#"
pub struct User { name: String }
pub enum Status { active, inactive }
"#;
let module = compile_to_ir(source).unwrap();
let mut counter = TypeCounter { struct_count: 0, enum_count: 0 };
walk_module(&mut counter, &module);

assert_eq!(counter.struct_count, 1);
assert_eq!(counter.enum_count, 1);
```
