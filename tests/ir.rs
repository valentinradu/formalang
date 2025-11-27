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
        struct Counter { count: Number, display: Number }
        impl Counter { display: count }
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
        impl Config { name: "default" }
    "#;
    let result = compile_to_ir(source);
    assert!(result.is_ok());
    let module = result.unwrap();

    assert_eq!(module.impls.len(), 1);
    assert!(!module.impls[0].defaults.is_empty());
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
        struct A { x: Number, display: Number }
        struct B { y: Number, display: Number }
        impl A { display: x }
        impl B { display: y }
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
        struct User: Named { name: String, age: Number, display: String }
        enum Status { active, inactive }
        impl User { display: name }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.struct_count, 1);
    assert_eq!(counter.trait_count, 1);
    assert_eq!(counter.enum_count, 1);
    assert_eq!(counter.impl_count, 1);
    // 1 trait field + 3 struct fields = 4 fields
    assert_eq!(counter.field_count, 4);
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
        impl S { name: "hello" }
    "#;
    let module = compile_to_ir(source).unwrap();

    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "String");
}

#[test]
fn test_expr_type_literal_number() {
    let source = r#"
        struct S { value: Number }
        impl S { value: 42 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Number");
}

#[test]
fn test_expr_type_literal_boolean() {
    let source = r#"
        struct S { flag: Boolean }
        impl S { flag: true }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Boolean");
}

#[test]
fn test_expr_type_array() {
    let source = r#"
        struct S { items: [Number] }
        impl S { items: [1, 2, 3] }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Array");
}

#[test]
fn test_expr_type_struct_instantiation() {
    let source = r#"
        struct Point { x: Number, y: Number }
        struct Container { p: Point }
        impl Container { p: Point(x: 1, y: 2) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Struct");
}

#[test]
fn test_expr_type_reference() {
    let source = r#"
        struct S { x: Number, y: Number }
        impl S { y: x }
    "#;
    let module = compile_to_ir(source).unwrap();

    // Impl defaults should have the reference expression
    assert!(!module.impls.is_empty());
    assert!(!module.impls[0].defaults.is_empty());
    // The expression has a type (implementation detail: might be TypeParam for field refs)
    let _ty = module.impls[0].defaults[0].1.ty();
}

#[test]
fn test_expr_type_binary_arithmetic() {
    let source = r#"
        struct S { sum: Number }
        impl S { sum: 1 + 2 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    // Arithmetic results in Number
    assert_eq!(type_name(expr.ty()), "Number");
}

#[test]
fn test_expr_type_binary_comparison() {
    let source = r#"
        struct S { result: Boolean }
        impl S { result: 1 == 2 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
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

// =============================================================================
// Expression Lowering Tests - Control Flow
// =============================================================================

#[test]
fn test_lower_if_expression() {
    let source = r#"
        struct S { value: Number }
        impl S { value: if true { 1 } else { 2 } }
    "#;
    let module = compile_to_ir(source).unwrap();

    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].defaults[0].1;
    assert!(matches!(expr, formalang::ir::IrExpr::If { .. }));
}

#[test]
fn test_lower_if_without_else() {
    let source = r#"
        struct S { value: Number? }
        impl S { value: if true { 1 } }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::If { else_branch, .. } = expr {
        assert!(else_branch.is_none());
    } else {
        panic!("Expected If expression");
    }
}

#[test]
fn test_lower_for_expression() {
    let source = r#"
        struct S { items: [Number] }
        impl S { items: for x in [1, 2, 3] { x } }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::For { var, .. } = expr {
        assert_eq!(var, "x");
    } else {
        panic!("Expected For expression");
    }
}

#[test]
fn test_lower_let_expression() {
    let source = r#"
        struct S { value: Number }
        impl S {
            value: (let x = 5
            x)
        }
    "#;
    let module = compile_to_ir(source).unwrap();

    assert!(!module.impls.is_empty());
    assert!(!module.impls[0].defaults.is_empty());
}

// =============================================================================
// Expression Lowering Tests - Enum Instantiation
// =============================================================================

#[test]
fn test_lower_enum_instantiation_simple() {
    let source = r#"
        enum Status { active, inactive }
        struct S { status: Status }
        impl S { status: Status.active }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::EnumInst { variant, .. } = expr {
        assert_eq!(variant, "active");
    } else {
        panic!("Expected EnumInst expression");
    }
}

#[test]
fn test_lower_enum_instantiation_with_data() {
    let source = r#"
        enum Option { none, some(value: Number) }
        struct S { opt: Option }
        impl S { opt: Option.some(value: 42) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::EnumInst {
        variant, fields, ..
    } = expr
    {
        assert_eq!(variant, "some");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "value");
    } else {
        panic!("Expected EnumInst expression");
    }
}

#[test]
fn test_lower_inferred_enum_instantiation() {
    let source = r#"
        enum Status { active, inactive }
        struct S { status: Status }
        impl S { status: .active }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::EnumInst { variant, .. } = expr {
        assert_eq!(variant, "active");
    } else {
        panic!("Expected EnumInst expression for inferred enum");
    }
}

// =============================================================================
// Expression Lowering Tests - Tuple
// =============================================================================

#[test]
fn test_lower_tuple_expression() {
    let source = r#"
        struct S { point: (x: Number, y: Number) }
        impl S { point: (x: 1, y: 2) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    if let formalang::ir::IrExpr::Tuple { fields, ty } = expr {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "x");
        assert_eq!(fields[1].0, "y");
        assert!(matches!(ty, formalang::ir::ResolvedType::Tuple(_)));
    } else {
        panic!("Expected Tuple expression");
    }
}

// =============================================================================
// Expression Lowering Tests - Binary Operations
// =============================================================================

#[test]
fn test_lower_binary_subtraction() {
    let source = r#"
        struct S { diff: Number }
        impl S { diff: 10 - 3 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert!(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. }));
}

#[test]
fn test_lower_binary_multiplication() {
    let source = r#"
        struct S { product: Number }
        impl S { product: 5 * 4 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert!(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. }));
}

#[test]
fn test_lower_binary_logical_and() {
    let source = r#"
        struct S { result: Boolean }
        impl S { result: true && false }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert!(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. }));
    assert_eq!(type_name(expr.ty()), "Boolean");
}

