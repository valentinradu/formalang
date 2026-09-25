# Worked Examples

These examples show how source code maps to the IR. Each tree below
comes from `compile_to_ir` on the source above it. The trees leave out
`doc`, `span`, `convention` and the fields that hold their default
value.

The prelude comes first in every module. So the first user struct is
`StructId(4)` (after `Array`, `Seq`, `Dictionary` and `Range`), the
first user enum is `EnumId(1)` (after `Optional`), and the first user
function is `FunctionId(1)` (after `assert`). The prelude has no
traits, so the first user trait is `TraitId(0)`.

Typed-id values like `BindingId`, `VariantIdx`, `FieldIdx`, `MethodIdx`,
and the `target` field on `Reference` are populated by
`ResolveReferencesPass`. Pre-pass (raw lowering output), they carry
`0` / `Unresolved` placeholders. The trees that show a function body
show the module after that pass.

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
+-- structs[4]: IrStruct            // StructId(4)
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

**JSON** (`serde_json::to_string_pretty(&module.structs[4])`, with
the `serde` feature):

```json
{
  "name": "User",
  "visibility": "Public",
  "traits": [],
  "fields": [
    {
      "name": "name",
      "ty": {
        "Primitive": "String"
      },
      "mutable": false,
      "optional": false,
      "default": null,
      "doc": null,
      "convention": "Let"
    },
    {
      "name": "age",
      "ty": {
        "Primitive": "I32"
      },
      "mutable": false,
      "optional": false,
      "default": null,
      "doc": null,
      "convention": "Let"
    }
  ],
  "generic_params": []
}
```

Serde leaves out a `span` that is the default span, and a `doc` of
`None` on a struct. An `IrField` always writes its `doc`.

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
+-- enums[1]: IrEnum                // EnumId(1)
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

pub struct User {
    name: String,
    age: I32
}

impl Named for User {}
```

The `impl` block declares the conformance. The lowering puts the trait
in the `traits` list of the struct, and keeps the empty impl block.

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
+-- structs[4]: IrStruct            // StructId(4)
|   +-- name: "User"
|   +-- visibility: Public
|   +-- traits: [IrTraitRef { trait_id: TraitId(0), args: [] }]
|   +-- fields:
|   |   +-- [0] IrField { name: "name", ty: Primitive(String), ... }
|   |   +-- [1] IrField { name: "age", ty: Primitive(I32), ... }
|   +-- generic_params: []
|
+-- impls[6]: IrImpl                // after the six prelude impls
    +-- target: Struct(StructId(4))
    +-- trait_ref: Some(IrTraitRef { trait_id: TraitId(0), args: [] })
    +-- is_extern: false
    +-- functions: []
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
|           +-- ty: Generic { base: Struct(StructId(0)), args: [Primitive(String)] }
|                                   // StructId(0) is the prelude `Array`
|
+-- structs[4]: IrStruct            // StructId(4)
    +-- name: "Box"
    +-- visibility: Public
    +-- traits: []
    +-- fields:
    |   +-- [0] IrField
    |   |   +-- name: "content"
    |   |   +-- ty: TypeParam("T")  // a parameter of the definition
    |   |   +-- optional: false
    |   +-- [1] IrField
    |       +-- name: "label"
    |       +-- ty: Generic { base: Enum(EnumId(0)), args: [Primitive(String)] }
    |       |                       // EnumId(0) is the prelude `Optional`
    |       +-- optional: true
    +-- generic_params:
        +-- [0] IrGenericParam
            +-- name: "T"
            +-- constraints: [IrTraitRef { trait_id: TraitId(0), args: [] }]
```

`MonomorphisePass` removes `Box` and adds one copy for each type that
the program gives `T`. This program uses no `Box`, so no copy is made.

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
+-- enums[1]: IrEnum                // EnumId(1)
|   +-- name: "Status"
|   +-- visibility: Private
|   +-- variants: [active, inactive]
|
+-- structs[4]: IrStruct            // StructId(4)
|   +-- name: "Author"
|   +-- visibility: Private
|   +-- fields:
|       +-- [0] IrField { name: "name", ty: Primitive(String) }
|
+-- structs[5]: IrStruct            // StructId(5)
    +-- name: "Book"
    +-- visibility: Private
    +-- fields:
        +-- [0] IrField { name: "title", ty: Primitive(String) }
        +-- [1] IrField { name: "author", ty: Struct(StructId(4)) }  // linked
        +-- [2] IrField { name: "status", ty: Enum(EnumId(1)) }      // linked
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

**IR structure** (after `ResolveReferencesPass`):

```text
IrModule
+-- structs[4]: IrStruct            // StructId(4)
|   +-- name: "Counter"
|   +-- fields:
|       +-- [0] IrField { name: "count", ty: Primitive(I32) }
|
+-- impls[6]: IrImpl
    +-- target: Struct(StructId(4))
    +-- trait_ref: None
    +-- is_extern: false
    +-- functions:
        +-- [0] IrFunction
        |   +-- name: "increment"
        |   +-- visibility: Private
        |   +-- params: [IrFunctionParam { binding_id: BindingId(0), name: "self",
        |   |                              ty: None, convention: Let }]
        |   +-- return_type: Some(Primitive(I32))
        |   +-- body: Some(IrExpr::BinaryOp {
        |           left: IrExpr::SelfFieldRef {
        |               field: "count",
        |               field_idx: FieldIdx(0),
        |               ty: Primitive(I32)
        |           },
        |           op: Add,
        |           right: IrExpr::Literal {
        |               value: Number(NumberLiteral { value: Integer(1), suffix: None, kind: Integer }),
        |               ty: Primitive(I32)
        |           },
        |           ty: Primitive(I32)
        |       })
        +-- [1] IrFunction
            +-- name: "reset"
            +-- visibility: Private
            +-- params: [IrFunctionParam { binding_id: BindingId(0), name: "self",
            |                              ty: None, convention: Mut }]
            +-- return_type: Some(Primitive(I32))
            +-- body: Some(IrExpr::Literal {
                    value: Number(NumberLiteral { value: Integer(0), suffix: None, kind: Integer }),
                    ty: Primitive(I32)
                })
```

