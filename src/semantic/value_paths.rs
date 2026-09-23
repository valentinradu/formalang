//! Separate an enum instantiation from a use of a value.
//!
//! The parser reads `Name.variant` and `Name.variant(label: value)` as
//! an enum instantiation when `Name` starts with an uppercase letter.
//! The same text is a field access or a method call when `Name` is a
//! value: `G.slice(start: 1, end: 3)` on a `let G: String`. The parser
//! cannot tell the two apart, because an enum can come from a `use`.
//!
//! This pass runs after the symbol table is complete. It changes each
//! `EnumInstantiation` whose name is not a type or a trait into the
//! expression that the text means for a value. Types are module-level
//! names, so the pass needs no scope: a local binding cannot be a type.
//! When a local binding has the name of an enum, the enum wins. A type
//! declared in an inline `mod` counts too, because the code in that
//! module names it without the module prefix.

use super::symbol_table::SymbolTable;
use crate::ast::{
    BlockStatement, Definition, Expr, File, FnParam, MatchArm, Statement, StructField,
};
use std::collections::HashSet;

/// The names that a type can have: the symbol table, which holds the
/// imports, and each type that the file declares, at the top level or
/// in an inline `mod`.
struct TypeNames<'a> {
    symbols: &'a SymbolTable,
    in_file: HashSet<String>,
}

/// Rewrite each value path in `file` that the parser read as an enum
/// instantiation.
pub(super) fn rewrite_value_paths(file: &mut File, symbols: &SymbolTable) {
    let mut in_file = HashSet::new();
    for statement in &file.statements {
        if let Statement::Definition(def) = statement {
            collect_types(std::slice::from_ref(&**def), &mut in_file);
        }
    }
    let types = TypeNames { symbols, in_file };
    for statement in &mut file.statements {
        match statement {
            Statement::Use(_) => {}
            Statement::Let(binding) => rewrite_expr(&mut binding.value, &types),
            Statement::Definition(def) => rewrite_definition(def, &types),
        }
    }
}

/// Add the name of each struct, enum and trait in `definitions`, and in
/// each module nested inside, to `names`.
fn collect_types(definitions: &[Definition], names: &mut HashSet<String>) {
    for def in definitions {
        match def {
            Definition::Struct(s) => {
                names.insert(s.name.name.clone());
            }
            Definition::Enum(e) => {
                names.insert(e.name.name.clone());
            }
            Definition::Trait(t) => {
                names.insert(t.name.name.clone());
            }
            Definition::Module(m) => collect_types(&m.definitions, names),
            Definition::Impl(_) | Definition::Function(_) => {}
        }
    }
}

fn rewrite_definition(def: &mut Definition, types: &TypeNames<'_>) {
    match def {
        Definition::Trait(t) => {
            for method in &mut t.methods {
                rewrite_params(&mut method.params, types);
            }
        }
        Definition::Struct(s) => rewrite_fields(&mut s.fields, types),
        Definition::Impl(i) => {
            for f in &mut i.functions {
                rewrite_params(&mut f.params, types);
                if let Some(body) = &mut f.body {
                    rewrite_expr(body, types);
                }
            }
        }
        Definition::Enum(_) => {}
        Definition::Module(m) => {
            for inner in &mut m.definitions {
                rewrite_definition(inner, types);
            }
        }
        Definition::Function(f) => {
            rewrite_params(&mut f.params, types);
            if let Some(body) = &mut f.body {
                rewrite_expr(body, types);
            }
        }
    }
}

fn rewrite_params(params: &mut [FnParam], types: &TypeNames<'_>) {
    for param in params {
        if let Some(default) = &mut param.default {
            rewrite_expr(default, types);
        }
    }
}

fn rewrite_fields(fields: &mut [StructField], types: &TypeNames<'_>) {
    for field in fields {
        if let Some(default) = &mut field.default {
            rewrite_expr(default, types);
        }
    }
}

/// True when `name` can only be a value: it is one path segment, and
/// no type or trait has that name.
fn names_a_value(name: &str, types: &TypeNames<'_>) -> bool {
    !name.contains("::")
        && !types.symbols.is_type(name)
        && !types.symbols.is_trait(name)
        && !types.in_file.contains(name)
}

fn rewrite_expr(expr: &mut Expr, types: &TypeNames<'_>) {
    if let Expr::EnumInstantiation {
        enum_name, span, ..
    } = expr
    {
        if names_a_value(&enum_name.name, types) {
            let span = *span;
            let placeholder = Expr::Reference {
                path: Vec::new(),
                span,
            };
            if let Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } = std::mem::replace(expr, placeholder)
            {
                *expr = if data.is_empty() {
                    // `Name.field`, the same shape as `name.field`.
                    Expr::Reference {
                        path: vec![enum_name, variant],
                        span,
                    }
                } else {
                    // `Name.method(label: value, ...)`.
                    let receiver_span = enum_name.span;
                    Expr::MethodCall {
                        receiver: Box::new(Expr::Reference {
                            path: vec![enum_name],
                            span: receiver_span,
                        }),
                        method: variant,
                        args: data
                            .into_iter()
                            .map(|(label, value)| (Some(label), value))
                            .collect(),
                        span,
                    }
                };
            }
        }
    }
    rewrite_children(expr, types);
}