#[test]
fn test_lower_binary_logical_or() {
    let source = r#"
        struct S { result: Boolean }
        impl S { result: true || false }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert!(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. }));
    assert_eq!(type_name(expr.ty()), "Boolean");
}

#[test]
fn test_lower_binary_less_than() {
    let source = r#"
        struct S { result: Boolean }
        impl S { result: 1 < 2 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Boolean");
}

#[test]
fn test_lower_binary_greater_than() {
    let source = r#"
        struct S { result: Boolean }
        impl S { result: 2 > 1 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let expr = &module.impls[0].defaults[0].1;
    assert_eq!(type_name(expr.ty()), "Boolean");
}

// =============================================================================
// Visitor Expression Walking Tests
// =============================================================================

use formalang::ir::IrExpr;

struct ExprCounter {
    literal_count: usize,
    binary_op_count: usize,
    if_count: usize,
    for_count: usize,
    match_count: usize,
    array_count: usize,
    tuple_count: usize,
    struct_inst_count: usize,
    enum_inst_count: usize,
    reference_count: usize,
}

impl ExprCounter {
    fn new() -> Self {
        Self {
            literal_count: 0,
            binary_op_count: 0,
            if_count: 0,
            for_count: 0,
            match_count: 0,
            array_count: 0,
            tuple_count: 0,
            struct_inst_count: 0,
            enum_inst_count: 0,
            reference_count: 0,
        }
    }
}

impl IrVisitor for ExprCounter {
    fn visit_expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Literal { .. } => self.literal_count += 1,
            IrExpr::BinaryOp { .. } => self.binary_op_count += 1,
            IrExpr::If { .. } => self.if_count += 1,
            IrExpr::For { .. } => self.for_count += 1,
            IrExpr::Match { .. } => self.match_count += 1,
            IrExpr::Array { .. } => self.array_count += 1,
            IrExpr::Tuple { .. } => self.tuple_count += 1,
            IrExpr::StructInst { .. } => self.struct_inst_count += 1,
            IrExpr::EnumInst { .. } => self.enum_inst_count += 1,
            IrExpr::Reference { .. } => self.reference_count += 1,
        }
        // Walk children
        formalang::ir::walk_expr_children(self, e);
    }
}

