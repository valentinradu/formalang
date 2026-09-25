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
  ok(value: T),
  error(err: E)
}

pub enum Option<T> {
  some(value: T),
  none
}
```

## Generic Instantiation

```formalang
pub struct Box<T> { value: T }
pub struct Pair<A, B> { first: A, second: B }
pub enum Result<T, E> { ok(value: T), error(err: E) }
pub enum Option<T> { some(value: T), none }

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

// A variant path takes the type arguments too
pub let also_nothing = Option<I32>.none
```

## Generic Methods

A method can declare its own type parameters, after its name. They
sit beside the type parameters of the impl block:

```formalang
pub struct Box<T> {
  value: T
}

impl Box<T> {
  fn map<U>(self, f: (T) -> U) -> Box<U> {
    Box(value: f(self.value))
  }
}

pub let flag = Box(value: 3).map(f: (v) -> v > 2)   // Box<Boolean>
```

**Rules**:

- A method call takes no `<...>`. The arguments give each type
  parameter its type, so each one must appear in the type of a
  parameter with no default (E147). A call may leave out a parameter
  with a default, and then that parameter gives no type
- A closure argument can give a type parameter its type through its
  return type: `U` above takes the type that `f` answers. A closure
  parameter with no type cannot: in `fn apply<U>(self, f: (U) -> I32)`,
  the call `b.apply(f: (x) -> 1)` gives `x` no type (E145). Write it:
  `(x: String) -> 1`
- A method type parameter cannot repeat a name of its impl block, or of
  the type when the impl block names none (E084)
- A trait method cannot declare type parameters (E146)
- A type parameter hides a type with the same name inside its
  definition

## Type Constraints

```formalang
pub trait Named { name: String }
pub trait Layout { width: I32 }
pub trait Renderable { fn render(self) -> Boolean }
pub trait Clickable { fn click(self) -> Boolean }

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
- Type arguments must match parameter count (arity). A wrong count,
  or type arguments on a type that takes none, is the error
  `GenericArityMismatch`
- A type parameter is visible only in its own definition. A use
  outside it is the error `OutOfScopeTypeParameter`
- Type inference works when types can be determined. Two arguments
  that give one type parameter two types are a `TypeMismatch`
- A free function gives each type parameter its type through `<...>`
  at the call, or through an argument. `fn make<T>() -> I32` needs
  `make<I32>()`: a bare `make()` is the error
  `UninferableMethodTypeParameter` (E147)
- Constraints must reference existing traits: an unknown name is the
  error `UndefinedTrait`, and a struct is the error `NotATrait`
- A constraint holds wherever the type argument comes from — written at
  the call site (`total<Square>(item: s)`) or inferred from the
  argument (`total(item: s)`)

```formalang,reject=GenericConstraintViolation
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

## Monomorphisation

The `MonomorphisePass` clones generic definitions per unique
argument tuple after parsing: see
[Built-in Passes / MonomorphisePass](../developer/architecture/passes.md#monomorphisepass).

The set of clones must be finite. A generic function that calls
itself with a larger type argument than its own, such as `Box<T>` for
`T`, would need a new clone at each level. The pass stops at 32 levels
with the error `InstantiationDepthExceeded` (E148). A written type that
nests its type arguments more than 32 levels deep gets the same error.

```formalang,reject=InstantiationDepthExceeded
struct Box<T> { value: T }

fn grow<T>(x: T, n: I32) -> I32 {
  if n <= 0 { 0 } else { 1 + grow(x: Box(value: x), n: n - 1) }
}

pub fn probe() -> I32 { grow(x: 1, n: 3) }
```

A generic function that calls itself must call itself with the same
type arguments. Wrap the growing value in a non-generic type, or give
it a fixed type.
