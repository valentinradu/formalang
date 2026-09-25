# Building a Code Generator

A complete TypeScript interface generator demonstrating how to walk
`IrModule` with a [`IrVisitor`](visitor.md), resolve types via
[`ResolvedType`](types.md), and emit target-language source.

The program below compiles and runs against the current crate.

```rust
use formalang::ast::PrimitiveType;
use formalang::compile_to_ir;
use formalang::ir::{
    walk_module, EnumId, GenericBase, IrEnum, IrField, IrModule, IrStruct, IrTrait, IrVisitor,
    ResolvedType, StructId, TraitId,
};

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

    fn struct_name(&self, id: StructId) -> String {
        self.module
            .get_struct(id)
            .map_or_else(|| "unknown".to_string(), |s| s.name.clone())
    }

    fn enum_name(&self, id: EnumId) -> String {
        self.module
            .get_enum(id)
            .map_or_else(|| "unknown".to_string(), |e| e.name.clone())
    }

    fn trait_name(&self, id: TraitId) -> String {
        self.module
            .get_trait(id)
            .map_or_else(|| "unknown".to_string(), |t| t.name.clone())
    }

    fn resolve_type(&self, ty: &ResolvedType) -> String {
        // The built-in carriers first: they are `Generic` over a
        // prelude definition.
        if let Some(inner) = self.module.array_element_ty(ty) {
            return format!("{}[]", self.resolve_type(inner));
        }
        if let Some(inner) = self.module.optional_inner_ty(ty) {
            return format!("{} | null", self.resolve_type(inner));
        }
        if let Some((key, value)) = self.module.dictionary_kv_ty(ty) {
            return format!(
                "Record<{}, {}>",
                self.resolve_type(key),
                self.resolve_type(value)
            );
        }
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::I32
                | PrimitiveType::I64
                | PrimitiveType::F32
                | PrimitiveType::F64 => "number".to_string(),
                PrimitiveType::Boolean => "boolean".to_string(),
                PrimitiveType::Never => "never".to_string(),
                _ => "unknown".to_string(),
            },
            ResolvedType::Struct(id) => self.struct_name(*id),
            ResolvedType::Trait(id) => self.trait_name(*id),
            ResolvedType::Enum(id) => self.enum_name(*id),
            ResolvedType::Tuple(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.resolve_type(ty)))
                    .collect();
                format!("{{ {} }}", fields.join("; "))
            }
            ResolvedType::Generic { base, args } => {
                let base_name = match base {
                    GenericBase::Struct(id) => self.struct_name(*id),
                    GenericBase::Enum(id) => self.enum_name(*id),
                    GenericBase::Trait(id) => self.trait_name(*id),
                };
                let args: Vec<String> = args.iter().map(|a| self.resolve_type(a)).collect();
                format!("{}<{}>", base_name, args.join(", "))
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                let params: Vec<String> = param_tys
                    .iter()
                    .enumerate()
                    .map(|(i, (_, t))| format!("a{}: {}", i, self.resolve_type(t)))
                    .collect();
                format!("({}) => {}", params.join(", "), self.resolve_type(return_ty))
            }
            ResolvedType::External {
                name, type_args, ..
            } => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    let args: Vec<String> =
                        type_args.iter().map(|a| self.resolve_type(a)).collect();
                    format!("{}<{}>", name, args.join(", "))
                }
            }
            ResolvedType::TypeParam(name) => name.clone(),
            ResolvedType::Error => "never".to_string(),
        }
    }

    fn emit_field(&mut self, field: &IrField) {
        let ts_type = self.resolve_type(&field.ty);
        let optional = if field.optional { "?" } else { "" };
        self.output
            .push_str(&format!("  {}{}: {};\n", field.name, optional, ts_type));
    }
}

impl IrVisitor for TypeScriptGenerator<'_> {
    fn visit_trait(&mut self, _id: TraitId, t: &IrTrait) {
        if !t.visibility.is_public() {
            return;
        }
        self.output
            .push_str(&format!("export interface {} {{\n", t.name));
        for field in &t.fields {
            self.emit_field(field);
        }
        self.output.push_str("}\n\n");
    }

    fn visit_struct(&mut self, id: StructId, s: &IrStruct) {
        // Skip the prelude carriers and private structs
        if self.module.is_prelude_struct(id) || !s.visibility.is_public() {
            return;
        }

        let generics = if s.generic_params.is_empty() {
            String::new()
        } else {
            let params: Vec<String> = s.generic_params.iter().map(|p| p.name.clone()).collect();
            format!("<{}>", params.join(", "))
        };

        // `impl Named for User` puts `Named` in `traits`
        let extends = if s.traits.is_empty() {
            String::new()
        } else {
            let traits: Vec<String> = s
                .traits
                .iter()
                .map(|t| self.trait_name(t.trait_id))
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

    fn visit_enum(&mut self, id: EnumId, e: &IrEnum) {
        if self.module.is_prelude_enum(id) || !e.visibility.is_public() {
            return;
        }
        self.output.push_str(&format!("export type {} =\n", e.name));
        for (i, variant) in e.variants.iter().enumerate() {
            let end = if i + 1 == e.variants.len() { ";" } else { "" };
            if variant.fields.is_empty() {
                self.output
                    .push_str(&format!("  | {{ type: \"{}\" }}{}\n", variant.name, end));
            } else {
                let fields: Vec<String> = variant
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.resolve_type(&f.ty)))
                    .collect();
                self.output.push_str(&format!(
                    "  | {{ type: \"{}\"; {} }}{}\n",
                    variant.name,
                    fields.join("; "),
                    end
                ));
            }
        }
        self.output.push('\n');
    }
}

fn generate_typescript(source: &str) -> Result<String, Vec<formalang::CompilerError>> {
    let module = compile_to_ir(source)?;
    let mut generator = TypeScriptGenerator::new(&module);
    walk_module(&mut generator, &module);
    Ok(generator.output)
}

fn main() {
    // Usage
    let source = r#"
pub trait Named {
    name: String
}

pub struct User {
    name: String,
    age: I32,
    email: String?
}

impl Named for User {}

pub enum Status {
    active,
    pending(reason: String),
    inactive
}
"#;

    let typescript = generate_typescript(source).unwrap();
    print!("{}", typescript);
}
```

Output. The visitor visits the structs before the traits:

```text
export interface User extends Named {
  name: string;
  age: number;
  email?: string | null;
}

export interface Named {
  name: string;
}

export type Status =
  | { type: "active" }
  | { type: "pending"; reason: string }
  | { type: "inactive" };
```