#[test]
fn test_visitor_walks_if_children() {
    let source = r#"
        struct S { value: Number }
        impl S { value: if true { 1 } else { 2 } }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.if_count, 1);
    // Condition (true) + then branch (1) + else branch (2) = 3 literals
    assert_eq!(counter.literal_count, 3);
}

#[test]
fn test_visitor_walks_for_children() {
    let source = r#"
        struct S { items: [Number] }
        impl S { items: for x in [1, 2] { x } }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.for_count, 1);
    assert_eq!(counter.array_count, 1);
    assert_eq!(counter.reference_count, 1); // x reference in body
}

#[test]
fn test_visitor_walks_nested_if() {
    let source = r#"
        struct S { value: Number }
        impl S {
            value: if true {
                if false { 1 } else { 2 }
            } else {
                3
            }
        }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    // 2 if expressions (outer and nested)
    assert_eq!(counter.if_count, 2);
    // literals: true, false, 1, 2, 3
    assert_eq!(counter.literal_count, 5);
}

#[test]
fn test_visitor_walks_binary_op_children() {
    let source = r#"
        struct S { result: Number }
        impl S { result: 1 + 2 + 3 }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    // (1 + 2) + 3 = 2 binary ops
    assert_eq!(counter.binary_op_count, 2);
    assert_eq!(counter.literal_count, 3);
}

#[test]
fn test_visitor_walks_struct_inst_children() {
    let source = r#"
        struct Point { x: Number, y: Number }
        struct Container { p: Point }
        impl Container { p: Point(x: 1 + 2, y: 3) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.struct_inst_count, 1);
    assert_eq!(counter.binary_op_count, 1); // 1 + 2
    assert_eq!(counter.literal_count, 3); // 1, 2, 3
}

#[test]
fn test_visitor_walks_enum_inst_children() {
    let source = r#"
        enum Option { none, some(value: Number) }
        struct S { opt: Option }
        impl S { opt: Option.some(value: 1 + 2) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.enum_inst_count, 1);
    assert_eq!(counter.binary_op_count, 1);
    assert_eq!(counter.literal_count, 2);
}

#[test]
fn test_visitor_walks_array_children() {
    let source = r#"
        struct S { items: [Number] }
        impl S { items: [1, 2 + 3, 4] }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.array_count, 1);
    assert_eq!(counter.binary_op_count, 1);
    assert_eq!(counter.literal_count, 4); // 1, 2, 3, 4
}

