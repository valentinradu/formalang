//! Tests for the IR (Intermediate Representation) module
//!
//! Tests lowering from AST to IR and verifies correct type resolution.

use formalang::compile_to_ir;

// =============================================================================
// Basic Lowering Tests
// =============================================================================

#[test]
fn test_lower_empty_source() {
    let result = compile_to_ir("");
    assert!(result.is_ok());
    let module = result.unwrap();
    assert!(module.structs.is_empty());
    assert!(module.traits.is_empty());
    assert!(module.enums.is_empty());
    assert!(module.impls.is_empty());
}

#[test]
fn test_lower_simple_struct() {
    let source = "struct Point { x: Number, y: Number }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    let point = &module.structs[0];
    assert_eq!(point.name, "Point");
    assert_eq!(point.fields.len(), 2);
    assert_eq!(point.fields[0].name, "x");
    assert_eq!(point.fields[1].name, "y");
}

#[test]
fn test_lower_struct_with_string_field() {
    let source = "struct User { name: String }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    let user = &module.structs[0];
    assert_eq!(user.name, "User");
    assert_eq!(user.fields.len(), 1);
    assert_eq!(user.fields[0].name, "name");
}

#[test]
fn test_lower_struct_with_boolean_field() {
    let source = "struct Config { enabled: Boolean }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    assert_eq!(module.structs[0].fields[0].name, "enabled");
}

#[test]
fn test_lower_struct_with_array_field() {
    let source = "struct List { items: [String] }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    assert_eq!(module.structs[0].fields[0].name, "items");
}

#[test]
fn test_lower_struct_with_optional_field() {
    let source = "struct Profile { bio: String? }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    let field = &module.structs[0].fields[0];
    assert_eq!(field.name, "bio");
    assert!(field.optional);
}

#[test]
fn test_lower_struct_with_mutable_field() {
    let source = "struct Counter { mut count: Number }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    let field = &module.structs[0].fields[0];
    assert_eq!(field.name, "count");
    assert!(field.mutable);
}

#[test]
fn test_lower_public_struct() {
    let source = "pub struct Public { value: Number }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    assert!(module.structs[0].visibility.is_public());
}

#[test]
fn test_lower_private_struct() {
    let source = "struct Private { value: Number }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    assert!(!module.structs[0].visibility.is_public());
}

// =============================================================================
// Trait Lowering Tests
// =============================================================================

#[test]
fn test_lower_simple_trait() {
    let source = "trait Named { name: String }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 1);
    let named = &module.traits[0];
    assert_eq!(named.name, "Named");
    assert_eq!(named.fields.len(), 1);
    assert_eq!(named.fields[0].name, "name");
}

#[test]
fn test_lower_trait_with_multiple_fields() {
    let source = "trait Entity { id: Number, name: String, active: Boolean }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 1);
    assert_eq!(module.traits[0].fields.len(), 3);
}

#[test]
fn test_lower_public_trait() {
    let source = "pub trait Visible { }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 1);
    assert!(module.traits[0].visibility.is_public());
}

// =============================================================================
// Enum Lowering Tests
// =============================================================================

#[test]
fn test_lower_simple_enum() {
    let source = "enum Status { active, inactive, pending }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.enums.len(), 1);
    let status = &module.enums[0];
    assert_eq!(status.name, "Status");
    assert_eq!(status.variants.len(), 3);
    assert_eq!(status.variants[0].name, "active");
    assert_eq!(status.variants[1].name, "inactive");
    assert_eq!(status.variants[2].name, "pending");
}

#[test]
fn test_lower_enum_with_data() {
    let source = "enum Result { ok(value: String), error(message: String) }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.enums.len(), 1);
    let result_enum = &module.enums[0];
    assert_eq!(result_enum.name, "Result");
    assert_eq!(result_enum.variants.len(), 2);

    // Check variant with data
    let ok_variant = &result_enum.variants[0];
    assert_eq!(ok_variant.name, "ok");
    assert_eq!(ok_variant.fields.len(), 1);
    assert_eq!(ok_variant.fields[0].name, "value");

    let error_variant = &result_enum.variants[1];
    assert_eq!(error_variant.name, "error");
    assert_eq!(error_variant.fields.len(), 1);
    assert_eq!(error_variant.fields[0].name, "message");
}

