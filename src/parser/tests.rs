use super::*;
use crate::ast::{BindingPattern, Definition, Expr, Literal, PrimitiveType, Type};
use crate::lexer::Lexer;

fn parse_type_str(input: &str) -> Result<Type, Vec<(String, CustomSpan)>> {
    // Parse the type as a struct field and extract it
    let wrapper = format!("struct Test {{ field: {input} }}");
    let tokens = Lexer::tokenize_all(&wrapper);
    let result = parse_file(&tokens)?;

    // Extract the type from the parsed struct
    if let Some(Statement::Definition(def)) = result.statements.first() {
        if let Definition::Struct(s) = &**def {
            if let Some(field) = s.fields.first() {
                return Ok(field.ty.clone());
            }
        }
    }
    Err(vec![(
        "Could not extract type".to_string(),
        CustomSpan::default(),
    )])
}

#[test]
fn test_never_type_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("Never");
    if result.is_err() {
        return Err(format!("Failed to parse Never type: {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    if ty != Type::Primitive(PrimitiveType::Never) {
        return Err(format!("{:?} != {:?}", ty, Type::Primitive(PrimitiveType::Never)).into());
    }
    Ok(())
}

#[test]
fn test_specialized_numeric_type_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("I32", PrimitiveType::I32),
        ("I64", PrimitiveType::I64),
        ("F32", PrimitiveType::F32),
        ("F64", PrimitiveType::F64),
    ];
    for (source, expected) in cases {
        let ty = parse_type_str(source).map_err(|e| format!("{source}: {e:?}"))?;
        if ty != Type::Primitive(expected) {
            return Err(format!("{source}: {:?} != {:?}", ty, Type::Primitive(expected)).into());
        }
    }
    Ok(())
}

