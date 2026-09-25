# Traits & Impls

Traits declare requirements (fields and method signatures); `impl` blocks
attach methods to a concrete type or declare conformance.

## Trait Definitions

```formalang
// Fields only
pub trait Named {
  name: String
}

// Fields and methods
pub trait Shape {
  color: String
  fn area(self) -> I32
  fn perimeter(self) -> I32
}

// Methods only
pub trait Drawable {
  fn draw(self) -> Boolean
  fn visible(self) -> Boolean
}

pub trait Identifiable {
  id: I32
}

// Trait composition (inheritance)
pub trait Entity: Named + Identifiable {
  createdAt: I32
}

// Generic trait
pub trait Collection<T> {
  items: [T]
}
```

**Trait Rules**:

- Fields listed without `fn` are structural requirements (the struct must have them)
- `fn` signatures listed without a body are method requirements
- A line break separates two members of a trait. A comma is also
  allowed
- Trait composition (`+`) combines requirements from multiple traits
- A type satisfies a trait by providing all required fields and all required methods
- A trait method cannot declare type parameters (E146)

## Impl Blocks

Impl blocks add methods to a struct (inherent impl) or declare trait
conformance (impl Trait for Struct).

**Inherent impl**: methods belong to the struct:

```formalang
pub struct Counter {
  value: I32
}

impl Counter {
  fn increment(self) -> I32 {
    self.value + 1
  }

  fn reset(self) -> Counter {
    Counter(value: 0)
  }
}

pub fn run_checks() {
  let c = Counter(value: 4)
  assert(condition: c.increment() == 5)
  assert(condition: c.reset().value == 0)
}
```

### The `self` parameter

`self` is the receiver of a method. It exists only in a method that
declares it:

- `self` must be the first parameter. In another position it is a
  `ParseError`.
- A method without a `self` parameter is a static method. A read of
  `self` in it is the error `UndefinedReference`.
- A free function has no receiver, so a `self` parameter there is a
  `ParseError`.
- `self` is a keyword, so it cannot name a binding, a parameter, a
  field or a function.