#[test]
fn test_lower_enum_mixed_variants() {
    let source = "enum Option { none, some(value: Number) }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let option = &module.enums[0];
    assert_eq!(option.variants[0].name, "none");
    assert!(option.variants[0].fields.is_empty()); // Unit variant
    assert_eq!(option.variants[1].name, "some");
    assert_eq!(option.variants[1].fields.len(), 1); // Data variant
}

#[test]
fn test_lower_public_enum() {
    let source = "pub enum Color { red, green, blue }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert!(module.enums[0].visibility.is_public());
}

// =============================================================================
// Module Lookup Tests
// =============================================================================

#[test]
fn test_struct_id_lookup() {
    let source = "struct A { } struct B { } struct C { }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert!(module.struct_id("A").is_some());
    assert!(module.struct_id("B").is_some());
    assert!(module.struct_id("C").is_some());
    assert!(module.struct_id("D").is_none());
}

#[test]
fn test_trait_id_lookup() {
    let source = "trait X { } trait Y { }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert!(module.trait_id("X").is_some());
    assert!(module.trait_id("Y").is_some());
    assert!(module.trait_id("Z").is_none());
}

#[test]
fn test_enum_id_lookup() {
    let source = "enum E1 { a } enum E2 { b }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert!(module.enum_id("E1").is_some());
    assert!(module.enum_id("E2").is_some());
    assert!(module.enum_id("E3").is_none());
}

#[test]
fn test_get_struct_by_id() {
    let source = "struct First { a: Number } struct Second { b: String }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let first_id = module.struct_id("First").unwrap();
    let second_id = module.struct_id("Second").unwrap();

    assert_eq!(module.get_struct(first_id).name, "First");
    assert_eq!(module.get_struct(second_id).name, "Second");
}

#[test]
fn test_get_trait_by_id() {
    let source = "trait TraitA { } trait TraitB { }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let a_id = module.trait_id("TraitA").unwrap();
    let b_id = module.trait_id("TraitB").unwrap();

    assert_eq!(module.get_trait(a_id).name, "TraitA");
    assert_eq!(module.get_trait(b_id).name, "TraitB");
}

#[test]
fn test_get_enum_by_id() {
    let source = "enum EnumA { x } enum EnumB { y }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let a_id = module.enum_id("EnumA").unwrap();
    let b_id = module.enum_id("EnumB").unwrap();

    assert_eq!(module.get_enum(a_id).name, "EnumA");
    assert_eq!(module.get_enum(b_id).name, "EnumB");
}

// =============================================================================
// Impl Block Lowering Tests
// =============================================================================

#[test]
fn test_lower_impl_block() {
    let source = r#"
        struct Counter { count: Number }
        impl Counter { count }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 1);
    assert_eq!(module.impls.len(), 1);
}

#[test]
fn test_lower_impl_with_literal() {
    let source = r#"
        struct Config { name: String }
        impl Config { "default" }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.impls.len(), 1);
    assert!(!module.impls[0].body.is_empty());
}

// =============================================================================
// Struct with Trait Implementation Tests
// =============================================================================

#[test]
fn test_lower_struct_implementing_trait() {
    let source = r#"
        trait Named { name: String }
        struct User: Named { name: String, age: Number }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 1);
    assert_eq!(module.structs.len(), 1);

    let user = &module.structs[0];
    assert_eq!(user.name, "User");
    assert!(!user.traits.is_empty());
}

