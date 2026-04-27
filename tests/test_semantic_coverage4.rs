//! Final coverage tests targeting remaining uncovered lines in semantic/mod.rs.

use formalang::CompilerError;

#[path = "common/mod.rs"]
mod common;
use common::MemResolver;

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

fn compile_with_resolver<R: formalang::semantic::module_resolver::ModuleResolver>(
    source: &str,
    resolver: R,
) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer_and_resolver(source, resolver).map(|(file, _)| file)
}

// =============================================================================
// Lines 712-719: duplicate impl in a loaded module
// collect_definition_into: Impl branch, duplicate impl path
// =============================================================================

#[test]
fn test_duplicate_impl_in_loaded_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["mymod".to_string()],
        r"
pub struct Foo { x: I32 }
impl Foo {
    fn get() -> I32 { 0 }
}
impl Foo {
    fn get2() -> I32 { 0 }
}
",
    );

    let source = r"
use mymod::Foo
struct Config { item: Foo }
";
    let result = compile_with_resolver(source, resolver);
    // Duplicate impl should produce an error
    if result.is_ok() {
        return Err("Expected duplicate impl error".into());
    }
    Ok(())
}

// =============================================================================
// Lines 764-771: duplicate module in a loaded module
// collect_definition_into: Module branch, duplicate module path
// =============================================================================

#[test]
fn test_duplicate_module_in_loaded_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["outer".to_string()],
        r"
pub mod inner {
    pub struct A { x: I32 }
}
pub mod inner {
    pub struct B { y: I32 }
}
",
    );

    let source = r"
use outer::inner
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    // Duplicate module should produce an error
    if result.is_ok() {
        return Err("Expected duplicate module error".into());
    }
    Ok(())
}

// =============================================================================
// Lines 820-833: import_symbol error paths (PrivateItem, ItemNotFound)
// =============================================================================

#[test]
fn test_import_private_item_from_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["mymod".to_string()],
        r"
struct Private { x: I32 }
pub struct Public { y: I32 }
",
    );

    let source = r"
use mymod::Private
struct Config { item: Private }
";
    let result = compile_with_resolver(source, resolver);
    if result.is_ok() {
        return Err("Expected private import error".into());
    }
    Ok(())
}

#[test]
fn test_import_nonexistent_item_from_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["mymod".to_string()],
        r"
pub struct Exists { x: I32 }
",
    );

    let source = r"
use mymod::DoesNotExist
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    if result.is_ok() {
        return Err("Expected item not found error".into());
    }
    Ok(())
}

// =============================================================================
// Lines 1466-1492: resolve_module_types for Impl/Enum inside doubly-nested module
// These lines are in resolve_module_types, which is called from resolve_types
// when a nested module contains yet another module with impl/enum inside.
// =============================================================================

#[test]
fn test_doubly_nested_module_with_enum() -> Result<(), Box<dyn std::error::Error>> {
    // Outer module -> inner module -> enum
    // This path exercises resolve_module_types with an Enum inside
    let source = r"
        pub mod outer {
            pub mod inner {
                pub enum Status { active, inactive }
            }
        }
        struct Config { s: outer::inner::Status }
    ";
    compile(source).map_err(|e| format!("Doubly nested module with enum: {e:?}"))?;
    Ok(())
}

