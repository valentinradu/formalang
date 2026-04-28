//! Walkers for top-level definitions: traits, structs, enums, impls,
//! modules, functions, and the field/type machinery they share.

use super::{NodeAtPosition, NodeFinder};
use crate::ast::{
    Definition, EnumDef, EnumVariant, FieldDef, FnDef, FunctionDef, ImplDef, ModuleDef, StructDef,
    StructField, TraitDef, Type,
};
use crate::semantic::position::span_contains_offset;

impl<'ast> NodeFinder<'ast> {
    /// Visit a definition
    pub(super) fn visit_definition(&mut self, definition: &'ast Definition) {
        match definition {
            Definition::Trait(trait_def) => {
                if span_contains_offset(&trait_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::TraitDef(trait_def));
                    self.visit_trait_def(trait_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Struct(struct_def) => {
                if span_contains_offset(&struct_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::StructDef(struct_def));
                    self.visit_struct_def(struct_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Enum(enum_def) => {
                if span_contains_offset(&enum_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::EnumDef(enum_def));
                    self.visit_enum_def(enum_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Impl(impl_def) => {
                if span_contains_offset(&impl_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::ImplDef(impl_def));
                    self.visit_impl_def(impl_def);
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Module(module_def) => {
                if span_contains_offset(&module_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::ModuleDef(module_def));
                    self.visit_module_def(module_def);
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Function(func_def) => {
                if span_contains_offset(&func_def.span, self.offset) {
                    self.parents
                        .push(NodeAtPosition::FunctionDef(func_def.as_ref()));
                    self.visit_function_def(func_def.as_ref());
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
        }
    }

    /// Visit a trait definition
    fn visit_trait_def(&mut self, trait_def: &'ast TraitDef) {
        // Check name
        if span_contains_offset(&trait_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&trait_def.name));
            return;
        }

        // Check generic parameters
        for generic in &trait_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check trait composition
        for trait_ref in &trait_def.traits {
            if span_contains_offset(&trait_ref.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_ref));
                return;
            }
        }

        // Check fields
        for field in &trait_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the trait def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::TraitDef(trait_def));
        }
    }

    /// Visit a struct definition (unified model/view)
    fn visit_struct_def(&mut self, struct_def: &'ast StructDef) {
        // Check name
        if span_contains_offset(&struct_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&struct_def.name));
            return;
        }

        // Check generic parameters
        for generic in &struct_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check regular fields
        for field in &struct_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::StructField(field));
                self.visit_struct_field(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the struct def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::StructDef(struct_def));
        }
    }