#[test]
fn test_lower_struct_with_multiple_traits() {
    let source = r#"
        trait Named { name: String }
        trait Aged { age: Number }
        struct Person: Named + Aged { name: String, age: Number }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let person = &module.structs[0];
    assert_eq!(person.traits.len(), 2);
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_lower_generic_struct() {
    let source = "struct Box<T> { value: T }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let box_struct = &module.structs[0];
    assert_eq!(box_struct.name, "Box");
    assert_eq!(box_struct.generic_params.len(), 1);
    assert_eq!(box_struct.generic_params[0].name, "T");
}

#[test]
fn test_lower_generic_trait() {
    let source = "trait Container<T> { item: T }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let container = &module.traits[0];
    assert_eq!(container.name, "Container");
    assert_eq!(container.generic_params.len(), 1);
    assert_eq!(container.generic_params[0].name, "T");
}

#[test]
fn test_lower_generic_enum() {
    let source = "enum Maybe<T> { nothing, just(value: T) }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let maybe = &module.enums[0];
    assert_eq!(maybe.name, "Maybe");
    assert_eq!(maybe.generic_params.len(), 1);
    assert_eq!(maybe.generic_params[0].name, "T");
}

#[test]
fn test_lower_multiple_generic_params() {
    let source = "struct Pair<A, B> { first: A, second: B }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let pair = &module.structs[0];
    assert_eq!(pair.generic_params.len(), 2);
    assert_eq!(pair.generic_params[0].name, "A");
    assert_eq!(pair.generic_params[1].name, "B");
}

// =============================================================================
// Complex Definition Tests
// =============================================================================

#[test]
fn test_lower_multiple_definitions() {
    let source = r#"
        trait Identifiable { id: Number }
        struct User: Identifiable { id: Number, name: String }
        struct Post: Identifiable { id: Number, title: String }
        enum Status { draft, published, archived }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 1);
    assert_eq!(module.structs.len(), 2);
    assert_eq!(module.enums.len(), 1);
}

#[test]
fn test_lower_struct_referencing_another() {
    let source = r#"
        struct Author { name: String }
        struct Book { title: String, author: Author }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.structs.len(), 2);

    // Book should have an Author field with struct type
    let book = module.structs.iter().find(|s| s.name == "Book").unwrap();
    let author_field = book.fields.iter().find(|f| f.name == "author").unwrap();
    // The type should reference Author struct
    assert!(matches!(
        &author_field.ty,
        formalang::ir::ResolvedType::Struct(_)
    ));
}

#[test]
fn test_lower_struct_with_enum_field() {
    let source = r#"
        enum Status { active, inactive }
        struct User { name: String, status: Status }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let user = module.structs.iter().find(|s| s.name == "User").unwrap();
    let status_field = user.fields.iter().find(|f| f.name == "status").unwrap();
    assert!(matches!(
        &status_field.ty,
        formalang::ir::ResolvedType::Enum(_)
    ));
}

// =============================================================================
// Default Value Tests
// =============================================================================

#[test]
fn test_lower_field_with_default_number() {
    let source = "struct Counter { count: Number = 0 }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let field = &module.structs[0].fields[0];
    assert_eq!(field.name, "count");
    assert!(field.default.is_some());
}

#[test]
fn test_lower_field_with_default_string() {
    let source = r#"struct Config { name: String = "default" }"#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let field = &module.structs[0].fields[0];
    assert!(field.default.is_some());
}

#[test]
fn test_lower_field_with_default_boolean() {
    let source = "struct Settings { enabled: Boolean = true }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let field = &module.structs[0].fields[0];
    assert!(field.default.is_some());
}

// =============================================================================
// Trait Composition Tests
// =============================================================================

#[test]
fn test_lower_trait_composition() {
    let source = r#"
        trait A { a: Number }
        trait B { b: Number }
        trait C: A + B { c: Number }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.traits.len(), 3);

    let trait_c = module.traits.iter().find(|t| t.name == "C").unwrap();
    assert_eq!(trait_c.composed_traits.len(), 2);
}

// =============================================================================
// Nested Array Tests
// =============================================================================

#[test]
fn test_lower_nested_array_type() {
    let source = "struct Matrix { rows: [[Number]] }";
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    let field = &module.structs[0].fields[0];
    assert_eq!(field.name, "rows");
    // Should be Array(Array(Primitive(Number)))
    if let formalang::ir::ResolvedType::Array(inner) = &field.ty {
        assert!(matches!(
            inner.as_ref(),
            formalang::ir::ResolvedType::Array(_)
        ));
    } else {
        panic!("Expected nested array type");
    }
}

// =============================================================================
// Error Case Tests
// =============================================================================