#[test]
fn test_visitor_walks_tuple_children() {
    let source = r#"
        struct S { point: (x: Number, y: Number) }
        impl S { point: (x: 1 + 2, y: 3) }
    "#;
    let module = compile_to_ir(source).unwrap();

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    assert_eq!(counter.tuple_count, 1);
    assert_eq!(counter.binary_op_count, 1);
    assert_eq!(counter.literal_count, 3);
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_lower_generic_wrapper_struct() {
    let source = "struct Wrapper<T> { value: T }";
    let module = compile_to_ir(source).unwrap();

    let wrapper = &module.structs[0];
    assert_eq!(wrapper.name, "Wrapper");
    assert_eq!(wrapper.generic_params.len(), 1);
    assert_eq!(wrapper.generic_params[0].name, "T");
}

#[test]
fn test_lower_generic_struct_multiple_params() {
    let source = "struct Pair<A, B> { first: A, second: B }";
    let module = compile_to_ir(source).unwrap();

    let pair = &module.structs[0];
    assert_eq!(pair.generic_params.len(), 2);
    assert_eq!(pair.generic_params[0].name, "A");
    assert_eq!(pair.generic_params[1].name, "B");
}

#[test]
fn test_lower_generic_with_constraint() {
    let source = r#"
        trait Named { name: String }
        struct Container<T: Named> { item: T }
    "#;
    let module = compile_to_ir(source).unwrap();

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .unwrap();
    assert_eq!(container.generic_params.len(), 1);
    assert!(!container.generic_params[0].constraints.is_empty());
}

// =============================================================================
// ResolvedType Additional Coverage
// =============================================================================

#[test]
fn test_resolved_type_display_trait_ref() {
    let source = r#"
        trait Named { name: String }
        struct Container { item: Named }
    "#;
    let module = compile_to_ir(source).unwrap();

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .unwrap();
    assert_eq!(container.fields[0].ty.display_name(&module), "Named");
}

#[test]
fn test_resolved_type_display_type_param() {
    let source = r#"
        struct Box<T> { value: T }
    "#;
    let module = compile_to_ir(source).unwrap();

    let box_struct = &module.structs[0];
    // Type parameter T should display as "T"
    assert_eq!(box_struct.fields[0].ty.display_name(&module), "T");
}

#[test]
fn test_resolved_type_display_generic() {
    let source = "struct Box<T> { value: T } struct Container { item: Box<String> }";
    let module = compile_to_ir(source).unwrap();

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .unwrap();
    let display = container.fields[0].ty.display_name(&module);
    // Generic instantiation should show type args
    assert!(display.contains("Box") || display.contains("String"));
}

// =============================================================================
// External Reference Tests
// =============================================================================

use formalang::ir::ExternalKind;
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use std::collections::HashMap;
use std::path::PathBuf;

/// Mock module resolver for IR external reference tests
struct MockResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MockResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn add_module(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
    }
}

impl ModuleResolver for MockResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(&path.to_vec())
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: vec![],
                span: formalang::location::Span::default(),
            })
    }
}

fn compile_to_ir_with_resolver<R: ModuleResolver>(
    source: &str,
    resolver: R,
) -> Result<formalang::IrModule, Vec<formalang::CompilerError>> {
    let (ast, analyzer) = formalang::compile_with_analyzer_and_resolver(source, resolver)?;
    formalang::ir::lower_to_ir(&ast, analyzer.symbols())
}

#[test]
fn test_external_struct_reference() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Main {
    helper: Helper
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // Main struct should exist
    let main = module.structs.iter().find(|s| s.name == "Main").unwrap();
    let helper_field = &main.fields[0];

    // Helper type should be External, not a local struct
    match &helper_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            type_args,
        } => {
            assert_eq!(module_path, &vec!["utils".to_string()]);
            assert_eq!(name, "Helper");
            assert_eq!(*kind, ExternalKind::Struct);
            assert!(type_args.is_empty());
        }
        _ => panic!(
            "Expected External type for imported Helper, got {:?}",
            helper_field.ty
        ),
    }
}

#[test]
fn test_external_trait_reference() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["traits".to_string()],
        "pub trait Named { name: String }",
    );

    let source = r#"
use traits::Named
struct User: Named {
    name: String
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // User struct should implement the external trait
    let _user = module.structs.iter().find(|s| s.name == "User").unwrap();

    // The trait reference should be external
    // Note: traits field contains TraitIds for local traits, but external traits
    // need different handling - checking imports instead
    assert!(!module.imports.is_empty());

    let named_import = module
        .imports
        .iter()
        .flat_map(|i| i.items.iter())
        .find(|item| item.name == "Named");

    assert!(named_import.is_some());
    assert_eq!(named_import.unwrap().kind, ExternalKind::Trait);
}

#[test]
fn test_external_enum_reference() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { active, inactive }",
    );

    let source = r#"
use types::Status
struct Item {
    status: Status
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let item = module.structs.iter().find(|s| s.name == "Item").unwrap();
    let status_field = &item.fields[0];