A read of `self.count` in a method is a `SelfFieldRef`, not a
`Reference`.

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

**IR structure** (after `ResolveReferencesPass`):

```text
IrModule
+-- enums[1]: IrEnum                // EnumId(1)
|   +-- name: "Option"
|   +-- variants:
|       +-- [0] IrEnumVariant { name: "none", fields: [] }
|       +-- [1] IrEnumVariant { name: "some", fields: [IrField { name: "value", ... }] }
|
+-- functions[1]: IrFunction        // FunctionId(1)
    +-- name: "describe"
    +-- visibility: Public
    +-- params:
    |   +-- [0] IrFunctionParam { binding_id: BindingId(0), name: "opt",
    |                             ty: Some(Enum(EnumId(1))), convention: Let }
    +-- return_type: Some(Primitive(String))
    +-- body: Some(IrExpr::Match {
            scrutinee: IrExpr::Reference {
                path: ["opt"],
                target: ReferenceTarget::Param(BindingId(0)),
                ty: Enum(EnumId(1))
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
    for tag in tags { tag }.collect()
}
```

A `for` gives a `Seq`, not an array, and a function cannot return a
`Seq`. The call `collect()` makes the array. See
[Large Data](../../user/large-data.md).

**IR structure** (after `ResolveReferencesPass`):

```text
IrModule
+-- functions[1]: IrFunction        // FunctionId(1)
    +-- name: "tag_labels"
    +-- params:
    |   +-- [0] IrFunctionParam { binding_id: BindingId(0), name: "tags",
    |                             ty: Some(Generic { base: Struct(StructId(0)), args: [Primitive(String)] }),
    |                             convention: Let }
    +-- return_type: Some(Generic { base: Struct(StructId(0)), args: [Primitive(String)] })
    +-- body: Some(IrExpr::MethodCall {
            receiver: IrExpr::For {
                var: "tag",
                var_ty: Primitive(String),
                var_binding_id: BindingId(1),
                collection: IrExpr::Reference {
                    path: ["tags"],
                    target: ReferenceTarget::Param(BindingId(0)),
                    ty: Generic { base: Struct(StructId(0)), args: [Primitive(String)] }
                },
                body: IrExpr::Reference {
                    path: ["tag"],
                    target: ReferenceTarget::Local(BindingId(1)),
                    ty: Primitive(String)
                },
                ty: Generic { base: Struct(StructId(1)), args: [Primitive(String)] }
            },
            method: "collect",
            method_idx: MethodIdx(4),
            args: [],
            dispatch: Static { impl_id: ImplId(2) },
            ty: Generic { base: Struct(StructId(0)), args: [Primitive(String)] }
        })
```

`StructId(0)` is the prelude `Array` and `StructId(1)` is the prelude
`Seq`. `ImplId(2)` is the prelude `extern impl Seq<T>`, and
`collect` is the method at index 4 in it. A backend lowers the methods
of that block as loop structure: see `IrModule::is_seq_intrinsic_impl`.

## Call of a Returned Closure

**FormaLang source:**

```formalang
fn doubler() -> (I32) -> I32 {
    (x) -> x * 2
}

pub fn eight() -> I32 {
    doubler()(4)
}
```

The callee of `doubler()(4)` is not a name. The parser gives the AST
node `Expr::Call`, and the lowering gives an `IrExpr::CallClosure`
whose `closure` is the call `doubler()`.

**IR structure** (after `ResolveReferencesPass`):

```text
IrModule
+-- functions[1]: IrFunction        // FunctionId(1)
|   +-- name: "doubler"
|   +-- return_type: Some(Closure { param_tys: [(Let, Primitive(I32))],
|   |                               return_ty: Primitive(I32) })
|   +-- body: Some(IrExpr::Closure {
|           params: [(Let, BindingId(0), "x", Primitive(I32))],
|           captures: [],
|           body: IrExpr::BinaryOp {
|               left: IrExpr::Reference { path: ["x"], target: Local(BindingId(0)), ... },
|               op: Mul,
|               right: IrExpr::Literal { value: Number(... Integer(2) ...), ty: Primitive(I32) },
|               ty: Primitive(I32)
|           },
|           ty: Closure { param_tys: [(Let, Primitive(I32))], return_ty: Primitive(I32) }
|       })
|
+-- functions[2]: IrFunction        // FunctionId(2)
    +-- name: "eight"
    +-- return_type: Some(Primitive(I32))
    +-- body: Some(IrExpr::CallClosure {
            closure: IrExpr::FunctionCall {
                path: ["doubler"],
                function_id: Some(FunctionId(1)),
                args: [],
                ty: Closure { param_tys: [(Let, Primitive(I32))], return_ty: Primitive(I32) }
            },
            args: [(None, IrExpr::Literal { value: Number(... Integer(4) ...), ty: Primitive(I32) })],
            ty: Primitive(I32)
        })
```

`ClosureConversionPass` then lifts the closure body to a function, and
`DefunctionalisePass` can turn the `CallClosure` into a direct call.
See [Built-in Passes](../architecture/passes.md).
