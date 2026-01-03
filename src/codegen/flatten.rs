//! Tree flattening for GPU rendering.
//!
//! UI elements form a tree structure (parents containing children).
//! For GPU rendering, this tree must be flattened into a linear buffer
//! where each element knows its depth and parent for coordinate transforms.
//!
//! # Structure
//!
//! Each flattened element has:
//! - `type_tag`: Identifies the concrete type (View, Label, Button, etc.)
//! - `depth_index`: How deep in the tree (0 = root)
//! - `parent_index`: Index of parent element (-1 for root)
//! - `child_count`: Number of direct children
//! - `child_start`: Index of first child in the flattened array
//!
//! # Example
//!
//! ```text
//! VStack {
//!   Label("Hello")
//!   HStack {
//!     Button("OK")
//!     Button("Cancel")
//!   }
//! }
//! ```
//!
//! Flattens to:
//! ```text
//! [0] VStack    depth=0, parent=-1, children=2, child_start=1
//! [1] Label     depth=1, parent=0,  children=0, child_start=-1
//! [2] HStack    depth=1, parent=0,  children=2, child_start=3
//! [3] Button    depth=2, parent=2,  children=0, child_start=-1
//! [4] Button    depth=2, parent=2,  children=0, child_start=-1
//! ```

use crate::ir::{IrExpr, IrModule, StructId};
use std::collections::HashMap;

/// A flattened element in the render tree.
#[derive(Debug, Clone)]
pub struct FlatElement {
    /// Unique index in the flattened array
    pub index: u32,

    /// Type tag for dispatch (see DispatchGenerator)
    pub type_tag: u32,

    /// Struct ID of this element
    pub struct_id: Option<StructId>,

    /// Name of the struct type
    pub type_name: String,

    /// Depth in the tree (0 = root)
    pub depth: u32,

    /// Index of parent element (u32::MAX for root)
    pub parent_index: u32,

    /// Number of direct children
    pub child_count: u32,

    /// Index of first child (u32::MAX if no children)
    pub child_start: u32,

    /// The original expression (for property access)
    pub expr: IrExpr,
}

/// Result of tree flattening.
#[derive(Debug)]
pub struct FlattenedTree {
    /// Elements in depth-first order
    pub elements: Vec<FlatElement>,

    /// Maximum depth encountered
    pub max_depth: u32,

    /// Type tag assignments
    pub type_tags: HashMap<String, u32>,
}

impl FlattenedTree {
    /// Get an element by index.
    pub fn get(&self, index: u32) -> Option<&FlatElement> {
        self.elements.get(index as usize)
    }

    /// Iterate elements at a specific depth.
    pub fn elements_at_depth(&self, depth: u32) -> impl Iterator<Item = &FlatElement> {
        self.elements.iter().filter(move |e| e.depth == depth)
    }

    /// Iterate root elements (depth 0).
    pub fn roots(&self) -> impl Iterator<Item = &FlatElement> {
        self.elements_at_depth(0)
    }

    /// Get children of an element.
    pub fn children(&self, index: u32) -> &[FlatElement] {
        if let Some(elem) = self.get(index) {
            if elem.child_count > 0 && elem.child_start != u32::MAX {
                let start = elem.child_start as usize;
                let end = start + elem.child_count as usize;
                if end <= self.elements.len() {
                    return &self.elements[start..end];
                }
            }
        }
        &[]
    }
}

/// Tree flattener for IR expressions.
pub struct TreeFlattener<'a> {
    module: &'a IrModule,
    elements: Vec<FlatElement>,
    type_tags: HashMap<String, u32>,
    next_type_tag: u32,
}

