//! Foundational snapshot tests.
//!
//! A small set of representative programs; each is compiled to both the AST
//! and the IR, and the resulting `Debug` shape is captured with `insta`.
//! The goal is to catch structural regressions — e.g. someone accidentally
//! changes the AST/IR shape of a core construct — not to exhaustively record
//! every output.
//!
//! To regenerate snapshots after intentional AST/IR shape changes:
//!
//! ```text
//! INSTA_UPDATE=always cargo test --test snapshots
//! ```
//!
//! Or use `cargo insta review` if the `cargo-insta` CLI is installed.

#![allow(clippy::expect_used)]

use formalang::compile_to_ir;

// =============================================================================
// AST snapshots
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn ast_struct_and_impl() {
    let source = r"
        struct Point { x: I32, y: I32 }

        impl Point {
            fn magnitude(self) -> I32 {
                self.x + self.y
            }
        }
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_struct_and_impl", file);
}

#[test]
fn ast_enum_and_match() {
    let source = r"
        enum Status { active, inactive, pending(since: I32) }

        pub fn label(s: Status) -> I32 {
            match s {
                .active: 1,
                .inactive: 0,
                .pending(since): since
            }
        }
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_enum_and_match", file);
}

#[test]
fn ast_trait_and_impl() {
    let source = r"
        trait Area {
            fn area(self) -> I32
        }

        struct Square { side: I32 }

        impl Area for Square {
            fn area(self) -> I32 {
                self.side * self.side
            }
        }
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_trait_and_impl", file);
}

#[test]
fn ast_closure_with_mut_and_sink() {
    let source = r"
        let bump: mut I32 -> I32 = mut n -> n
        let consume: sink String -> String = sink s -> s
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_closure_with_mut_and_sink", file);
}

#[test]
fn ast_generic_struct_with_impl() {
    let source = r"
        struct Box<T> { value: T }

        impl Box<T> {
            fn get(self) -> T {
                self.value
            }
        }
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_generic_struct_with_impl", file);
}

#[test]
fn ast_module_with_path_access() {
    let source = r"
        pub mod shapes {
            pub struct Circle { radius: I32 }
        }

        let c = shapes::Circle(radius: 10)
    ";
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_module_with_path_access", file);
}

#[test]
fn ast_if_and_match_exprs() {
    let source = r#"
        enum Choice { yes, no }

        let pick: Choice = Choice.yes
        let label: String = match pick {
            .yes: "y",
            .no: "n"
        }
        let flag: I32 = if true { 1 } else { 0 }
    "#;
    let file = compile(source).expect("should compile");
    insta::assert_debug_snapshot!("ast_if_and_match_exprs", file);
}

// =============================================================================
// IR snapshots
// =============================================================================

#[test]
fn ir_struct_and_impl() {
    let source = r"
        struct Point { x: I32, y: I32 }

        impl Point {
            fn magnitude(self) -> I32 {
                self.x + self.y
            }
        }
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    insta::assert_debug_snapshot!("ir_struct_and_impl", module);
}

#[test]
fn ir_enum_and_match() {
    let source = r"
        enum Status { active, inactive, pending(since: I32) }

        pub fn label(s: Status) -> I32 {
            match s {
                .active: 1,
                .inactive: 0,
                .pending(since): since
            }
        }
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    insta::assert_debug_snapshot!("ir_enum_and_match", module);
}

#[test]
fn ir_trait_and_impl() {
    let source = r"
        trait Area {
            fn area(self) -> I32
        }

        struct Square { side: I32 }

        impl Area for Square {
            fn area(self) -> I32 {
                self.side * self.side
            }
        }
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    insta::assert_debug_snapshot!("ir_trait_and_impl", module);
}