`self` takes a convention, as each parameter does: `self`, `mut self`
or `sink self`. See
[Functions / Parameter Conventions](functions.md#parameter-conventions).

```formalang,reject=UndefinedReference
pub struct Counter { value: I32 }

impl Counter {
  fn get() -> I32 {
    self.value          // error: this method declares no `self`
  }
}
```

### Static methods

A call on the type, `Type.method(...)`, reaches a static method. This
is the form of Swift. A call through a value of the type, `c.zero()`,
reaches it too:

```formalang
pub struct Counter {
  value: I32
}

impl Counter {
  fn zero() -> Counter {
    Counter(value: 0)
  }

  fn starting(at: I32) -> Counter {
    Counter(value: at)
  }

  fn reset(self) -> Counter {
    Counter.zero()          // inside the impl, name the type too
  }
}

pub fn run_checks() {
  assert(condition: Counter.zero().value == 0)
  assert(condition: Counter.starting(at: 5).reset().value == 0)
}
```

**Rules**:

- `Name.method(...)` is a static call only when `Name` is a struct. On
  an enum, `Name.member(...)` is a variant
- A call on the type reaches only a method with no `self`. A method
  that takes `self` needs a value: `Counter.get()` is the error E151
  (`NotAStaticMethod`)
- A bare name names a function, not a method, as in Rust: inside the
  impl, a bare `zero()` is the error `UndefinedReference`. Write
  `Counter.zero()`

```formalang,reject=NotAStaticMethod
pub struct Counter { value: I32 }

impl Counter {
  fn get(self) -> I32 { self.value }
}

pub fn f() -> I32 {
  Counter.get()           // error E151: get takes self
}
```

## Impl Trait for Type

Declare that a type conforms to a trait using `impl Trait for Type`:

```formalang
pub trait Named {
  name: String
}

pub trait Drawable {
  fn draw(self) -> Boolean
}

pub struct Circle {
  name: String,
  radius: I32
}

// Declare conformance (fields are checked against struct definition)
impl Named for Circle {}

// Provide required methods
impl Drawable for Circle {
  fn draw(self) -> Boolean {
    self.radius > 0
  }
}
```

Trait composition requires a separate impl block for each trait in the hierarchy:

```formalang
pub trait Base {
  fn id(self) -> I32
}

pub trait Extended: Base {
  fn name(self) -> String
}

pub struct Item {
  value: I32
}

impl Base for Item {
  fn id(self) -> I32 {
    self.value
  }
}

impl Extended for Item {
  fn name(self) -> String {
    "item"
  }
}
```

**Conformance rules**:

- `impl Trait for Type` is the only way to declare trait conformance
- A struct or an enum can conform to a trait. Struct fields required
  by the trait must be present in the struct definition
  (`MissingTraitField`, `TraitFieldTypeMismatch`). An enum has no
  fields, so it can only meet a trait with no field requirements
- All `fn` signatures in the trait must be implemented in the impl
  block (`MissingTraitMethod`)
- Method signatures must match exactly: the parameter count, each
  label, each parameter type, the convention of `self`, and the return
  type (`TraitMethodSignatureMismatch`)
- Each trait in a chain needs its own impl block. `impl Extended for
  Item` does not satisfy `Base`: without `impl Base for Item`, the
  error is `MissingTraitMethod`
- An impl block provides only the methods of its own trait. A method
  of the parent trait in the child's impl block is a
  `DuplicateDefinition`
- A trait impl is part of its type. A module that imports the type
  gets the impl too, with no `use` of the trait. See
  [Module System](modules.md#what-an-import-brings)

## Impls on Primitive Types

A primitive type (`String`, `I32`, `I64`, `F32`, `F64`, `Boolean`)
takes only an `extern impl`: the host provides its methods. A regular
`impl` or a trait impl on a primitive is the error `ImplOnPrimitive`.
See [Extern Declarations](extern.md#extern-impl-on-primitive-types).

```formalang,reject=ImplOnPrimitive
impl I32 {
  fn double(self) -> I32 { self * 2 }   // error: must be an extern impl
}
```

## Trait-Bounded Polymorphism

A trait is a **constraint, never the type of a value**. Write
`<T: Printable>`, not `item: Printable`. Every call in `FormaLang`
dispatches statically, so there is no vtable and no indirect call.

```formalang,reject=TraitUsedAsValueType
pub trait Printable {
  fn label(self) -> String
}

pub struct Doc {
  text: String
}

impl Printable for Doc {
  fn label(self) -> String { self.text }
}

// Correct: a generic bound. Monomorphisation makes one specialised
// function per concrete type, and the call is direct.
fn print_it<T: Printable>(item: T) -> String {
  item.label()
}

// Error E063: a trait cannot be the type of a value.
fn print_any(item: Printable) -> String {
  item.label()
}
```

The rule holds at every value position: a parameter, a return type, a
`let` annotation, a struct field, an enum variant field, an array
element, a dictionary value, and a closure parameter.

### When the type is chosen at run time

A generic bound fixes one concrete type per call. For a value that may
hold any of several types, declare an enum with one variant per type
and `match` on it:

```formalang
pub struct Square { side: I32 }
pub struct Rect { w: I32, h: I32 }

impl Square { fn area(self) -> I32 { self.side * self.side } }
impl Rect { fn area(self) -> I32 { self.w * self.h } }

pub enum AnyShape {
  square(value: Square),
  rect(value: Rect)
}

fn area_of(shape: AnyShape) -> I32 {
  match shape {
    .square(value): value.area(),
    .rect(value): value.area()
  }
}

// Two branches, one type — no cast needed.
fn pick(k: I32) -> AnyShape {
  if k == 0 {
    .square(value: Square(side: 2))
  } else {
    .rect(value: Rect(w: 2, h: 3))
  }
}
```

The enum lists its variants in one place, so adding a type means
editing the enum and each `match`. In exchange the compiler checks that
every case is handled, and the generated code needs no vtable.

### Generic Traits

Traits can themselves be generic, and constraints / impls can carry
the concrete arguments:

```formalang
pub trait Container<T> {
  fn get(self) -> T
}

pub struct Box {
  value: I32
}

impl Container<I32> for Box {
  fn get(self) -> I32 { self.value }
}

fn unwrap<T: Container<I32>>(b: T) -> I32 {
  b.get()
}

pub fn run_checks() {
  assert(condition: unwrap(b: Box(value: 7)) == 7)
}
```

A bound on a generic trait gives its types. Through `T: Container<I32>`,
the method `get` returns an `I32`, and a parameter of type `T` in a
trait method takes an `I32`. A call through the bound is checked
against these types, and against the argument count and the labels of
the trait method. A generic trait needs its type arguments at each
bound and each impl: `<T: Container>` is the error
`MissingGenericArguments`, and a wrong count is the error
`GenericArityMismatch`.

One type can implement two instances of one generic trait. Each
instance is its own impl, and a bound says which one a call means. A
second impl of the same instance is a `DuplicateDefinition`. A call
that both instances answer, such as `p.get()` on a `Pair` below, is
the error `AmbiguousCall`: make the call through a bound.

```formalang
pub trait Container<T> {
  fn get(self) -> T
}

pub struct Pair {
  n: I32,
  s: String
}

impl Container<I32> for Pair {
  fn get(self) -> I32 { self.n }
}

impl Container<String> for Pair {
  fn get(self) -> String { self.s }
}

fn number<T: Container<I32>>(b: T) -> I32 { b.get() }
fn text<T: Container<String>>(b: T) -> String { b.get() }

pub fn run_checks() {
  let p = Pair(n: 7, s: "seven")
  assert(condition: number(b: p) == 7)
  assert(condition: text(b: p) == "seven")
}
```

The monomorphisation pass clones generic traits, structs, enums,
and functions per unique argument tuple, then rewrites every
reference (including `DispatchKind::Virtual` on now-concrete
receivers) to point at the specialised clone. After mono runs, no
generic definitions remain in the IR.

**Allowed trait positions**:

- Generic constraint: `<T: Trait>` or `<T: Trait<X>>`
- Impl target: `impl Trait for Foo` or `impl Trait<X> for Foo`
- Trait composition: `trait A: B + C`

**Rejected trait positions** (use a generic bound instead):

- Function parameter type: `fn foo(x: Trait)` ✗
- Function return type: `fn make() -> Trait` ✗
- Let annotation: `let x: Trait = ...` ✗
- Struct/enum field: `field: Trait` ✗
- Closure params/return: `(x: Trait) -> I32` ✗