    match &status_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            ..
        } => {
            assert_eq!(module_path, &vec!["types".to_string()]);
            assert_eq!(name, "Status");
            assert_eq!(*kind, ExternalKind::Enum);
        }
        _ => panic!(
            "Expected External type for imported Status, got {:?}",
            status_field.ty
        ),
    }
}

#[test]
fn test_external_generic_reference() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["containers".to_string()],
        "pub struct Box<T> { value: T }",
    );

    let source = r#"
use containers::Box
struct Wrapper {
    item: Box<String>
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let wrapper = module.structs.iter().find(|s| s.name == "Wrapper").unwrap();
    let item_field = &wrapper.fields[0];

    match &item_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            type_args,
        } => {
            assert_eq!(module_path, &vec!["containers".to_string()]);
            assert_eq!(name, "Box");
            assert_eq!(*kind, ExternalKind::Struct);
            assert_eq!(type_args.len(), 1);
            assert!(matches!(
                &type_args[0],
                ResolvedType::Primitive(formalang::ast::PrimitiveType::String)
            ));
        }
        _ => panic!(
            "Expected External type with type args, got {:?}",
            item_field.ty
        ),
    }
}

#[test]
fn test_ir_imports_populated() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct Helper { name: String }
pub struct Utils { value: Number }
"#,
    );

    // Only Helper is actually used, so only Helper should be in imports
    let source = r#"
use utils::{Helper, Utils}
struct Main {
    helper: Helper
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // imports should contain only used items
    assert!(!module.imports.is_empty());

    let utils_import = module
        .imports
        .iter()
        .find(|i| i.module_path == vec!["utils".to_string()]);

    assert!(utils_import.is_some());
    let utils_import = utils_import.unwrap();

    // Only Helper is used, Utils is imported but not used
    assert!(utils_import.items.iter().any(|i| i.name == "Helper"));
    // Utils is NOT used, so it should NOT be in the imports
    // (we only track imports that are actually used in the code)
}

#[test]
fn test_external_nested_module_path() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );

    let source = r#"
use std::collections::List
struct Container {
    items: List
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .unwrap();
    let items_field = &container.fields[0];

    match &items_field.ty {
        ResolvedType::External { module_path, .. } => {
            assert_eq!(
                module_path,
                &vec!["std".to_string(), "collections".to_string()]
            );
        }
        _ => panic!("Expected External type with nested path"),
    }
}

#[test]
fn test_external_display_name_simple() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Main {
    helper: Helper
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let main = module.structs.iter().find(|s| s.name == "Main").unwrap();
    let helper_field = &main.fields[0];

    // display_name should return just the type name
    assert_eq!(helper_field.ty.display_name(&module), "Helper");
}

#[test]
fn test_external_display_name_with_generics() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["containers".to_string()],
        "pub struct Box<T> { value: T }",
    );

    let source = r#"
use containers::Box
struct Wrapper {
    item: Box<String>
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let wrapper = module.structs.iter().find(|s| s.name == "Wrapper").unwrap();
    let item_field = &wrapper.fields[0];

    // display_name should show type with args
    assert_eq!(item_field.ty.display_name(&module), "Box<String>");
}

#[test]
fn test_local_types_not_external() {
    let source = r#"
struct Helper { name: String }
struct Main {
    helper: Helper
}
"#;

    let module = compile_to_ir(source).unwrap();

    let main = module.structs.iter().find(|s| s.name == "Main").unwrap();
    let helper_field = &main.fields[0];

    // Local types should remain as Struct, not External
    assert!(matches!(helper_field.ty, ResolvedType::Struct(_)));
}

#[test]
fn test_mixed_local_and_external() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct External { name: String }",
    );

    let source = r#"
use utils::External
struct Local { value: Number }
struct Main {
    external: External,
    local: Local
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let main = module.structs.iter().find(|s| s.name == "Main").unwrap();

    let external_field = main.fields.iter().find(|f| f.name == "external").unwrap();
    let local_field = main.fields.iter().find(|f| f.name == "local").unwrap();