impl<'a> TreeFlattener<'a> {
    /// Create a new tree flattener.
    pub fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            elements: Vec::new(),
            type_tags: HashMap::new(),
            next_type_tag: 0,
        }
    }

    /// Flatten an expression tree into a linear array.
    pub fn flatten(mut self, expr: &IrExpr) -> FlattenedTree {
        self.flatten_expr(expr, 0, u32::MAX);
        self.fixup_child_indices();

        let max_depth = self.elements.iter().map(|e| e.depth).max().unwrap_or(0);

        FlattenedTree {
            elements: self.elements,
            max_depth,
            type_tags: self.type_tags,
        }
    }

    fn flatten_expr(&mut self, expr: &IrExpr, depth: u32, parent_index: u32) -> u32 {
        let index = self.elements.len() as u32;

        match expr {
            IrExpr::StructInst {
                struct_id, fields, ..
            } => {
                // Get struct info if available
                let (type_name, mount_fields) = if let Some(sid) = struct_id {
                    let info = self.module.get_struct(*sid);
                    (
                        info.name.clone(),
                        info.mount_fields
                            .iter()
                            .map(|f| f.name.clone())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    ("Unknown".to_string(), Vec::new())
                };
                let type_tag = self.get_or_assign_type_tag(&type_name);

                // Add this element
                self.elements.push(FlatElement {
                    index,
                    type_tag,
                    struct_id: *struct_id,
                    type_name,
                    depth,
                    parent_index,
                    child_count: 0, // Will be updated later
                    child_start: u32::MAX,
                    expr: expr.clone(),
                });

                // Find children in mount fields
                let child_start = self.elements.len() as u32;
                let mut child_count = 0u32;

                for (field_name, field_expr) in fields {
                    // Check if this is a mount field
                    let is_mount = mount_fields.iter().any(|f| f == field_name);

                    if is_mount {
                        // Flatten children
                        match field_expr {
                            IrExpr::Array { elements, .. } => {
                                for child_expr in elements {
                                    self.flatten_expr(child_expr, depth + 1, index);
                                    child_count += 1;
                                }
                            }
                            _ => {
                                // Single child
                                self.flatten_expr(field_expr, depth + 1, index);
                                child_count += 1;
                            }
                        }
                    }
                }

                // Update child info
                if child_count > 0 {
                    self.elements[index as usize].child_count = child_count;
                    self.elements[index as usize].child_start = child_start;
                }

                index
            }

            IrExpr::Array { elements, .. } => {
                // Arrays at the top level - flatten each element as a root
                for elem in elements {
                    self.flatten_expr(elem, depth, parent_index);
                }
                index
            }

            _ => {
                // Non-struct expressions - wrap in a generic element
                let type_name = format!("Expr_{}", self.type_name_for_expr(expr));
                let type_tag = self.get_or_assign_type_tag(&type_name);

                self.elements.push(FlatElement {
                    index,
                    type_tag,
                    struct_id: None,
                    type_name,
                    depth,
                    parent_index,
                    child_count: 0,
                    child_start: u32::MAX,
                    expr: expr.clone(),
                });

                index
            }
        }
    }

    fn type_name_for_expr(&self, expr: &IrExpr) -> &'static str {
        match expr {
            IrExpr::Literal { .. } => "Literal",
            IrExpr::BinaryOp { .. } => "BinaryOp",
            IrExpr::If { .. } => "If",
            IrExpr::Match { .. } => "Match",
            IrExpr::Reference { .. } => "Reference",
            IrExpr::StructInst { .. } => "StructInst",
            IrExpr::EnumInst { .. } => "EnumInst",
            IrExpr::Array { .. } => "Array",
            IrExpr::Tuple { .. } => "Tuple",
            IrExpr::For { .. } => "For",
            IrExpr::SelfFieldRef { .. } => "SelfFieldRef",
            IrExpr::LetRef { .. } => "LetRef",
            IrExpr::FunctionCall { .. } => "FunctionCall",
            IrExpr::MethodCall { .. } => "MethodCall",
            IrExpr::EventMapping { .. } => "EventMapping",
            IrExpr::DictLiteral { .. } => "DictLiteral",
            IrExpr::DictAccess { .. } => "DictAccess",
        }
    }

    fn get_or_assign_type_tag(&mut self, type_name: &str) -> u32 {
        if let Some(&tag) = self.type_tags.get(type_name) {
            tag
        } else {
            let tag = self.next_type_tag;
            self.type_tags.insert(type_name.to_string(), tag);
            self.next_type_tag += 1;
            tag
        }
    }

    fn fixup_child_indices(&mut self) {
        // Child indices are already correct from the flatten pass
        // This method is a hook for any post-processing needed
    }
}

/// Flatten an expression tree from an IR module.
pub fn flatten_tree(module: &IrModule, expr: &IrExpr) -> FlattenedTree {
    TreeFlattener::new(module).flatten(expr)
}