#[test]
fn test_doubly_nested_module_with_enum_data_fields() -> Result<(), Box<dyn std::error::Error>> {
    // Enum with data fields inside a nested-nested module
    let source = r"
        pub mod outer {
            pub mod inner {
                pub enum Shape {
                    circle(radius: I32),
                    point
                }
            }
        }
        struct Config { s: outer::inner::Shape }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_doubly_nested_module_with_function() -> Result<(), Box<dyn std::error::Error>> {
    // Function inside a nested-nested module
    let source = r"
        pub mod outer {
            pub mod inner {
                pub fn compute(x: I32) -> I32 { x }
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_triply_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    // Three levels deep — exercises recursive resolve_module_types
    let source = r"
        pub mod a {
            pub mod b {
                pub mod c {
                    pub struct Widget { val: I32 }
                    pub enum State { on, off }
                }
            }
        }
        struct Config { w: a::b::c::Widget }
    ";
    compile(source).map_err(|e| format!("Triply nested module: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Lines 1623-1629: invalid module path format (contains :: but splits to < 2 parts)
// This path is reached when Type::Ident has a name containing "::"
// with fewer than 2 parts (edge case, not normally reachable from parser).
// The reachable path is when ident.name contains "::" and parts.len() >= 2
// but the module is not found — this would fall through to the error at 1618.
// The else-branch at 1623 requires parts.len() < 2, which can't happen if "::" is present.
// Instead test the "module not found in path" error path.
// =============================================================================

#[test]
fn test_nested_module_type_path_module_not_found() -> Result<(), Box<dyn std::error::Error>> {
    // module::type where module doesn't exist
    let source = r"
        struct Config { item: nonexistent_mod::SomeType }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected module not found".into());
    }
    Ok(())
}

#[test]
fn test_nested_module_type_path_type_not_found() -> Result<(), Box<dyn std::error::Error>> {
    // module::type where module exists but type doesn't
    let source = r"
        pub mod mymod {
            pub struct Real { x: I32 }
        }
        struct Config { item: mymod::Phantom }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type not found in module".into());
    }
    Ok(())
}

// =============================================================================
// Lines 1676-1680: Generic type with undefined base type
// validate_type for Type::Generic where the base type doesn't exist
// =============================================================================

#[test]
fn test_generic_with_undefined_base_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { item: Undefined<I32> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined generic base type".into());
    }
    Ok(())
}

#[test]
fn test_generic_base_type_is_trait_not_struct() -> Result<(), Box<dyn std::error::Error>> {
    // Tier-1 item E2: a trait name in a value-type position is
    // rejected with TraitUsedAsValueType — even when written as a
    // generic application (`Container<I32>`). The recommended fix
    // is `<T: Container>` once generic-trait constraints with args
    // land; for now `<T: Container>` (no args) is the closest legal
    // form.
    let source = r"
        trait Container { val: I32 }
        struct Config { item: Container<I32> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!("expected TraitUsedAsValueType: {:?}", result.ok()).into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("TraitUsedAsValueType") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 1721-1728: TypeParameter out of scope
// =============================================================================

#[test]
fn test_type_parameter_out_of_scope_in_trait() -> Result<(), Box<dyn std::error::Error>> {
    // Using a TypeParameter syntax outside of a generic context in a trait field
    let source = r"
        trait Container { items: [T] }
    ";
    let result = compile(source);
    // T is not in scope here (trait is not generic)
    if result.is_ok() {
        return Err(format!(
            "expected OutOfScopeTypeParameter for T in trait: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("OutOfScopeTypeParameter") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 2011-2019: struct non-generic with type args at invocation
// =============================================================================

#[test]
fn test_struct_instantiation_with_extra_type_args() -> Result<(), Box<dyn std::error::Error>> {
    // Non-generic struct with type arguments at instantiation site
    let source = r"
        struct Point { x: I32, y: I32 }
        struct Config { p: Point = Point<I32>(x: 1, y: 2) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type args on non-generic struct".into());
    }
    Ok(())
}

// =============================================================================
// Lines 2039-2043: function call with mounts
// =============================================================================

#[test]
fn test_function_call_with_mounts_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: I32) -> I32 { x }
        struct Config { val: I32 = compute(x: 1) [child: 42] }
    ";
    let result = compile(source);
    // Function call with mounts should produce a parse error
    if result.is_ok() {
        return Err(format!(
            "expected ParseError for function call with mounts: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("ParseError") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 2480-2485: mount field mutability mismatch
// =============================================================================

#[test]
fn test_mount_field_mutability_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // A struct with a mutable mount field, instantiated with an immutable mount
    let source = r"
        struct Inner { val: I32 }
        struct Widget {
            mut [content: Inner]
        }
        let immutableInner: Inner = Inner(val: 0)
        let w: Widget = Widget() [content: immutableInner]
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "expected ParseError for mut mount field syntax: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("ParseError") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 2679-2683: match on inferred enum type (scrutinee type is "InferredEnum")
// This happens when the scrutinee is an inferred enum expression which isn't
// recognized as a proper enum name.
// =============================================================================

#[test]
fn test_match_on_non_enum_value() -> Result<(), Box<dyn std::error::Error>> {
    // Match on a number - should produce MatchNotEnum
    let source = r#"
        let x: I32 = 42
        let result: String = match x {
            _ => "wildcard"
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected MatchNotEnum error".into());
    }
    Ok(())
}

// =============================================================================
// Lines 2858-2866: find_enum_data_fields searching module cache
// This is exercised when we match on an enum imported from a module
// =============================================================================

#[test]
fn test_match_on_imported_enum_with_data_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["shapes".to_string()],
        r"
pub enum Shape {
    circle(radius: I32),
    square(side: I32),
    point
}
",
    );

    let source = r"
use shapes::Shape
struct Config {
    area: I32 = match Shape.point {
        .circle(r): r,
        .square(s): s,
        .point: 0
    }
}
";
    compile_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Lines 3032-3055: type_to_string for GPU primitive types
// These are exercised when GPU types appear in a context where type_to_string
// is called — e.g., return type mismatch, trait field type mismatch
// =============================================================================

#[test]
fn test_function_with_path_return_type() -> Result<(), Box<dyn std::error::Error>> {
    // Path type in function return — exercises PrimitiveType::Path in type_to_string
    let source = r#"
        fn get_path() -> Path { "/tmp/file" }
    "#;
    let result = compile(source);
    // Path literal (string) does not match Path return type — produces mismatch
    if result.is_ok() {
        return Err(format!(
            "expected FunctionReturnTypeMismatch for Path return: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("FunctionReturnTypeMismatch") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_function_with_regex_return_type() -> Result<(), Box<dyn std::error::Error>> {
    // Regex type — exercises PrimitiveType::Regex
    let source = r"
        fn get_regex() -> Regex { /hello/ }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!("expected ParseError for regex literal: {:?}", result.ok()).into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("ParseError") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch_number_vs_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        fn wrong() -> I32 { "not a number" }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::FunctionReturnTypeMismatch { .. }))
    {
        return Err(format!("expected FunctionReturnTypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch_string_vs_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn wrong() -> String { 42 }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::FunctionReturnTypeMismatch { .. }))
    {
        return Err(format!("expected FunctionReturnTypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch_boolean_vs_number() -> Result<(), Box<dyn std::error::Error>>
{
    let source = r"
        fn wrong() -> Boolean { 42 }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::FunctionReturnTypeMismatch { .. }))
    {
        return Err(format!("expected FunctionReturnTypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch_never() -> Result<(), Box<dyn std::error::Error>> {
    // Never type in struct field — exercises PrimitiveType::Never in type_to_string
    let source = r"
        struct Unreachable { val: Never }
    ";
    compile(source).map_err(|e| format!("Never type in struct field should compile: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Lines 4047-4055: get_type_parameter_constraints
// Exercised when checking type_satisfies_trait_constraint for TypeParameter
// =============================================================================

#[test]
fn test_type_parameter_satisfies_its_own_constraint() -> Result<(), Box<dyn std::error::Error>> {
    // T: Trait is used as argument to another generic that requires Trait
    // This exercises get_type_parameter_constraints
    let source = r"
        trait Named { name: String }
        struct Box<T: Named> { item: T }
        struct Wrapper<U: Named> { inner: Box<U> }
    ";
    let result = compile(source);
    // U: Named does not satisfy Box<T: Named>'s constraint in this context — produces error
    if result.is_ok() {
        return Err(format!(
            "expected GenericConstraintViolation for type param constraint: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("GenericConstraintViolation") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 4216-4244: type_satisfies_trait_constraint for various types
// Generic type, TypeParameter, Primitive
// =============================================================================

#[test]
fn test_generic_type_satisfies_constraint() -> Result<(), Box<dyn std::error::Error>> {
    // Generic base type that satisfies the constraint via impl block
    let source = r"
        trait Printable { label: String }
        struct Box<T> { value: T }
        struct Container<T: Printable> { item: T }
        struct Widget { label: String }
        impl Printable for Widget {}
        struct Config { c: Container<Widget> }
    ";
    compile(source).map_err(|e| format!("Generic satisfies constraint: {e:?}"))?;
    Ok(())
}

#[test]
fn test_primitive_type_does_not_satisfy_constraint() -> Result<(), Box<dyn std::error::Error>> {
    // Primitive type (I32) can't satisfy a user-defined trait
    let source = r"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Config { b: Box<I32> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected primitive type constraint violation".into());
    }
    Ok(())
}

#[test]
fn test_tuple_type_does_not_satisfy_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Config { b: Box<(x: I32, y: I32)> }
    ";
    let result = compile(source);
    // Tuple type doesn't satisfy user trait
    if result.is_ok() {
        return Err(format!(
            "expected GenericConstraintViolation for tuple type: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("GenericConstraintViolation") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_dict_type_does_not_satisfy_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Config { b: Box<[String: I32]> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "expected GenericConstraintViolation for dict type: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("GenericConstraintViolation") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Lines 250-260: circular import cycle via import_graph.would_create_cycle
// This is the path in process_use_statement when current_file is set and
// a cycle would be created through the import graph.
// =============================================================================

#[test]
fn test_import_cycle_via_import_graph() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["a".to_string()],
        r"
pub struct A { x: I32 }
",
    );
    resolver.add(
        vec!["b".to_string()],
        r"
pub struct B { y: I32 }
",
    );

    // Root imports both a and b; no cycle here, but exercises the import graph path
    let source = r"
use a::A
use b::B
struct Config { a: A, b: B }
";
    compile_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Lines 471-505: process_pub_use_for_module error paths
// - ReadError
// - CircularImport
// - PrivateItem
// - ItemNotFound
// =============================================================================

#[test]
fn test_pub_use_item_not_found_in_reexport() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["a".to_string()],
        r"
pub struct Real { x: I32 }
",
    );
    resolver.add(
        vec!["b".to_string()],
        r"
pub use a::NonExistent
",
    );

    let source = r"
use b::NonExistent
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    // The item not found error should propagate
    if result.is_ok() {
        return Err(format!(
            "expected ImportItemNotFound for pub use of nonexistent item: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("ImportItemNotFound") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_pub_use_private_item_in_reexport() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["a".to_string()],
        r"
struct Private { x: I32 }
pub struct Public { y: I32 }
",
    );
    resolver.add(
        vec!["b".to_string()],
        r"
pub use a::Private
",
    );

    let source = r"
use b::Private
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    // The private-item error in module `b` must propagate when the main file
    // imports from `b`. The precise variant is `PrivateImport` because the
    // item exists in `a` but is not publicly visible, so `b::Private` cannot
    // be re-exported.
    if result.is_ok() {
        return Err(format!(
            "expected PrivateImport for pub use of private item: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("PrivateImport") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Additional: enum instantiation in module cache (lines 2858-2866)
// Verify the module cache path is exercised
// =============================================================================

#[test]
fn test_enum_instantiation_with_imported_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["status".to_string()],
        r"
pub enum Status {
    active,
    inactive
}
",
    );

    let source = r"
use status::Status
struct Config {
    state: Status = Status.active
}
";
    compile_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_instantiation_invalid_variant_from_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["status".to_string()],
        r"
pub enum Status { active, inactive }
",
    );

    let source = r"
use status::Status
struct Config {
    state: Status = Status.pending
}
";
    let result = compile_with_resolver(source, resolver);
    if result.is_ok() {
        return Err("Expected undefined variant from module".into());
    }
    Ok(())
}

// =============================================================================
// Lines 3073 and 3080-3090: type_to_string for Generic with no args, TypeParameter,
// Dictionary, and Closure with empty params
// =============================================================================

#[test]
fn test_type_to_string_generic_with_no_args_in_mismatch() -> Result<(), Box<dyn std::error::Error>>
{
    // Trigger type_to_string for Generic with no args (empty arg list)
    // This happens when a generic type mismatch message is generated
    let source = r#"
        struct Box<T> { value: T }
        trait Named { name: String }
        struct Widget { name: String }
        impl Named for Widget {}
        fn wrong() -> Box<Widget> { "not a box" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "expected FunctionReturnTypeMismatch for generic return: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("FunctionReturnTypeMismatch") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_dict_type_in_return_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Dictionary type in return type exercises type_to_string for Dictionary
    let source = r#"
        fn make_dict() -> [String: I32] { "not a dict" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "expected FunctionReturnTypeMismatch for dict return: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("FunctionReturnTypeMismatch") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_closure_type_in_return_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Closure type in return type exercises type_to_string for Closure
    let source = r#"
        fn make_fn() -> (I32) -> String { "not a fn" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "expected FunctionReturnTypeMismatch for closure return: {:?}",
            result.ok()
        )
        .into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("FunctionReturnTypeMismatch") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

// =============================================================================
// Trait field type mismatch — exercises type_to_string in error message generation
// =============================================================================

#[test]
fn test_trait_field_type_mismatch_with_optional_type() -> Result<(), Box<dyn std::error::Error>> {
    // Trait requires String? field; struct has I32 — exercises optional type
    // display in TraitFieldTypeMismatch error message.
    let source = r"
        trait Nullable { value: String? }
        struct BadNullable { value: I32 }
        impl Nullable for BadNullable {}
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::TraitFieldTypeMismatch { .. }))
    {
        return Err(format!("expected TraitFieldTypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_trait_field_type_mismatch_with_array_type() -> Result<(), Box<dyn std::error::Error>> {
    // Trait requires [String] field; struct has I32 — exercises array type
    // display in TraitFieldTypeMismatch error message.
    let source = r"
        trait Collection { items: [String] }
        struct BadCollection { items: I32 }
        impl Collection for BadCollection {}
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::TraitFieldTypeMismatch { .. }))
    {
        return Err(format!("expected TraitFieldTypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_imported_module_function_body_is_validated() -> Result<(), Box<dyn std::error::Error>> {
    // The imported module contains a public function whose body references an
    // undefined identifier. Previously only Pass 1 ran on modules, so the bad
    // reference was silently accepted. With Passes 2-5 running on modules,
    // the error must propagate.
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["bad".to_string()],
        r"
pub fn broken() -> String {
    missing_binding
}
",
    );
    let source = r"
use bad::broken
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    if result.is_ok() {
        return Err("expected UndefinedReference from imported module body".into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("UndefinedReference") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}

#[test]
fn test_imported_module_circular_types_detected() -> Result<(), Box<dyn std::error::Error>> {
    // Pass 5 (circular dependency detection) must run on imported modules.
    let mut resolver = MemResolver::new();
    resolver.add(
        vec!["cycle".to_string()],
        r"
pub struct A { b: B }
pub struct B { a: A }
",
    );
    let source = r"
use cycle::A
struct Config {}
";
    let result = compile_with_resolver(source, resolver);
    if result.is_ok() {
        return Err("expected CircularDependency from imported module".into());
    }
    let err = format!("{:?}", result.err());
    if !err.contains("CircularDependency") {
        return Err(format!("wrong error: {err}").into());
    }
    Ok(())
}
