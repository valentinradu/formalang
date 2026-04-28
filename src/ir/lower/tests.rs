use super::lower_to_ir;
use crate::ast::File;
use crate::semantic::SymbolTable;

#[test]
fn test_lower_empty_file() -> Result<(), Box<dyn std::error::Error>> {
    let ast = File {
        statements: vec![],
        span: crate::location::Span::default(),
        format_version: 1,
    };
    let symbols = SymbolTable::new();
    let result = lower_to_ir(&ast, &symbols);
    if result.is_err() {
        return Err(format!("Expected ok: {:?}", result.err()).into());
    }
    let module = result.map_err(|e| format!("{e:?}"))?;
    if !module.structs.is_empty() {
        return Err(format!("Expected empty structs, got {}", module.structs.len()).into());
    }
    if !module.traits.is_empty() {
        return Err(format!("Expected empty traits, got {}", module.traits.len()).into());
    }
    if !module.enums.is_empty() {
        return Err(format!("Expected empty enums, got {}", module.enums.len()).into());
    }
    Ok(())
}
