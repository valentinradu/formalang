# Structs

Structs define data types: a named record of typed fields, optionally
generic over type parameters.

## Definitions

A comma separates two fields. A line break after the comma is
optional. A line break alone does not separate two fields.

```formalang
pub trait Layout { width: I32 }

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

A line break alone between two fields is an error:

```formalang,reject=ParseError
pub struct Point {
  x: I32
  y: I32
}
```

A comma after the last field is allowed.

## Instantiation

A struct literal names each field: `Point(x: 10, y: 20)`. A positional
argument is the error `PositionalArgInStruct`. Each field needs a value,
with two exceptions: a field with a default value, and an optional
field, which is `nil` when you leave it out. A missing field is the
error `MissingField`, and a name that is not a field is the error
`UnknownField`.

```formalang
pub struct Point { x: I32, y: I32 }

pub struct User {
  name: String,
  email: String,
  age: I32,
  nickname: String?
}

pub struct Box<T> { value: T }

pub struct Pair<A, B> { first: A, second: B }

// Basic instantiation
pub let point = Point(x: 10, y: 20)

// Multi-line instantiation; `nickname` is optional, so it is nil
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

pub fn run_checks() {
  assert(condition: point.x + point.y == 30)
  assert(condition: user.age == 30)
  assert(condition: pair.second)
}
```

## Field Defaults

A field can declare a default value with `= expr`. A literal that
leaves out the field gets the default:

```formalang
pub struct Config {
  retries: I32 = 3,
  name: String = "default"
}

pub fn run_checks() {
  let c = Config()
  assert(condition: c.retries == 3)
  let d = Config(retries: 9)
  assert(condition: d.retries == 9)
  assert(condition: d.name == "default")
}
```

## Adding Methods

To attach methods to a struct, write an `impl` block: see
[Traits & Impls](traits.md#impl-blocks).

## Each Name Appears Once

Two fields of one name leave no answer to what the name means, so a
definition may not declare one twice. The same holds for the
parameters of a function, a method, or a trait method, and for the
payload fields of an enum variant.

```formalang,reject=DuplicateDefinition
pub struct P {
  x: I32,
  x: I32          // error: Duplicate definition
}

fn g(a: I32, a: I32) -> I32 { a }   // error: Duplicate definition
```

Shadowing is a different thing and stays legal inside a body. A later
`let` of the same name introduces a second binding that hides the
first, and the name has a defined meaning at every point:

```formalang
pub fn f() -> I32 {
  let a = 1
  let a = a + 1   // ok: a is 2 from here on
  a
}
```

At module level a name is a definition, so two `let` bindings of one
name at module level are a duplicate definition.

The rule covers every place where a name is declared. Each of these is
the error `DuplicateDefinition`:

- two top-level definitions of one name: two structs, a struct and an
  enum, a function and a `let`, two traits, or two modules;
- two variants of one enum with one name;
- two fields or two methods of one trait with one name;
- two functions or two methods with one signature. A different return
  type does not make a second signature (see
  [Function Overloading](functions.md#function-overloading));
- two names of one pattern: a destructuring pattern, a match arm, or
  the parameters of a closure;
- two arguments of one call with one label, and two fields of one
  literal with one name.

Two type parameters of one name are the error `DuplicateGenericParam`,
and two match arms for one variant are the error `DuplicateMatchArm`.
