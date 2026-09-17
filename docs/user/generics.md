# Generics

Full generic type system with constraints and type inference.

## Generic Structs

```formalang
// Single type parameter
pub struct Box<T> {
  value: T
}

// Multiple type parameters
pub struct Pair<A, B> {
  first: A,
  second: B
}

// With constraints
pub trait Layout {
  width: I32
}

pub struct Container<T: Layout> {
  items: [T],
  gap: I32
}

// Multiple constraints
pub trait Renderable { fn render(self) -> Boolean }
pub trait Clickable { fn click(self) -> Boolean }

pub struct Widget<T: Renderable + Clickable> {
  component: T
}
```

## Generic Traits

```formalang
pub trait Collection<T> {
  items: [T]
}

pub trait Comparable<T> {
  fn compare(self, other: T) -> I32
}
```

## Generic Enums

```formalang
pub enum Result<T, E> {
  ok(value: T)
  error(err: E)
}

pub enum Option<T> {
  some(value: T)
  none
}
```

## Generic Instantiation

```formalang
// With explicit type arguments
pub let string_box = Box<String>(value: "hello")
pub let number_box = Box<I32>(value: 42)
pub let pair = Pair<I32, Boolean>(first: 42, second: true)

// Type inference (when inferrable)
pub let inferred_box = Box(value: "inferred as String")
pub let inferred_pair = Pair(first: 10, second: true)

// Generic enums
pub let success: Result<String, I32> = .ok(value: "success")
pub let failure: Result<String, I32> = .error(err: 404)
pub let maybe: Option<I32> = .some(value: 42)
pub let nothing: Option<I32> = .none
```

## Type Constraints

```formalang
// Single constraint
pub struct Wrapper<T: Named> {
  item: T
}

// Multiple constraints
pub struct Interactive<T: Renderable + Clickable> {
  component: T
}

// Constraint on trait field
pub trait Container<T: Layout> {
  items: [T]
}
```

**Rules**:

- Type parameters use `<T>`, `<A, B>`, etc.
- Constraints use `:` syntax: `<T: Constraint>`
- Multiple constraints use `+`: `<T: A + B>`
- Type arguments must match parameter count (arity)
- Type inference works when types can be determined
- Constraints must reference existing traits
- A constraint holds wherever the type argument comes from — written at
  the call site (`total<Square>(item: s)`) or inferred from the
  argument (`total(item: s)`)

```formalang
pub trait Shape { fn area(self) -> I32 }
pub struct Square { side: I32 }
pub struct Plain { n: I32 }

impl Shape for Square {
  fn area(self) -> I32 { self.side * self.side }
}

fn total<T: Shape>(item: T) -> I32 { item.area() }

let a: I32 = total(item: Square(side: 3))  // ok
let b: I32 = total(item: Plain(n: 1))      // error E081: Plain is not a Shape
```

The `MonomorphisePass` clones generic definitions per unique
argument tuple after parsing: see
[Built-in Passes / MonomorphisePass](../developer/architecture/passes.md#monomorphisepass).