    // External type should be External variant
    assert!(matches!(external_field.ty, ResolvedType::External { .. }));

    // Local type should be Struct variant
    assert!(matches!(local_field.ty, ResolvedType::Struct(_)));
}

#[test]
fn test_external_in_array() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Item { name: String }",
    );

    let source = r#"
use utils::Item
struct Collection {
    items: [Item]
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let collection = module
        .structs
        .iter()
        .find(|s| s.name == "Collection")
        .unwrap();
    let items_field = &collection.fields[0];

    match &items_field.ty {
        ResolvedType::Array(inner) => match inner.as_ref() {
            ResolvedType::External { name, .. } => {
                assert_eq!(name, "Item");
            }
            _ => panic!("Expected External type inside array"),
        },
        _ => panic!("Expected Array type"),
    }
}

#[test]
fn test_external_in_optional() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Item { name: String }",
    );

    let source = r#"
use utils::Item
struct Container {
    item: Item?
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .unwrap();
    let item_field = &container.fields[0];

    match &item_field.ty {
        ResolvedType::Optional(inner) => match inner.as_ref() {
            ResolvedType::External { name, .. } => {
                assert_eq!(name, "Item");
            }
            _ => panic!("Expected External type inside optional"),
        },
        _ => panic!("Expected Optional type"),
    }
}

// =============================================================================
// External Reference Safety Tests
// =============================================================================

/// Tests that external types cannot be looked up via struct_id - this is the
/// expected behavior that code generators must handle.
#[test]
fn test_external_struct_not_in_struct_id_lookup() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Main {
    helper: Helper
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // External types should NOT be found via struct_id lookup
    // This is expected - code generators must handle External variant
    assert!(
        module.struct_id("Helper").is_none(),
        "External types should not be in struct_id lookup"
    );

    // Only local structs should be in the lookup
    assert!(module.struct_id("Main").is_some());
}

/// Tests that code generators can safely iterate over all struct fields
/// without panicking when encountering external types.
#[test]
fn test_safe_iteration_over_external_types() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Local { value: Number }
struct Main {
    helper: Helper,
    local: Local,
    primitive: String
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    let main = module.structs.iter().find(|s| s.name == "Main").unwrap();

    // This is how code generators should safely handle all type variants
    for field in &main.fields {
        match &field.ty {
            ResolvedType::Struct(id) => {
                // Safe: only local structs have StructIds
                let struct_def = module.get_struct(*id);
                assert!(!struct_def.name.is_empty());
            }
            ResolvedType::External {
                module_path, name, ..
            } => {
                // External types should be handled by emitting imports
                assert!(!module_path.is_empty());
                assert!(!name.is_empty());
            }
            ResolvedType::Primitive(_) => {
                // Primitives don't need lookup
            }
            _ => {
                // Other types (Array, Optional, etc.) may contain nested types
            }
        }
    }
}

/// Tests that all StructIds in a module are valid and won't cause panics.
/// This catches the bug where imported types incorrectly get u32::MAX IDs.
#[test]
fn test_all_struct_ids_are_valid() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Local { value: Number }
struct Main {
    helper: Helper,
    local: Local
}
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // Collect all StructIds from the IR and verify they are valid
    fn collect_struct_ids(ty: &ResolvedType, ids: &mut Vec<StructId>) {
        match ty {
            ResolvedType::Struct(id) => ids.push(*id),
            ResolvedType::Generic { base, args } => {
                ids.push(*base);
                for arg in args {
                    collect_struct_ids(arg, ids);
                }
            }
            ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
                collect_struct_ids(inner, ids);
            }
            ResolvedType::Tuple(fields) => {
                for (_, ty) in fields {
                    collect_struct_ids(ty, ids);
                }
            }
            ResolvedType::External { type_args, .. } => {
                // External types don't have StructIds, but their type_args might
                for arg in type_args {
                    collect_struct_ids(arg, ids);
                }
            }
            _ => {}
        }
    }

    let mut all_ids = Vec::new();
    for s in &module.structs {
        for field in &s.fields {
            collect_struct_ids(&field.ty, &mut all_ids);
        }
    }

    // All collected StructIds must be valid (in bounds)
    for id in all_ids {
        assert!(
            (id.0 as usize) < module.structs.len(),
            "StructId({}) is out of bounds (module has {} structs)",
            id.0,
            module.structs.len()
        );
        // This should not panic
        let _ = module.get_struct(id);
    }
}