    /// Visit an enum definition
    fn visit_enum_def(&mut self, enum_def: &'ast EnumDef) {
        // Check name
        if span_contains_offset(&enum_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&enum_def.name));
            return;
        }

        // Check generic parameters
        for generic in &enum_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check variants
        for variant in &enum_def.variants {
            if span_contains_offset(&variant.span, self.offset) {
                self.parents.push(NodeAtPosition::EnumVariant(variant));
                self.visit_enum_variant(variant);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the enum def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumDef(enum_def));
        }
    }

    /// Visit an enum variant
    fn visit_enum_variant(&mut self, variant: &'ast EnumVariant) {
        // Check name
        if span_contains_offset(&variant.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&variant.name));
            return;
        }

        // Check fields
        for field in &variant.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the variant itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumVariant(variant));
        }
    }

    /// Visit an impl block definition
    fn visit_impl_def(&mut self, impl_def: &'ast ImplDef) {
        // Check the struct name
        if span_contains_offset(&impl_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&impl_def.name));
            return;
        }

        // Check trait name if present
        if let Some(trait_name) = &impl_def.trait_name {
            if span_contains_offset(&trait_name.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_name));
                return;
            }
        }

        // Check functions within the impl block
        for func in &impl_def.functions {
            if span_contains_offset(&func.span, self.offset) {
                self.parents.push(NodeAtPosition::FnDef(func));
                self.visit_fn_def(func);
                if self.found_node.is_none() {
                    self.parents.pop();
                }
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the impl def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::ImplDef(impl_def));
        }
    }

    /// Visit a function definition inside an impl block (`FnDef`)
    fn visit_fn_def(&mut self, func_def: &'ast FnDef) {
        // Check function name
        if span_contains_offset(&func_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&func_def.name));
            return;
        }

        // Check parameters
        for param in &func_def.params {
            if span_contains_offset(&param.span, self.offset) {
                // Check parameter name
                if span_contains_offset(&param.name.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(&param.name));
                    return;
                }
                // Check parameter type
                if let Some(ref ty) = param.ty {
                    self.visit_type(ty);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                // Return the parameter itself
                self.found_node = Some(NodeAtPosition::FunctionParam(param));
                return;
            }
        }

        // Check return type
        if let Some(ref ret_ty) = func_def.return_type {
            self.visit_type(ret_ty);
            if self.found_node.is_some() {
                return;
            }
        }

        // Check body expression (only if function has a body)
        if let Some(ref body) = func_def.body {
            self.visit_expr(body);
        }

        // If no specific node found, return the fn def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::FnDef(func_def));
        }
    }

    /// Visit a module definition
    fn visit_module_def(&mut self, module_def: &'ast ModuleDef) {
        // Check the module name
        if span_contains_offset(&module_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&module_def.name));
            return;
        }

        // Check nested definitions
        for def in &module_def.definitions {
            self.visit_definition(def);
            if self.found_node.is_some() {
                return;
            }
        }

        // If no specific node found, return the module def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::ModuleDef(module_def));
        }
    }

    /// Visit a standalone function definition
    fn visit_function_def(&mut self, func_def: &'ast FunctionDef) {
        // Check function name
        if span_contains_offset(&func_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&func_def.name));
            return;
        }

        // Check parameters
        for param in &func_def.params {
            if span_contains_offset(&param.span, self.offset) {
                // Check parameter name
                if span_contains_offset(&param.name.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(&param.name));
                    return;
                }
                // Check parameter type
                if let Some(ref ty) = param.ty {
                    self.visit_type(ty);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                // Return the parameter itself
                self.found_node = Some(NodeAtPosition::FunctionParam(param));
                return;
            }
        }

        // Check return type
        if let Some(ref ret_ty) = func_def.return_type {
            self.visit_type(ret_ty);
            if self.found_node.is_some() {
                return;
            }
        }

        // Check body expression (only if function has a body)
        if let Some(ref body) = func_def.body {
            self.visit_expr(body);
        }

        // If no specific node found, return the function def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::FunctionDef(func_def));
        }
    }

    /// Visit a field definition
    fn visit_field_def(&mut self, field: &'ast FieldDef) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);
    }

    /// Visit a model field
    /// Visit a struct field (unified for regular and mount fields)
    fn visit_struct_field(&mut self, field: &'ast StructField) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);

        // Check default value
        if let Some(default) = &field.default {
            self.visit_expr(default);
        }
    }

    /// Visit a type
    fn visit_type(&mut self, ty: &'ast Type) {
        match ty {
            Type::Primitive(_) => {
                // Primitive types don't have spans to check
            }
            Type::Ident(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            Type::Generic { name, args, span } => {
                if span_contains_offset(span, self.offset) {
                    // Check the generic type name
                    if span_contains_offset(&name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(name));
                        return;
                    }

                    // Check type arguments
                    for arg in args {
                        self.visit_type(arg);
                        if self.found_node.is_some() {
                            return;
                        }
                    }

                    // If no specific part found, return the type itself
                    if self.found_node.is_none() {
                        self.found_node = Some(NodeAtPosition::Type(ty));
                    }
                }
            }
            Type::Array(inner) | Type::Optional(inner) => {
                self.visit_type(inner);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if span_contains_offset(&field.span, self.offset) {
                        if span_contains_offset(&field.name.span, self.offset) {
                            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
                            return;
                        }
                        self.visit_type(&field.ty);
                        if self.found_node.is_some() {
                            return;
                        }
                    }
                }
            }
            Type::Dictionary { key, value } => {
                self.visit_type(key);
                if self.found_node.is_some() {
                    return;
                }
                self.visit_type(value);
            }
            Type::Closure { params, ret } => {
                for (_, param) in params {
                    self.visit_type(param);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                self.visit_type(ret);
            }
        }
    }
}
