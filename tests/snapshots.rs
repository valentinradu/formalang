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
        let bump: (mut I32) -> I32 = (mut n) -> n
        let consume: (sink String) -> String = (sink s) -> s
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

/// Render a value with `Debug` and lexicographically sort any `{ "key":
/// ..., ... }` block whose keys are quoted strings. This is the
/// `HashMap` shape — Rust's `Debug` for `HashMap<String, _>` prints in
/// non-deterministic iteration order, which makes the snapshot flaky.
/// We post-process the rendering rather than reach into the IR module.
fn debug_with_sorted_string_maps<T: std::fmt::Debug>(value: &T) -> String {
    sort_string_keyed_blocks(&format!("{value:#?}"))
}

fn sort_string_keyed_blocks(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch == '{' {
            if let Some((end, sorted)) = try_sort_block(input, i) {
                out.push_str(&sorted);
                i = end;
                continue;
            }
        }
        out.push(ch);
        i = i.saturating_add(1);
    }
    out
}

/// If the brace at `start` opens a HashMap-shaped block (every entry is
/// `"<key>": ...`), return `(end, sorted)`. Else return None and the
/// caller leaves the block alone.
fn try_sort_block(input: &str, start: usize) -> Option<(usize, String)> {
    let bytes = input.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut end = start;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '{' || ch == '[' || ch == '(' {
            depth = depth.saturating_add(1);
        } else if ch == '}' || ch == ']' || ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 && ch == '}' {
                end = end.saturating_add(1);
                break;
            }
        }
        end = end.saturating_add(1);
    }
    let body = &input[start.saturating_add(1)..end.saturating_sub(1)];
    let trimmed_body = body.trim();
    if trimmed_body.is_empty() {
        // `{}` — nothing to sort, leave shape intact.
        return None;
    }
    let entries = split_top_level_commas(body);
    let mut keyed: Vec<(String, String)> = Vec::new();
    for entry in &entries {
        let trimmed = entry.trim_matches(|c: char| c.is_whitespace() || c == ',');
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with('"') {
            return None;
        }
        let after_open = &trimmed[1..];
        let key_end = after_open.find('"')?;
        let key = after_open[..key_end].to_string();
        // Recurse: the value side may itself contain HashMaps.
        keyed.push((key, sort_string_keyed_blocks(trimmed)));
    }
    if keyed.is_empty() {
        return None;
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut rebuilt = String::from("{\n");
    for (_, raw) in &keyed {
        rebuilt.push_str("    ");
        rebuilt.push_str(raw);
        rebuilt.push_str(",\n");
    }
    rebuilt.push('}');
    Some((end, rebuilt))
}

fn split_top_level_commas(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut last = 0usize;
    let mut out: Vec<&str> = Vec::new();
    for i in 0..bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == '{' || ch == '[' || ch == '(' {
            depth = depth.saturating_add(1);
        } else if ch == '}' || ch == ']' || ch == ')' {
            depth = depth.saturating_sub(1);
        } else if ch == ',' && depth == 0 {
            out.push(&body[last..i]);
            last = i.saturating_add(1);
        }
    }
    if last < body.len() {
        out.push(&body[last..]);
    }
    out
}

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
    insta::assert_snapshot!("ir_struct_and_impl", debug_with_sorted_string_maps(&module));
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
    insta::assert_snapshot!("ir_enum_and_match", debug_with_sorted_string_maps(&module));
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
    insta::assert_snapshot!("ir_trait_and_impl", debug_with_sorted_string_maps(&module));
}
