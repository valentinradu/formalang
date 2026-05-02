# Worked Examples

These examples show how source code maps to the IR. Typed-id values
like `BindingId`, `VariantIdx`, `FieldIdx`, `MethodIdx`, and the
`target` field on `Reference` are populated by `ResolveReferencesPass`.
Pre-pass (raw lowering output), they carry `0` / `Unresolved` placeholders.

## Simple Struct

**FormaLang source:**

```formalang
pub struct User {
    name: String,
    age: I32
}
```

**IR structure:**

```text
IrModule
+-- structs[0]: IrStruct
    +-- name: "User"
    +-- visibility: Public
    +-- traits: []
    +-- fields:
    |   +-- [0] IrField
    |   |   +-- name: "name"
    |   |   +-- ty: Primitive(String)
    |   |   +-- mutable: false
    |   |   +-- optional: false
    |   |   +-- default: None
    |   +-- [1] IrField
    |       +-- name: "age"
    |       +-- ty: Primitive(I32)
    |       +-- mutable: false
    |       +-- optional: false
    |       +-- default: None
    +-- generic_params: []
```

## Enum with Variants

**FormaLang source:**

```formalang
pub enum Status {
    active,
    inactive,
    pending(reason: String)
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum
    +-- name: "Status"
    +-- visibility: Public
    +-- variants:
    |   +-- [0] IrEnumVariant
    |   |   +-- name: "active"
    |   |   +-- fields: []
    |   +-- [1] IrEnumVariant
    |   |   +-- name: "inactive"
    |   |   +-- fields: []
    |   +-- [2] IrEnumVariant
    |       +-- name: "pending"
    |       +-- fields:
    |           +-- [0] IrField
    |               +-- name: "reason"
    |               +-- ty: Primitive(String)
    +-- generic_params: []
```

## Struct Implementing Trait

**FormaLang source:**

```formalang
pub trait Named {
    name: String
}

pub struct User: Named {
    name: String,
    age: I32
}
```

**IR structure:**

```text
IrModule
+-- traits[0]: IrTrait              // TraitId(0)
|   +-- name: "Named"
|   +-- visibility: Public
|   +-- composed_traits: []
|   +-- fields:
|   |   +-- [0] IrField
|   |       +-- name: "name"
|   |       +-- ty: Primitive(String)
|   +-- methods: []
|   +-- generic_params: []
|
+-- structs[0]: IrStruct            // StructId(0)
    +-- name: "User"
    +-- visibility: Public
    +-- traits: [TraitId(0)]        // <-- linked to Named trait
    +-- fields:
    |   +-- [0] IrField { name: "name", ty: Primitive(String), ... }
    |   +-- [1] IrField { name: "age", ty: Primitive(I32), ... }
    +-- generic_params: []
```

## Generic Struct with Constraint

**FormaLang source:**

```formalang
pub trait Container {
    items: [String]
}

pub struct Box<T: Container> {
    content: T,
    label: String?
}
```

**IR structure:**

```text
IrModule
+-- traits[0]: IrTrait              // TraitId(0)
|   +-- name: "Container"
|   +-- fields:
|       +-- [0] IrField
|           +-- name: "items"
|           +-- ty: Array(Box::new(Primitive(String)))
|
+-- structs[0]: IrStruct            // StructId(0)
    +-- name: "Box"
    +-- visibility: Public
    +-- traits: []
    +-- fields:
    |   +-- [0] IrField
    |   |   +-- name: "content"
    |   |   +-- ty: TypeParam("T")  // Unresolved in definition
    |   |   +-- optional: false
    |   +-- [1] IrField
    |       +-- name: "label"
    |       +-- ty: Optional(Box::new(Primitive(String)))
    |       +-- optional: true
    +-- generic_params:
        +-- [0] IrGenericParam
            +-- name: "T"
            +-- constraints: [TraitId(0)]  // <-- must implement Container
```

## Struct with Cross-References

**FormaLang source:**

```formalang
enum Status { active, inactive }

struct Author {
    name: String
}

struct Book {
    title: String,
    author: Author,
    status: Status
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum                // EnumId(0)
|   +-- name: "Status"
|   +-- variants: [active, inactive]
|
+-- structs[0]: IrStruct            // StructId(0)
|   +-- name: "Author"
|   +-- fields:
|       +-- [0] IrField { name: "name", ty: Primitive(String) }
|
+-- structs[1]: IrStruct            // StructId(1)
    +-- name: "Book"
    +-- fields:
        +-- [0] IrField { name: "title", ty: Primitive(String) }
        +-- [1] IrField { name: "author", ty: Struct(StructId(0)) }  // linked!
        +-- [2] IrField { name: "status", ty: Enum(EnumId(0)) }      // linked!
```