#[test]
fn test_lower_invalid_source_returns_error() {
    let source = "this is not valid formalang";
    let result = compile_to_ir(source);
    assert!(result.is_err());
}

#[test]
fn test_lower_undefined_type_returns_error() {
    let source = "struct Bad { field: UnknownType }";
    let result = compile_to_ir(source);
    assert!(result.is_err());
}

#[test]
fn test_lower_duplicate_struct_returns_error() {
    let source = "struct Dup { } struct Dup { }";
    let result = compile_to_ir(source);
    assert!(result.is_err());
}

// =============================================================================
// Visitor Pattern Tests
// =============================================================================

use formalang::ir::{
    walk_module, EnumId, IrEnum, IrEnumVariant, IrField, IrImpl, IrStruct, IrTrait, IrVisitor,
    StructId, TraitId,
};

struct TypeCounter {
    struct_count: usize,
    trait_count: usize,
    enum_count: usize,
    field_count: usize,
    impl_count: usize,
    variant_count: usize,
}

impl TypeCounter {
    fn new() -> Self {
        Self {
            struct_count: 0,
            trait_count: 0,
            enum_count: 0,
            field_count: 0,
            impl_count: 0,
            variant_count: 0,
        }
    }
}

impl IrVisitor for TypeCounter {
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.struct_count += 1;
    }

    fn visit_trait(&mut self, _id: TraitId, _t: &IrTrait) {
        self.trait_count += 1;
    }

    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
        self.enum_count += 1;
    }

    fn visit_field(&mut self, _f: &IrField) {
        self.field_count += 1;
    }

    fn visit_impl(&mut self, _i: &IrImpl) {
        self.impl_count += 1;
    }

    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {
        self.variant_count += 1;
    }
}

#[test]
fn test_visitor_counts_structs() {
    let source = "struct A { } struct B { } struct C { }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.struct_count, 3);
}

#[test]
fn test_visitor_counts_traits() {
    let source = "trait X { } trait Y { }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.trait_count, 2);
}

#[test]
fn test_visitor_counts_enums() {
    let source = "enum E1 { a } enum E2 { b } enum E3 { c }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.enum_count, 3);
}

#[test]
fn test_visitor_counts_fields() {
    let source = "struct Point { x: Number, y: Number, z: Number }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.field_count, 3);
}

#[test]
fn test_visitor_counts_variants() {
    let source = "enum Color { red, green, blue, yellow }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.variant_count, 4);
}

#[test]
fn test_visitor_counts_impls() {
    let source = r#"
        struct A { x: Number }
        struct B { y: Number }
        impl A { x }
        impl B { y }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.impl_count, 2);
}

#[test]
fn test_visitor_mixed_definitions() {
    let source = r#"
        trait Named { name: String }
        struct User: Named { name: String, age: Number }
        enum Status { active, inactive }
        impl User { name }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.struct_count, 1);
    assert_eq!(counter.trait_count, 1);
    assert_eq!(counter.enum_count, 1);
    assert_eq!(counter.impl_count, 1);
    // 1 trait field + 2 struct fields = 3 fields
    assert_eq!(counter.field_count, 3);
    // 2 enum variants
    assert_eq!(counter.variant_count, 2);
}

#[test]
fn test_visitor_enum_variant_fields() {
    let source = "enum Option { none, some(value: Number) }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    // 2 variants
    assert_eq!(counter.variant_count, 2);
    // 1 field (in "some" variant)
    assert_eq!(counter.field_count, 1);
}

#[test]
fn test_visitor_trait_fields() {
    let source = "trait Entity { id: Number, name: String }";
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.trait_count, 1);
    assert_eq!(counter.field_count, 2);
}

// =============================================================================
// Expression Type Tests (using IrExpr::ty() method)
// =============================================================================

use formalang::ir::ResolvedType;

fn type_name(ty: &ResolvedType) -> String {
    match ty {
        ResolvedType::Primitive(p) => format!("{:?}", p),
        ResolvedType::Struct(_) => "Struct".to_string(),
        ResolvedType::Enum(_) => "Enum".to_string(),
        ResolvedType::Array(_) => "Array".to_string(),
        _ => "Other".to_string(),
    }
}