#[test]
fn test_never_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let input = r"
            pub struct Empty {
                body: Never
            }
        ";
    let tokens = Lexer::tokenize_all(input);
    let result = parse_file(&tokens);
    if result.is_err() {
        return Err(format!("Failed to parse struct with Never field: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_optional_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("Never?");
    if result.is_err() {
        return Err(format!("Failed to parse Never? type: {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Optional(inner) => {
            if *inner != Type::Primitive(PrimitiveType::Never) {
                return Err(format!(
                    "{:?} != {:?}",
                    *inner,
                    Type::Primitive(PrimitiveType::Never)
                )
                .into());
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. }
        | Type::Closure { .. } => return Err(format!("Expected Optional type, got {ty:?}").into()),
    }
    Ok(())
}

#[test]
fn test_array_of_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("[Never]");
    if result.is_err() {
        return Err(format!("Failed to parse [Never] type: {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Array(inner) => {
            if *inner != Type::Primitive(PrimitiveType::Never) {
                return Err(format!(
                    "{:?} != {:?}",
                    *inner,
                    Type::Primitive(PrimitiveType::Never)
                )
                .into());
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. }
        | Type::Closure { .. } => return Err(format!("Expected Array type, got {ty:?}").into()),
    }
    Ok(())
}

#[test]
fn test_dictionary_type_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("[String: I32]");
    if result.is_err() {
        return Err(format!("Failed to parse [String: I32] type: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Dictionary { key, value } => {
            if *key != Type::Primitive(PrimitiveType::String) {
                return Err(
                    format!("{:?} != {:?}", *key, Type::Primitive(PrimitiveType::String)).into(),
                );
            }
            if *value != Type::Primitive(PrimitiveType::I32) {
                return Err(
                    format!("{:?} != {:?}", *value, Type::Primitive(PrimitiveType::I32)).into(),
                );
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Closure { .. } => {
            return Err(format!("Expected Dictionary type, got {ty:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_dictionary_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let input = r"
            pub struct Config {
                settings: [String: String]
            }
        ";
    let tokens = Lexer::tokenize_all(input);
    let result = parse_file(&tokens);
    if result.is_err() {
        return Err(format!("Failed to parse struct with Dictionary field: : {result:?}").into());
    }

    let file = result.map_err(|e| format!("{e:?}"))?;
    if let Some(Statement::Definition(def)) = file.statements.first() {
        if let Definition::Struct(s) = &**def {
            if let Some(field) = s.fields.first() {
                match &field.ty {
                    Type::Dictionary { key, value } => {
                        if **key != Type::Primitive(PrimitiveType::String) {
                            return Err(format!(
                                "{:?} != {:?}",
                                **key,
                                Type::Primitive(PrimitiveType::String)
                            )
                            .into());
                        }
                        if **value != Type::Primitive(PrimitiveType::String) {
                            return Err(format!(
                                "{:?} != {:?}",
                                **value,
                                Type::Primitive(PrimitiveType::String)
                            )
                            .into());
                        }
                    }
                    Type::Primitive(_)
                    | Type::Ident(_)
                    | Type::Generic { .. }
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Tuple(_)
                    | Type::Closure { .. } => {
                        return Err(format!("Expected Dictionary type, got {:?}", field.ty).into())
                    }
                }
            } else {
                return Err("No fields found".into());
            }
        } else {
            return Err("No struct found".into());
        }
    } else {
        return Err("No definition found".into());
    }
    Ok(())
}

#[test]
fn test_nested_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("[String: [I32: Boolean]]");
    if result.is_err() {
        return Err(format!("Failed to parse nested dictionary type: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Dictionary { key, value } => {
            if *key != Type::Primitive(PrimitiveType::String) {
                return Err(
                    format!("{:?} != {:?}", *key, Type::Primitive(PrimitiveType::String)).into(),
                );
            }
            match *value {
                Type::Dictionary {
                    key: inner_key,
                    value: inner_value,
                } => {
                    if *inner_key != Type::Primitive(PrimitiveType::I32) {
                        return Err(format!(
                            "{:?} != {:?}",
                            *inner_key,
                            Type::Primitive(PrimitiveType::I32)
                        )
                        .into());
                    }
                    if *inner_value != Type::Primitive(PrimitiveType::Boolean) {
                        return Err(format!(
                            "{:?} != {:?}",
                            *inner_value,
                            Type::Primitive(PrimitiveType::Boolean)
                        )
                        .into());
                    }
                }
                Type::Primitive(_)
                | Type::Ident(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Closure { .. } => {
                    return Err(format!("Expected inner Dictionary type, got {value:?}").into())
                }
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Closure { .. } => {
            return Err(format!("Expected Dictionary type, got {ty:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_optional_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("[String: I32]?");
    if result.is_err() {
        return Err(format!("Failed to parse optional dictionary type: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Optional(inner) => match *inner {
            Type::Dictionary { key, value } => {
                if *key != Type::Primitive(PrimitiveType::String) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *key,
                        Type::Primitive(PrimitiveType::String)
                    )
                    .into());
                }
                if *value != Type::Primitive(PrimitiveType::I32) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *value,
                        Type::Primitive(PrimitiveType::I32)
                    )
                    .into());
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Closure { .. } => {
                return Err(
                    format!("Expected Dictionary type inside Optional, got {inner:?}").into(),
                )
            }
        },
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. }
        | Type::Closure { .. } => return Err(format!("Expected Optional type, got {ty:?}").into()),
    }
    Ok(())
}

#[test]
fn test_dictionary_with_custom_types() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("[UserId: UserData]");
    if result.is_err() {
        return Err(format!("Failed to parse dictionary with custom types: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Dictionary { key, value } => match (*key, *value) {
            (Type::Ident(k), Type::Ident(v)) => {
                if k.name != "UserId" {
                    return Err(format!("expected {:?} == {:?}", k.name, "UserId").into());
                }
                if v.name != "UserData" {
                    return Err(format!("expected {:?} == {:?}", v.name, "UserData").into());
                }
            }
            _ => return Err("Expected Ident types".into()),
        },
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Closure { .. } => {
            return Err(format!("Expected Dictionary type, got {ty:?}").into())
        }
    }
    Ok(())
}

// Helper to parse an expression from let binding
fn parse_expr_from_let(input: &str) -> Result<Expr, Vec<(String, CustomSpan)>> {
    let wrapper = format!("let x = {input}");
    let tokens = Lexer::tokenize_all(&wrapper);
    let result = parse_file(&tokens)?;

    if let Some(Statement::Let(binding)) = result.statements.first() {
        return Ok(binding.value.clone());
    }
    Err(vec![(
        "Could not extract expression".to_string(),
        CustomSpan::default(),
    )])
}

#[test]
fn test_dictionary_literal_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("[\"key\": 42, \"name\": 100]");
    if result.is_err() {
        return Err(format!("Failed to parse dictionary literal: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::DictLiteral { entries, .. } => {
            if entries.len() != 2 {
                return Err(format!("expected {:?} == {:?}", entries.len(), 2).into());
            }
            // Check first entry
            #[expect(
                clippy::indexing_slicing,
                reason = "bounds checked above: entries.len() == 2"
            )]
            let (first_key, first_val) = (&entries[0].0, &entries[0].1);
            match (first_key, first_val) {
                (
                    Expr::Literal {
                        value: Literal::String(k),
                        ..
                    },
                    Expr::Literal {
                        value: Literal::Number(v),
                        ..
                    },
                ) => {
                    if k != "key" {
                        return Err(format!("expected {:?} == {:?}", k, "key").into());
                    }
                    if (v.value.as_f64() - 42.0_f64).abs() > f64::EPSILON {
                        return Err(format!("expected {:?} == {:?}", v.value.as_f64(), 42.0).into());
                    }
                }
                _ => return Err("Expected string key and number value".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected DictLiteral, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_empty_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("[:]");
    if result.is_err() {
        return Err(format!("Failed to parse empty dictionary: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::DictLiteral { entries, .. } => {
            if !entries.is_empty() {
                return Err("Expected empty entries".into());
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected DictLiteral, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_dictionary_access_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("data[\"key\"]");
    if result.is_err() {
        return Err(format!("Failed to parse dictionary access: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
            (
                Expr::Reference { path, .. },
                Expr::Literal {
                    value: Literal::String(k),
                    ..
                },
            ) => {
                let first = path.first().ok_or("expected at least one path segment")?;
                if first.name != "data" {
                    return Err(format!("expected {:?} == {:?}", first.name, "data").into());
                }
                if k != "key" {
                    return Err(format!("expected {:?} == {:?}", k, "key").into());
                }
            }
            _ => return Err("Expected reference and string key".into()),
        },
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "match expression over all Expr variants — exhaustive arms cannot be extracted without losing context"
)]
fn test_chained_dictionary_access() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("data[\"outer\"][\"inner\"]");
    if result.is_err() {
        return Err(format!("Failed to parse chained dict access: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::DictAccess { dict, key, .. } => {
            // Outer access: dict is another DictAccess, key is "inner"
            match (*key,) {
                (Expr::Literal {
                    value: Literal::String(k),
                    ..
                },) => {
                    if k != "inner" {
                        return Err(format!("expected {:?} == {:?}", k, "inner").into());
                    }
                }
                _ => return Err("Expected string key 'inner'".into()),
            }
            match *dict {
                Expr::DictAccess {
                    dict: inner_dict,
                    key: inner_key,
                    ..
                } => {
                    match (*inner_key,) {
                        (Expr::Literal {
                            value: Literal::String(k),
                            ..
                        },) => {
                            if k != "outer" {
                                return Err(format!("expected {:?} == {:?}", k, "outer").into());
                            }
                        }
                        _ => return Err("Expected string key 'outer'".into()),
                    }
                    match *inner_dict {
                        Expr::Reference { path, .. } => {
                            let first = path.first().ok_or("expected at least one path segment")?;
                            if first.name != "data" {
                                return Err(
                                    format!("expected {:?} == {:?}", first.name, "data").into()
                                );
                            }
                        }
                        Expr::Literal { .. }
                        | Expr::Invocation { .. }
                        | Expr::EnumInstantiation { .. }
                        | Expr::InferredEnumInstantiation { .. }
                        | Expr::Array { .. }
                        | Expr::Tuple { .. }
                        | Expr::BinaryOp { .. }
                        | Expr::UnaryOp { .. }
                        | Expr::ForExpr { .. }
                        | Expr::IfExpr { .. }
                        | Expr::MatchExpr { .. }
                        | Expr::Group { .. }
                        | Expr::DictLiteral { .. }
                        | Expr::DictAccess { .. }
                        | Expr::FieldAccess { .. }
                        | Expr::ClosureExpr { .. }
                        | Expr::LetExpr { .. }
                        | Expr::MethodCall { .. }
                        | Expr::Block { .. } => return Err("Expected reference 'data'".into()),
                    }
                }
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::InferredEnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::Reference { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::LetExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => return Err("Expected inner DictAccess".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_dictionary_with_expression_key() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("data[index]");
    if result.is_err() {
        return Err(format!("Failed to parse dict access with expr key: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
            (Expr::Reference { path: d, .. }, Expr::Reference { path: k, .. }) => {
                let d0 = d
                    .first()
                    .ok_or("expected at least one segment in dict path")?;
                if d0.name != "data" {
                    return Err(format!("expected {:?} == {:?}", d0.name, "data").into());
                }
                let k0 = k
                    .first()
                    .ok_or("expected at least one segment in key path")?;
                if k0.name != "index" {
                    return Err(format!("expected {:?} == {:?}", k0.name, "index").into());
                }
            }
            _ => return Err("Expected two references".into()),
        },
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
    }
    Ok(())
}

// Closure type tests
#[test]
fn test_closure_type_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("() -> Event");
    if result.is_err() {
        return Err(format!("Failed to parse () -> Event: {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Closure { params, ret } => {
            if !params.is_empty() {
                return Err("Expected empty params".into());
            }
            match *ret {
                Type::Ident(ident) => {
                    if ident.name != "Event" {
                        return Err(format!("{:?} != {:?}", ident.name, "Event").into());
                    }
                }
                Type::Primitive(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Dictionary { .. }
                | Type::Closure { .. } => return Err("Expected Ident return type".into()),
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. } => {
            return Err(format!("Expected Closure type, got {ty:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("String -> Event");
    if result.is_err() {
        return Err(format!("Failed to parse String -> Event: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Closure { params, ret } => {
            if params.len() != 1 {
                return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
            }
            let (_, p0) = params.first().ok_or("expected at least one param")?;
            if *p0 != Type::Primitive(PrimitiveType::String) {
                return Err(
                    format!("{:?} != {:?}", p0, Type::Primitive(PrimitiveType::String)).into(),
                );
            }
            match *ret {
                Type::Ident(ident) => {
                    if ident.name != "Event" {
                        return Err(format!("{:?} != {:?}", ident.name, "Event").into());
                    }
                }
                Type::Primitive(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Dictionary { .. }
                | Type::Closure { .. } => return Err("Expected Ident return type".into()),
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. } => {
            return Err(format!("Expected Closure type, got {ty:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_closure_type_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("I32, I32 -> Point");
    if result.is_err() {
        return Err(format!("Failed to parse I32, I32 -> Point: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Closure { params, ret } => {
            if params.len() != 2 {
                return Err(format!("expected {:?} == {:?}", params.len(), 2).into());
            }
            let (_, p0) = params.first().ok_or("expected at least 1 param")?;
            if *p0 != Type::Primitive(PrimitiveType::I32) {
                return Err(
                    format!("{:?} != {:?}", p0, Type::Primitive(PrimitiveType::I32)).into(),
                );
            }
            let (_, p1) = params.get(1).ok_or("expected at least 2 params")?;
            if *p1 != Type::Primitive(PrimitiveType::I32) {
                return Err(
                    format!("{:?} != {:?}", p1, Type::Primitive(PrimitiveType::I32)).into(),
                );
            }
            match *ret {
                Type::Ident(ident) => {
                    if ident.name != "Point" {
                        return Err(format!("{:?} != {:?}", ident.name, "Point").into());
                    }
                }
                Type::Primitive(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Dictionary { .. }
                | Type::Closure { .. } => return Err("Expected Ident return type".into()),
            }
        }
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Optional(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. } => {
            return Err(format!("Expected Closure type, got {ty:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_optional_closure_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_type_str("(String -> Event)?");
    if result.is_err() {
        return Err(format!("Failed to parse (String -> Event)?: : {result:?}").into());
    }
    let ty = result.map_err(|e| format!("{e:?}"))?;
    match ty {
        Type::Optional(inner) => match *inner {
            Type::Closure { params, .. } => {
                if params.len() != 1 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. } => return Err("Expected Closure inside Optional".into()),
        },
        Type::Primitive(_)
        | Type::Ident(_)
        | Type::Generic { .. }
        | Type::Array(_)
        | Type::Tuple(_)
        | Type::Dictionary { .. }
        | Type::Closure { .. } => return Err(format!("Expected Optional type, got {ty:?}").into()),
    }
    Ok(())
}

// Closure expression tests
#[test]
fn test_closure_expr_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("() -> .submit");
    if result.is_err() {
        return Err(format!("Failed to parse () -> .submit: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::ClosureExpr { params, body, .. } => {
            if !params.is_empty() {
                return Err(format!("params should be empty, has {} items", params.len()).into());
            }
            match *body {
                Expr::InferredEnumInstantiation { variant, .. } => {
                    if variant.name != "submit" {
                        return Err(format!("expected {:?} == {:?}", variant.name, "submit").into());
                    }
                }
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::Reference { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::DictAccess { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::LetExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => {
                    return Err(format!("Expected InferredEnumInstantiation, got {body:?}").into())
                }
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected ClosureExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_closure_expr_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("x -> .changed(value: x)");
    if result.is_err() {
        return Err(format!("Failed to parse x -> .changed(...): : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::ClosureExpr { params, .. } => {
            if params.len() != 1 {
                return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
            }
            let p0 = params.first().ok_or("expected at least one param")?;
            if p0.name.name != "x" {
                return Err(format!("expected {:?} == {:?}", p0.name.name, "x").into());
            }
            if p0.ty.is_some() {
                return Err("params[0].ty should be None".into());
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected ClosureExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_closure_expr_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("w, h -> .resized(width: w, height: h)");
    if result.is_err() {
        return Err(format!("Failed to parse w, h -> ...: {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::ClosureExpr { params, .. } => {
            if params.len() != 2 {
                return Err(format!("expected {:?} == {:?}", params.len(), 2).into());
            }
            let p0 = params.first().ok_or("expected at least 1 param")?;
            if p0.name.name != "w" {
                return Err(format!("expected {:?} == {:?}", p0.name.name, "w").into());
            }
            let p1 = params.get(1).ok_or("expected at least 2 params")?;
            if p1.name.name != "h" {
                return Err(format!("expected {:?} == {:?}", p1.name.name, "h").into());
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected ClosureExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_closure_expr_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("x: String -> .textChanged(value: x)");
    if result.is_err() {
        return Err(format!("Failed to parse x: String -> ...: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::ClosureExpr { params, .. } => {
            if params.len() != 1 {
                return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
            }
            let p0 = params.first().ok_or("expected at least one param")?;
            if p0.name.name != "x" {
                return Err(format!("expected {:?} == {:?}", p0.name.name, "x").into());
            }
            match &p0.ty {
                Some(Type::Primitive(PrimitiveType::String)) => {}
                _ => return Err("Expected String type annotation".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::LetExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected ClosureExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_closure_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let input = r"
            pub struct Button<E> {
                action: () -> E
            }
        ";
    let tokens = Lexer::tokenize_all(input);
    let result = parse_file(&tokens);
    if result.is_err() {
        return Err(format!("Failed to parse struct with closure field: : {result:?}").into());
    }
    Ok(())
}

// Let expression tests
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "match expression over all Expr variants — exhaustive arms cannot be extracted without losing context"
)]
fn test_let_expr_basic() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("let x = 42 in x");
    if result.is_err() {
        return Err(format!("Failed to parse `let x = 42 in x`: {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::LetExpr {
            mutable,
            pattern,
            ty,
            value,
            body,
            ..
        } => {
            if mutable {
                return Err("expected !mutable, but was true".into());
            }
            match pattern {
                BindingPattern::Simple(ident) => {
                    if ident.name != "x" {
                        return Err(format!("{:?} != {:?}", ident.name, "x").into());
                    }
                }
                BindingPattern::Array { .. }
                | BindingPattern::Struct { .. }
                | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
            }
            if ty.is_some() {
                return Err(format!("ty should be None but got {ty:?}").into());
            }
            match *value {
                Expr::Literal {
                    value: Literal::Number(n),
                    ..
                } => {
                    if (n.value.as_f64() - 42.0_f64).abs() > f64::EPSILON {
                        return Err(format!("{:?} != {:?}", n.value.as_f64(), 42.0).into());
                    }
                }
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::InferredEnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::Reference { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::DictAccess { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::LetExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => return Err("Expected number literal".into()),
            }
            match *body {
                Expr::Reference { path, .. } => {
                    let first = path.first().ok_or("expected at least one path segment")?;
                    if first.name != "x" {
                        return Err(format!("{:?} != {:?}", first.name, "x").into());
                    }
                }
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::InferredEnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::DictAccess { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::LetExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => return Err("Expected reference in body".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_let_expr_with_type() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("let count: I32 = 100 in count");
    if result.is_err() {
        return Err(format!("Failed to parse let with type: : {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::LetExpr { pattern, ty, .. } => {
            match pattern {
                BindingPattern::Simple(ident) => {
                    if ident.name != "count" {
                        return Err(format!("{:?} != {:?}", ident.name, "count").into());
                    }
                }
                BindingPattern::Array { .. }
                | BindingPattern::Struct { .. }
                | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
            }
            match ty {
                Some(Type::Primitive(PrimitiveType::I32)) => {}
                _ => return Err("Expected I32 type annotation".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_let_expr_mutable() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("let mut counter = 0 in counter");
    if result.is_err() {
        return Err(format!("Failed to parse let mut: {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::LetExpr {
            mutable, pattern, ..
        } => {
            if !mutable {
                return Err("expected mutable to be true".into());
            }
            match pattern {
                BindingPattern::Simple(ident) => {
                    if ident.name != "counter" {
                        return Err(format!("{:?} != {:?}", ident.name, "counter").into());
                    }
                }
                BindingPattern::Array { .. }
                | BindingPattern::Struct { .. }
                | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_let_expr_in_for() -> Result<(), Box<dyn std::error::Error>> {
    let input = r"
            struct App {
                content: [String] = for item in items {
                    let formatted = item
                    Label(text: formatted)
                }
            }
        ";
    let tokens = Lexer::tokenize_all(input);
    let result = parse_file(&tokens);
    if result.is_err() {
        return Err(format!("Failed to parse let in for block: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_nested_let_exprs() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("let x = 1 in let y = 2 in x");
    if result.is_err() {
        return Err(format!("Failed to parse nested let: {result:?}").into());
    }
    let expr = result.map_err(|e| format!("{e:?}"))?;
    match expr {
        Expr::LetExpr { pattern, body, .. } => {
            match pattern {
                BindingPattern::Simple(ident) => {
                    if ident.name != "x" {
                        return Err(format!("{:?} != {:?}", ident.name, "x").into());
                    }
                }
                BindingPattern::Array { .. }
                | BindingPattern::Struct { .. }
                | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
            }
            match *body {
                Expr::LetExpr {
                    pattern: inner_pattern,
                    ..
                } => match inner_pattern {
                    BindingPattern::Simple(ident) => {
                        if ident.name != "y" {
                            return Err(format!("{:?} != {:?}", ident.name, "y").into());
                        }
                    }
                    BindingPattern::Array { .. }
                    | BindingPattern::Struct { .. }
                    | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
                },
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::InferredEnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::Reference { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::DictAccess { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => return Err("Expected nested LetExpr".into()),
            }
        }
        Expr::Literal { .. }
        | Expr::Invocation { .. }
        | Expr::EnumInstantiation { .. }
        | Expr::InferredEnumInstantiation { .. }
        | Expr::Array { .. }
        | Expr::Tuple { .. }
        | Expr::Reference { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::ForExpr { .. }
        | Expr::IfExpr { .. }
        | Expr::MatchExpr { .. }
        | Expr::Group { .. }
        | Expr::DictLiteral { .. }
        | Expr::DictAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::ClosureExpr { .. }
        | Expr::MethodCall { .. }
        | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
    }
    Ok(())
}

#[test]
fn test_block_expr_simple() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("{ let x = 1 x }");
    if result.is_err() {
        return Err(format!("Failed to parse block: {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_with_call() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_expr_from_let("{ let v = foo.bar(1) Result(value: v) }");
    if result.is_err() {
        return Err(format!("Failed to parse block with call: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_no_let() -> Result<(), Box<dyn std::error::Error>> {
    // Block with just a result expression (no let statements)
    let result = parse_expr_from_let("{ Result(value: 1) }");
    if result.is_err() {
        return Err(format!("Failed to parse block no let: {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_let_simple_then_call() -> Result<(), Box<dyn std::error::Error>> {
    // Block with let binding a literal, then a call
    let result = parse_expr_from_let("{ let v = 1 Result(value: v) }");
    if result.is_err() {
        return Err(format!("Failed to parse block let simple then call: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_let_field_access() -> Result<(), Box<dyn std::error::Error>> {
    // Block with let binding field access, then a reference
    let result = parse_expr_from_let("{ let v = foo.bar v }");
    if result.is_err() {
        return Err(format!("Failed to parse block let field access: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_let_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
    // Block with let binding a call, then a reference
    let result = parse_expr_from_let("{ let v = foo(1) v }");
    if result.is_err() {
        return Err(format!("Failed to parse block let call then ref: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_block_expr_let_method_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
    // Block with let binding a method call, then a reference
    let result = parse_expr_from_let("{ let v = foo.bar(1) v }");
    if result.is_err() {
        return Err(format!("Failed to parse block let method call then ref: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_let_expr_method_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
    // Let expression with method call value, then reference body
    // This uses the let EXPRESSION, not block statement
    let result = parse_expr_from_let("let v = foo.bar(1) in v");
    if result.is_err() {
        return Err(format!("Failed to parse let expr method call then ref: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_let_expr_fn_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
    // Let expression with function call value, then reference body
    let result = parse_expr_from_let("let v = foo(1) in v");
    if result.is_err() {
        return Err(format!("Failed to parse let expr fn call then ref: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_let_expr_field_access_then_ref() -> Result<(), Box<dyn std::error::Error>> {
    // Let expression with field access value, then reference body
    let result = parse_expr_from_let("let v = foo.bar in v");
    if result.is_err() {
        return Err(format!("Failed to parse let expr field access then ref: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_method_call_standalone() -> Result<(), Box<dyn std::error::Error>> {
    // Just a method call, no following expression
    let result = parse_expr_from_let("foo.bar(1)");
    if result.is_err() {
        return Err(format!("Failed to parse standalone method call: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_method_call_no_args() -> Result<(), Box<dyn std::error::Error>> {
    // Method call with no args
    let result = parse_expr_from_let("foo.bar()");
    if result.is_err() {
        return Err(format!("Failed to parse method call no args: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_field_access_standalone() -> Result<(), Box<dyn std::error::Error>> {
    // Field access (no parens)
    let result = parse_expr_from_let("foo.bar");
    if result.is_err() {
        return Err(format!("Failed to parse field access: {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_reference_standalone() -> Result<(), Box<dyn std::error::Error>> {
    // Just a reference
    let result = parse_expr_from_let("foo");
    if result.is_err() {
        return Err(format!("Failed to parse reference: {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_method_call_on_self() -> Result<(), Box<dyn std::error::Error>> {
    // Method call on self
    let result = parse_expr_from_let("self.bar(1)");
    if result.is_err() {
        return Err(format!("Failed to parse method call on self: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_method_call_on_this() -> Result<(), Box<dyn std::error::Error>> {
    // Method call on 'this' (not a keyword, just an identifier)
    let result = parse_expr_from_let("this.bar(1)");
    if result.is_err() {
        return Err(format!("Failed to parse method call on this: : {result:?}").into());
    }
    Ok(())
}

#[test]
fn test_invocation_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Simple invocation (should work)
    let result = parse_expr_from_let("foo(1)");
    if result.is_err() {
        return Err(format!("Failed to parse invocation: {result:?}").into());
    }
    Ok(())
}