fn rewrite_children(expr: &mut Expr, types: &TypeNames<'_>) {
    match expr {
        Expr::Literal { .. } | Expr::Reference { .. } => {}
        Expr::Invocation { args, .. } => {
            for (_, arg) in args {
                rewrite_expr(arg, types);
            }
        }
        Expr::MethodCall { receiver, args, .. } => {
            rewrite_expr(receiver, types);
            for (_, arg) in args {
                rewrite_expr(arg, types);
            }
        }
        Expr::EnumInstantiation { data, .. }
        | Expr::InferredEnumInstantiation { data, .. }
        | Expr::Tuple { fields: data, .. } => {
            for (_, value) in data {
                rewrite_expr(value, types);
            }
        }
        Expr::Array { elements, .. } => {
            for element in elements {
                rewrite_expr(element, types);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            rewrite_expr(left, types);
            rewrite_expr(right, types);
        }
        Expr::UnaryOp { operand: inner, .. }
        | Expr::Group { expr: inner, .. }
        | Expr::ClosureExpr { body: inner, .. }
        | Expr::FieldAccess { object: inner, .. } => rewrite_expr(inner, types),
        Expr::ForExpr {
            collection, body, ..
        } => {
            rewrite_expr(collection, types);
            rewrite_expr(body, types);
        }
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rewrite_expr(condition, types);
            rewrite_expr(then_branch, types);
            if let Some(else_branch) = else_branch {
                rewrite_expr(else_branch, types);
            }
        }
        Expr::MatchExpr {
            scrutinee, arms, ..
        } => {
            rewrite_expr(scrutinee, types);
            for MatchArm { body, .. } in arms {
                rewrite_expr(body, types);
            }
        }
        Expr::DictLiteral { entries, .. } => {
            for (key, value) in entries {
                rewrite_expr(key, types);
                rewrite_expr(value, types);
            }
        }
        Expr::DictAccess { dict, key, .. } => {
            rewrite_expr(dict, types);
            rewrite_expr(key, types);
        }
        Expr::LetExpr { value, body, .. } => {
            rewrite_expr(value, types);
            rewrite_expr(body, types);
        }
        Expr::Block {
            statements, result, ..
        } => {
            for statement in statements {
                match statement {
                    BlockStatement::Let { value, .. } | BlockStatement::Expr(value) => {
                        rewrite_expr(value, types);
                    }
                    BlockStatement::Assign { target, value, .. } => {
                        rewrite_expr(target, types);
                        rewrite_expr(value, types);
                    }
                }
            }
            rewrite_expr(result, types);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_value_paths;
    use crate::ast::{Definition, Expr, Statement};
    use crate::semantic::symbol_table::SymbolTable;

    /// The body of the function `name` in module `module`, or at the
    /// top level when `module` is `None`, after the rewrite.
    fn body_after_rewrite(
        source: &str,
        module: Option<&str>,
        name: &str,
    ) -> Result<Expr, Box<dyn std::error::Error>> {
        let mut file = crate::parse_only(source).map_err(|e| format!("{e:?}"))?;
        rewrite_value_paths(&mut file, &SymbolTable::new());
        let definitions: Vec<&Definition> = file
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Definition(def) => Some(&**def),
                Statement::Use(_) | Statement::Let(_) => None,
            })
            .collect();
        let scope: Vec<&Definition> = match module {
            None => definitions,
            Some(module) => definitions
                .iter()
                .find_map(|def| {
                    if let Definition::Module(m) = def {
                        if m.name.name == module {
                            return Some(m.definitions.iter().collect());
                        }
                    }
                    None
                })
                .ok_or("no such module")?,
        };
        scope
            .iter()
            .find_map(|def| {
                if let Definition::Function(f) = def {
                    if f.name.name == name {
                        return f.body.clone();
                    }
                }
                None
            })
            .ok_or_else(|| "no such function".into())
    }

    #[test]
    fn a_value_name_becomes_a_method_call() -> Result<(), Box<dyn std::error::Error>> {
        let body = body_after_rewrite(
            "pub fn f(G: String) -> String { G.slice(start: 1, end: 3) }",
            None,
            "f",
        )?;
        if !matches!(body, Expr::MethodCall { .. }) {
            return Err(format!("expected a method call, got {body:?}").into());
        }
        Ok(())
    }

    #[test]
    fn an_enum_of_an_inline_module_stays_an_enum_instantiation(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "pub mod m {\n  pub enum E { a, b(n: I32) }\n  pub fn pick() -> E { E.a }\n  pub fn pick2() -> E { E.b(n: 1) }\n}";
        for name in ["pick", "pick2"] {
            let body = body_after_rewrite(source, Some("m"), name)?;
            if !matches!(body, Expr::EnumInstantiation { .. }) {
                return Err(format!("{name}: expected an enum instantiation, got {body:?}").into());
            }
        }
        Ok(())
    }

    #[test]
    fn a_top_level_enum_stays_an_enum_instantiation() -> Result<(), Box<dyn std::error::Error>> {
        let body =
            body_after_rewrite("pub enum E { a }\npub fn pick() -> E { E.a }", None, "pick")?;
        // The symbol table is empty here, so the enum is known only
        // from the file.
        if matches!(body, Expr::EnumInstantiation { .. }) {
            return Ok(());
        }
        Err(format!("expected an enum instantiation, got {body:?}").into())
    }
}