## Impl Block with Methods

**FormaLang source:**

```formalang
pub struct Counter {
    count: I32
}

impl Counter {
    fn increment(self) -> I32 {
        self.count + 1
    }

    fn reset(mut self) -> I32 {
        0
    }
}
```

**IR structure:**

```text
IrModule
+-- structs[0]: IrStruct            // StructId(0)
|   +-- name: "Counter"
|   +-- fields:
|       +-- [0] IrField { name: "count", ty: Primitive(I32) }
|
+-- impls[0]: IrImpl
    +-- target: ImplTarget::Struct(StructId(0))
    +-- functions:
        +-- [0] IrFunction
        |   +-- name: "increment"
        |   +-- params: [IrFunctionParam { name: "self", ty: None, convention: Let }]
        |   +-- return_type: Some(Primitive(I32))
        |   +-- body: Some(IrExpr::BinaryOp {
        |           left: IrExpr::Reference { path: ["self", "count"], ty: Primitive(I32) },
        |           op: Add,
        |           right: IrExpr::Literal { value: Number(NumberLiteral { value: NumberValue::Integer(1), .. }), ty: Primitive(I32) },
        |           ty: Primitive(I32)
        |       })
        +-- [1] IrFunction
            +-- name: "reset"
            +-- params: [IrFunctionParam { name: "self", ty: None, convention: Mut }]
            +-- return_type: Some(Primitive(I32))
            +-- body: Some(IrExpr::Literal { value: Number(NumberLiteral { value: NumberValue::Integer(0), .. }), ty: Primitive(I32) })
```

## Match Expression

**FormaLang source:**

```formalang
pub enum Option {
    none,
    some(value: I32)
}

pub fn describe(opt: Option) -> String {
    match opt {
        .none: "Nothing",
        .some(value): "Got value"
    }
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum                // EnumId(0)
|   +-- name: "Option"
|   +-- variants:
|       +-- [0] IrEnumVariant { name: "none", fields: [] }
|       +-- [1] IrEnumVariant { name: "some", fields: [IrField { name: "value", ... }] }
|
+-- functions[0]: IrFunction
    +-- name: "describe"
    +-- params:
    |   +-- [0] IrFunctionParam { name: "opt", ty: Some(Enum(EnumId(0))), convention: Let }
    +-- return_type: Some(Primitive(String))
    +-- body: Some(IrExpr::Match {
            scrutinee: IrExpr::Reference {
                path: ["opt"],
                target: ReferenceTarget::Param(BindingId(0)),
                ty: Enum(EnumId(0))
            },
            arms: [
                IrMatchArm {
                    variant: "none",
                    variant_idx: VariantIdx(0),
                    is_wildcard: false,
                    bindings: [],
                    body: IrExpr::Literal { value: String("Nothing"), ty: Primitive(String) }
                },
                IrMatchArm {
                    variant: "some",
                    variant_idx: VariantIdx(1),
                    is_wildcard: false,
                    bindings: [("value", BindingId(1), Primitive(I32))],
                    body: IrExpr::Literal { value: String("Got value"), ty: Primitive(String) }
                }
            ],
            ty: Primitive(String)
        })
```

## For Expression

**FormaLang source:**

```formalang
pub fn tag_labels(tags: [String]) -> [String] {
    for tag in tags { tag }
}
```

**IR structure:**

```text
IrModule
+-- functions[0]: IrFunction
    +-- name: "tag_labels"
    +-- params:
    |   +-- [0] IrFunctionParam { name: "tags", ty: Some(Array(Primitive(String))), convention: Let }
    +-- return_type: Some(Array(Primitive(String)))
    +-- body: Some(IrExpr::For {
            var: "tag",
            var_ty: Primitive(String),
            var_binding_id: BindingId(1),
            collection: IrExpr::Reference {
                path: ["tags"],
                target: ReferenceTarget::Param(BindingId(0)),
                ty: Array(Primitive(String))
            },
            body: IrExpr::LetRef {
                name: "tag",
                binding_id: BindingId(1),
                ty: Primitive(String)
            },
            ty: Array(Primitive(String))
        })
```
