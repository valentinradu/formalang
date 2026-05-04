# Structs

Structs define data types: a named record of typed fields, optionally
generic over type parameters.

## Definitions

```formalang
// Basic struct
pub struct Point {
  x: I32,
  y: I32
}

// With optional fields
pub struct User {
  name: String,
  email: String,
  age: I32,
  verified: Boolean,
  nickname: String?
}

// All struct fields share the binding's mutability. There is no
// per-field `mut` modifier; to update any field of a value, the
// binding must be `let mut`.
pub struct Counter {
  count: I32,
  label: String
}

// Generic struct
pub struct Box<T> {
  value: T
}

pub struct Pair<A, B> {
  first: A,
  second: B
}

// Generic with constraints
pub struct Container<T: Layout> {
  items: [T],
  gap: I32
}
```

## Instantiation

```formalang
pub struct Point { x: I32, y: I32 }

// Basic instantiation
pub let point = Point(x: 10, y: 20)

// Multi-line instantiation
pub let user = User(
  name: "Alice",
  email: "alice@example.com",
  age: 30
)

// Generic instantiation with type arguments
pub let box_str = Box<String>(value: "hello")
pub let pair = Pair<I32, Boolean>(first: 42, second: true)

// Type inference (type arguments optional when inferrable)
pub let box_inferred = Box(value: "inferred as String")
```

## Adding Methods

To attach methods to a struct, write an `impl` block: see
[Traits & Impls](traits.md#impl-blocks).
