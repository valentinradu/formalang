# Building a Code Generator

A complete TypeScript interface generator demonstrating how to walk
`IrModule` with a [`IrVisitor`](visitor.md), resolve types via
[`ResolvedType`](types.md), and emit target-language source.

```rust
use formalang::compile_to_ir;
use formalang::ir::{
    IrModule, IrStruct, IrEnum, IrEnumVariant, IrField, IrVisitor,
    StructId, EnumId, ResolvedType, walk_module
};
use formalang::ast::PrimitiveType;

struct TypeScriptGenerator<'a> {
    module: &'a IrModule,
    output: String,
}

impl<'a> TypeScriptGenerator<'a> {
    fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            output: String::new(),
        }
    }

    fn resolve_type(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::I32 | PrimitiveType::I64 |
                PrimitiveType::F32 | PrimitiveType::F64 => "number".to_string(),
                PrimitiveType::Boolean => "boolean".to_string(),
                PrimitiveType::Path => "string".to_string(),
                PrimitiveType::Regex => "RegExp".to_string(),
                PrimitiveType::Never => "never".to_string(),
            },
            ResolvedType::Struct(id) => {
                self.module.get_struct(*id).unwrap().name.clone()
            }
            ResolvedType::Trait(id) => {
                self.module.get_trait(*id).unwrap().name.clone()
            }
            ResolvedType::Enum(id) => {
                self.module.get_enum(*id).unwrap().name.clone()
            }
            ResolvedType::Array(inner) => {
                format!("{}[]", self.resolve_type(inner))
            }
            ResolvedType::Optional(inner) => {
                format!("{} | null", self.resolve_type(inner))
            }
            ResolvedType::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.resolve_type(ty)))
                    .collect();
                format!("{{ {} }}", fields_str.join("; "))
            }
            ResolvedType::Generic { base, args } => {
                let base_name = match base {
                    GenericBase::Struct(id) => self.module.get_struct(*id).unwrap().name.clone(),
                    GenericBase::Enum(id) => self.module.get_enum(*id).unwrap().name.clone(),
                    GenericBase::Trait(id) => self.module.get_trait(*id).unwrap().name.clone(),
                };
                let args_str: Vec<_> = args.iter().map(|a| self.resolve_type(a)).collect();
                format!("{}<{}>", base_name, args_str.join(", "))
            }
            ResolvedType::Dictionary { key_ty, value_ty } => {
                format!(
                    "Record<{}, {}>",
                    self.resolve_type(key_ty),
                    self.resolve_type(value_ty)
                )
            }
            ResolvedType::Closure { param_tys, return_ty } => {
                let params: Vec<_> = param_tys
                    .iter()
                    .enumerate()
                    .map(|(i, (_, t))| format!("a{}: {}", i, self.resolve_type(t)))
                    .collect();
                format!("({}) => {}", params.join(", "), self.resolve_type(return_ty))
            }
            ResolvedType::External { name, type_args, .. } => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    let args: Vec<_> = type_args.iter().map(|a| self.resolve_type(a)).collect();
                    format!("{}<{}>", name, args.join(", "))
                }
            }
            ResolvedType::TypeParam(name) => name.clone(),
        }
    }

    fn emit_field(&mut self, field: &IrField) {
        let ts_type = self.resolve_type(&field.ty);
        let optional = if field.optional { "?" } else { "" };
        self.output.push_str(&format!(
            "  {}{}: {};\n",
            field.name, optional, ts_type
        ));
    }
}

impl<'a> IrVisitor for TypeScriptGenerator<'a> {
    fn visit_struct(&mut self, _id: StructId, s: &IrStruct) {
        // Skip private structs
        if !s.visibility.is_public() {
            return;
        }

        // Generic parameters
        let generics = if s.generic_params.is_empty() {
            String::new()
        } else {
            let params: Vec<_> = s.generic_params
                .iter()
                .map(|p| p.name.clone())
                .collect();
            format!("<{}>", params.join(", "))
        };

        // Extends clause for traits
        let extends = if s.traits.is_empty() {
            String::new()
        } else {
            let traits: Vec<_> = s.traits
                .iter()
                .map(|id| self.module.get_trait(*id).name.clone())
                .collect();
            format!(" extends {}", traits.join(", "))
        };

        self.output.push_str(&format!(
            "export interface {}{}{} {{\n",
            s.name, generics, extends
        ));

        for field in &s.fields {
            self.emit_field(field);
        }

        self.output.push_str("}\n\n");
    }

    fn visit_enum(&mut self, _id: EnumId, e: &IrEnum) {
        if !e.visibility.is_public() {
            return;
        }

        // Generate discriminated union
        self.output.push_str(&format!(
            "export type {} =\n",
            e.name
        ));

        for (i, variant) in e.variants.iter().enumerate() {
            let sep = if i == e.variants.len() - 1 { ";" } else { " |" };

            if variant.fields.is_empty() {
                self.output.push_str(&format!(
                    "  | {{ type: \"{}\" }}{}\n",
                    variant.name, sep
                ));
            } else {
                let fields: Vec<_> = variant.fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.resolve_type(&f.ty)))
                    .collect();
                self.output.push_str(&format!(
                    "  | {{ type: \"{}\"; {} }}{}\n",
                    variant.name, fields.join("; "), sep
                ));
            }
        }

        self.output.push('\n');
    }
}

fn generate_typescript(source: &str) -> Result<String, Vec<formalang::CompilerError>> {
    let module = compile_to_ir(source)?;
    let mut gen = TypeScriptGenerator::new(&module);
    walk_module(&mut gen, &module);
    Ok(gen.output)
}

// Usage
let source = r#"
pub trait Named {
    name: String
}

pub struct User: Named {
    name: String,
    age: I32,
    email: String?
}

pub enum Status {
    active,
    pending(reason: String),
    inactive
}
"#;

let typescript = generate_typescript(source).unwrap();
println!("{}", typescript);

// Output:
// export interface Named {
//   name: string;
// }
//
// export interface User extends Named {
//   name: string;
//   age: number;
//   email?: string | null;
// }
//
// export type Status =
//   | { type: "active" } |
//   | { type: "pending"; reason: string } |
//   | { type: "inactive" };
```