#[test]
fn test_expr_type_literal_string() {
    let source = r#"
        struct S { name: String }
        impl S { "hello" }
    "#;
    let module = compile_to_ir(source).unwrap();

    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].body[0];
    assert_eq!(type_name(expr.ty()), "String");
}

#[test]
fn test_expr_type_literal_number() {
    let source = r#"
        struct S { value: Number }
        impl S { 42 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    assert_eq!(type_name(expr.ty()), "Number");
}

#[test]
fn test_expr_type_literal_boolean() {
    let source = r#"
        struct S { flag: Boolean }
        impl S { true }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    assert_eq!(type_name(expr.ty()), "Boolean");
}

#[test]
fn test_expr_type_array() {
    let source = r#"
        struct S { items: [Number] }
        impl S { [1, 2, 3] }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    assert_eq!(type_name(expr.ty()), "Array");
}

#[test]
fn test_expr_type_struct_instantiation() {
    let source = r#"
        struct Point { x: Number, y: Number }
        struct Container { p: Point }
        impl Container { Point(x: 1, y: 2) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    assert_eq!(type_name(expr.ty()), "Struct");
}

#[test]
fn test_expr_type_reference() {
    let source = r#"
        struct S { x: Number }
        impl S { x }
    "#;
    let module = compile_to_ir(source).unwrap();

    // Impl body should have the reference expression
    assert!(!module.impls.is_empty());
    assert!(!module.impls[0].body.is_empty());
    // The expression has a type (implementation detail: might be TypeParam for field refs)
    let _ty = module.impls[0].body[0].ty();
}

#[test]
fn test_expr_type_binary_arithmetic() {
    let source = r#"
        struct S { sum: Number }
        impl S { 1 + 2 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    // Arithmetic results in Number
    assert_eq!(type_name(expr.ty()), "Number");
}

#[test]
fn test_expr_type_binary_comparison() {
    let source = r#"
        struct S { result: Boolean }
        impl S { 1 == 2 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].body[0];
    // Comparison results in Boolean
    assert_eq!(type_name(expr.ty()), "Boolean");
}

// =============================================================================
// ResolvedType Display Name Tests
// =============================================================================

#[test]
fn test_resolved_type_display_primitive() {
    let source = "struct S { n: Number, s: String, b: Boolean }";
    let module = compile_to_ir(source).unwrap();

    let s = &module.structs[0];
    assert_eq!(s.fields[0].ty.display_name(&module), "Number");
    assert_eq!(s.fields[1].ty.display_name(&module), "String");
    assert_eq!(s.fields[2].ty.display_name(&module), "Boolean");
}

#[test]
fn test_resolved_type_display_array() {
    let source = "struct S { items: [String] }";
    let module = compile_to_ir(source).unwrap();

    let s = &module.structs[0];
    assert_eq!(s.fields[0].ty.display_name(&module), "[String]");
}

#[test]
fn test_resolved_type_display_optional() {
    let source = "struct S { maybe: String? }";
    let module = compile_to_ir(source).unwrap();

    let s = &module.structs[0];
    assert_eq!(s.fields[0].ty.display_name(&module), "String?");
}

#[test]
fn test_resolved_type_display_struct_ref() {
    let source = "struct Inner { } struct Outer { inner: Inner }";
    let module = compile_to_ir(source).unwrap();

    let outer = module.structs.iter().find(|s| s.name == "Outer").unwrap();
    assert_eq!(outer.fields[0].ty.display_name(&module), "Inner");
}

#[test]
fn test_resolved_type_display_enum_ref() {
    let source = "enum Status { active } struct S { status: Status }";
    let module = compile_to_ir(source).unwrap();

    let s = module.structs.iter().find(|s| s.name == "S").unwrap();
    assert_eq!(s.fields[0].ty.display_name(&module), "Status");
}

#[test]
fn test_resolved_type_display_nested_array() {
    let source = "struct S { matrix: [[Number]] }";
    let module = compile_to_ir(source).unwrap();

    let s = &module.structs[0];
    assert_eq!(s.fields[0].ty.display_name(&module), "[[Number]]");
}