/// Generate WGSL struct for flattened elements.
pub fn gen_flat_element_struct() -> String {
    r#"struct FlatElement {
    type_tag: u32,
    depth: u32,
    parent_index: u32,
    child_count: u32,
    child_start: u32,
    // Element-specific data follows...
}
"#
    .to_string()
}

/// Generate WGSL array type for flat element buffer.
pub fn gen_flat_buffer_type(max_elements: usize) -> String {
    format!(
        "@group(0) @binding(0) var<storage, read> elements: array<FlatElement, {}>;\n",
        max_elements
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_flatten_simple_struct() {
        let source = r#"
            struct Box { width: f32 }
            impl Box { width: 100.0 }
            let root: Box = Box()
        "#;

        let module = compile_to_ir(source).unwrap();
        let root_let = module.get_let("root").unwrap();
        let tree = flatten_tree(&module, &root_let.value);

        assert_eq!(tree.elements.len(), 1);
        assert_eq!(tree.elements[0].type_name, "Box");
        assert_eq!(tree.elements[0].depth, 0);
        assert_eq!(tree.elements[0].parent_index, u32::MAX);
    }

    #[test]
    fn test_flatten_array_of_structs() {
        let source = r#"
            struct Item { value: f32 }
            impl Item { value: 10.0 }
            let items: [Item] = [Item(), Item(), Item()]
        "#;

        let module = compile_to_ir(source).unwrap();
        let items_let = module.get_let("items").unwrap();
        let tree = flatten_tree(&module, &items_let.value);

        // Should have 3 items at depth 0
        assert_eq!(tree.elements.len(), 3);
        for elem in &tree.elements {
            assert_eq!(elem.type_name, "Item");
            assert_eq!(elem.depth, 0);
        }
    }

    #[test]
    fn test_flatten_struct_with_nested_field() {
        // Test nested structs through regular fields (not mount)
        let source = r#"
            struct Inner { value: f32 }
            struct Outer { inner: Inner }
            impl Inner { value: 1.0 }
            impl Outer { inner: Inner() }
            let root: Outer = Outer()
        "#;

        let module = compile_to_ir(source).unwrap();
        let root_let = module.get_let("root").unwrap();
        let tree = flatten_tree(&module, &root_let.value);

        // Single struct instantiation (nested structs not children in tree sense)
        assert_eq!(tree.elements.len(), 1);
        assert_eq!(tree.elements[0].type_name, "Outer");
        assert_eq!(tree.elements[0].depth, 0);
    }

    #[test]
    fn test_type_tags_assigned() {
        let source = r#"
            struct TypeA { x: f32 }
            struct TypeB { y: f32 }
            impl TypeA { x: 1.0 }
            impl TypeB { y: 2.0 }
            let a: TypeA = TypeA()
        "#;

        let module = compile_to_ir(source).unwrap();
        let root_let = module.get_let("a").unwrap();
        let tree = flatten_tree(&module, &root_let.value);

        assert!(tree.type_tags.contains_key("TypeA"));
        // TypeB not in tree, so not assigned
    }

    #[test]
    fn test_elements_at_depth() {
        let source = r#"
            struct Item { value: f32 }
            impl Item { value: 10.0 }
            let items: [Item] = [Item(), Item(), Item()]
        "#;

        let module = compile_to_ir(source).unwrap();
        let items_let = module.get_let("items").unwrap();
        let tree = flatten_tree(&module, &items_let.value);

        let depth_0: Vec<_> = tree.elements_at_depth(0).collect();
        let depth_1: Vec<_> = tree.elements_at_depth(1).collect();

        // All items at depth 0
        assert_eq!(depth_0.len(), 3);
        assert_eq!(depth_1.len(), 0);
    }

    #[test]
    fn test_wgsl_generation() {
        let struct_code = gen_flat_element_struct();
        assert!(struct_code.contains("struct FlatElement"));
        assert!(struct_code.contains("type_tag: u32"));
        assert!(struct_code.contains("depth: u32"));
        assert!(struct_code.contains("parent_index: u32"));

        let buffer_code = gen_flat_buffer_type(1024);
        assert!(buffer_code.contains("array<FlatElement, 1024>"));
    }
}
