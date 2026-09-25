# Module System

## Use Statements

A module is a `.fv` file. A `use` statement imports items from another
module. Take a project with these files:

```text
project/
├── main.fv
├── types.fv
├── components.fv
├── widgets/
│   └── controls.fv
└── utils/
    └── helpers.fv
```

```formalang,file=types.fv
pub struct User { name: String, age: I32 }

impl User {
  fn is_adult(self) -> Boolean { self.age >= limit() }
}

// Private: other modules cannot import it, but the module's own code
// still calls it.
fn limit() -> I32 { 18 }
```

```formalang,file=components.fv
pub struct Button { label: String }
pub struct Text { value: String }
pub struct VStack { gap: I32 }
```

```formalang,file=widgets/controls.fv
pub struct Slider { value: I32 }
```

```formalang,file=utils/helpers.fv
pub fn formatDate(day: I32) -> String { "day " }
```

`main.fv` imports from each of them:

```formalang
// Import a single item
use types::User

// Import multiple items
use components::{Button, Text, VStack}

// Import from a nested path: widgets/controls.fv
use widgets::controls::Slider
use utils::helpers::formatDate

pub fn run_checks() {
  let user = User(name: "Ada", age: 36)
  assert(condition: user.is_adult())        // the impl block comes too
  assert(condition: Button(label: "ok").label == "ok")
  assert(condition: Slider(value: 3).value == 3)
  assert(condition: formatDate(day: 1) == "day ")
}
```

**Module Resolution**:

- Modules map to `.fv` files
- Path separators use `::`. The segments before the last one name the
  file: `use widgets::controls::Slider` reads `widgets/controls.fv`.
  The last segment, or each name in `{ }`, names an item of that file
- A `use` imports items. It does not import a module as a whole:
  `use utils::helpers` looks for an item `helpers` in `utils.fv`
- A path that no file matches is the error `ModuleNotFound`. A name
  that the file does not declare is the error `ImportItemNotFound`
- Can only import `pub` items. A private item is the error
  `PrivateImport`
- No circular imports allowed: two modules that import each other are
  the error `CircularImport`

## What an Import Brings

An import brings the item and what belongs to it:

- A struct or an enum comes with its impl blocks, so its methods work
  in the importer.
- A trait impl is part of its type. The importer can call the methods
  of the impl with no `use` of the trait.
- The imported code keeps its own private helpers. `is_adult` above
  calls the private `limit`, and the importer cannot name `limit`.

```formalang,file=geom.fv
pub trait Area { fn area(self) -> I32 }

pub struct Square { side: I32 }

impl Area for Square {
  fn area(self) -> I32 { self.side * self.side }
}
```

```formalang
use geom::Square

pub fn run_checks() {
  assert(condition: Square(side: 3).area() == 9)   // no `use geom::Area`
}
```

## Re-exports

A plain `use` makes the name available inside the importing module
only. Another module cannot import the name through it. A `pub use`
exports the name again, so a module can collect items from several
files:

```formalang,file=prelude.fv
pub use types::User
pub use geom::Square
```

```formalang
use prelude::{User, Square}

pub fn run_checks() {
  assert(condition: Square(side: 2).area() == 4)
  assert(condition: User(name: "Bo", age: 9).age == 9)
}
```

The module `plain.fv` imports `Button` with a plain `use`:

```formalang,file=plain.fv
use components::Button
```

So another module cannot import `Button` from it:

```formalang,reject=PrivateImport
use plain::Button
```

## Nested Modules

Use `mod` blocks to create nested namespaces within a file:

```formalang
mod alignment {
  pub enum Vertical {
    top,
    center,
    bottom
  }

  pub enum Horizontal {
    left,
    center,
    right
  }
}

// Use with namespace path
pub let vertical: alignment::Vertical = .top
pub let horizontal: alignment::Horizontal = .center

// Can also import nested items
use alignment::Vertical

pub let v: Vertical = .bottom
```

Items of a module are private unless they are `pub`. Code outside the
module reaches only the `pub` items, by path (`alignment::Vertical`)
or by `use`. A private item is the error `VisibilityViolation` when a
path names it, and `PrivateImport` when a `use` names it. A nested
module that is not `pub` is private too:

```formalang,reject=VisibilityViolation
mod tools {
  pub fn shown() -> I32 { hidden() }   // ok: inside the module
  fn hidden() -> I32 { 1 }
}

pub let a: I32 = tools::shown()        // ok
pub let b: I32 = tools::hidden()       // error: 'hidden' is private
```

An inline module and a module file can have the same name. Then a
`use` of that name has two meanings, and the compiler refuses it with
the error `AmbiguousModulePath`. This page has the file `types.fv`:

```formalang,reject=AmbiguousModulePath
mod types {
  pub struct User { name: String }
}

use types::User
```

Rename the inline module, or the file.

**Multiple Levels**:

```formalang
mod ui {
  pub mod layout {
    pub enum Direction {
      horizontal,
      vertical
    }
  }

  pub struct Theme {
    primary: String,
    secondary: String
  }
}

pub let direction: ui::layout::Direction = .horizontal
pub let theme: ui::Theme = ui::Theme(
  primary: "#007AFF",
  secondary: "#5856D6"
)
```

## A Public Signature Names Public Types

A `pub` definition is what another module sees. Naming a private type
in one hands the reader a value whose type they cannot write down: they
can call the function, but they cannot declare a binding for the
result, pass it on, or name it in their own signature.

```formalang,reject=PrivateTypeInPublic
struct Hidden { x: I32 }

pub fn f() -> Hidden { Hidden(x: 1) }   // error E141
pub fn g(h: Hidden) -> I32 { h.x }      // error E141
pub struct Shown { h: Hidden }          // error E141

fn build() -> Hidden { Hidden(x: 1) }   // ok: not public
```

The rule reaches inside containers, because `[Hidden]` and `Hidden?`
name the same type a bare `Hidden` does. It covers each part of a
public definition that another module sees: the parameters and the
return type of a function (an `extern fn` too), the fields of a
struct, the payload fields of an enum, the fields and methods of a
trait, the type of a `let` (written or inferred), a type argument, a
tuple field, a closure type, and a generic bound. The expression of
a default value is not part of the signature: it may use a private
type, if the value has a public type. The rule
holds inside a nested module too.

This pairs with the closure rule: a struct that holds a closure field
cannot be `pub`, so no public signature may name it either. Such a
struct is built and read inside its own module, and the module exposes
the results instead. See
[Closures / Rules](closures.md).