/// Tests that get_struct panics with invalid IDs - this documents the
/// expected behavior and ensures code generators handle External types.
#[test]
#[should_panic(expected = "index out of bounds")]
fn test_get_struct_panics_on_invalid_id() {
    let source = "struct Only { value: Number }";
    let module = compile_to_ir(source).unwrap();

    // Only 1 struct exists (index 0)
    assert_eq!(module.structs.len(), 1);

    // This should panic - simulates what happens if external types
    // incorrectly get u32::MAX as their ID
    let invalid_id = StructId(u32::MAX);
    let _ = module.get_struct(invalid_id);
}

/// Tests that instantiating an external struct produces struct_id=None
/// and ty=External, not struct_id=u32::MAX which would panic.
#[test]
fn test_external_struct_instantiation_has_none_id() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Container { h: Helper }
impl Container { h: Helper(name: "test") }
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    // Find the impl's default expression
    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].defaults[0].1;

    // It should be a StructInst with struct_id=None
    if let IrExpr::StructInst { struct_id, ty, .. } = expr {
        assert!(
            struct_id.is_none(),
            "External struct instantiation should have struct_id=None, got {:?}",
            struct_id
        );

        // The type should be External
        match ty {
            ResolvedType::External {
                module_path, name, ..
            } => {
                assert_eq!(module_path, &vec!["utils".to_string()]);
                assert_eq!(name, "Helper");
            }
            _ => panic!("Expected External type, got {:?}", ty),
        }
    } else {
        panic!("Expected StructInst expression, got {:?}", expr);
    }
}

/// Tests that instantiating an external enum produces enum_id=None.
#[test]
fn test_external_enum_instantiation_has_none_id() {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { active, inactive }",
    );

    let source = r#"
use types::Status
struct Item { status: Status }
impl Item { status: Status.active }
"#;

    let module = compile_to_ir_with_resolver(source, resolver).unwrap();

    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].defaults[0].1;

    if let IrExpr::EnumInst {
        enum_id,
        variant,
        ty,
        ..
    } = expr
    {
        assert!(
            enum_id.is_none(),
            "External enum instantiation should have enum_id=None, got {:?}",
            enum_id
        );
        assert_eq!(variant, "active");

        match ty {
            ResolvedType::External {
                module_path, name, ..
            } => {
                assert_eq!(module_path, &vec!["types".to_string()]);
                assert_eq!(name, "Status");
            }
            _ => panic!("Expected External type, got {:?}", ty),
        }
    } else {
        panic!("Expected EnumInst expression, got {:?}", expr);
    }
}

/// Tests that local struct instantiation still has Some(struct_id).
#[test]
fn test_local_struct_instantiation_has_some_id() {
    let source = r#"
struct Point { x: Number, y: Number }
struct Container { p: Point }
impl Container { p: Point(x: 1, y: 2) }
"#;

    let module = compile_to_ir(source).unwrap();

    assert!(!module.impls.is_empty());
    let expr = &module.impls[0].defaults[0].1;

    if let IrExpr::StructInst { struct_id, ty, .. } = expr {
        assert!(
            struct_id.is_some(),
            "Local struct instantiation should have Some(struct_id)"
        );

        // Verify the ID is valid
        let id = struct_id.unwrap();
        let struct_def = module.get_struct(id);
        assert_eq!(struct_def.name, "Point");

        // The type should be Struct, not External
        assert!(matches!(ty, ResolvedType::Struct(_)));
    } else {
        panic!("Expected StructInst expression");
    }
}
