# Module System

## Use Statements

Import definitions from other modules:

```formalang
// Import single item
use components::Button

// Import multiple items
use components::{Button, Text, VStack}

// Import from nested paths
use ui::controls::Slider
use data::models::User

// Import from file
use types::User         // From types.fv
use utils::helpers      // From utils/helpers.fv
```

**Module Resolution**:

- Modules map to `.fv` files
- Path separators use `::`
- Can only import `pub` items
- No circular imports allowed

## Nested Modules

Use `mod` blocks to create nested namespaces within a file:

```formalang
mod alignment {
  pub enum Vertical {
    top
    center
    bottom
  }

  pub enum Horizontal {
    left
    center
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

**Multiple Levels**:

```formalang
mod ui {
  pub mod layout {
    pub enum Direction {
      horizontal
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

## File Structure Example

```text
project/
├── main.fv
├── types.fv
├── components/
│   ├── button.fv
│   └── text.fv
└── utils/
    └── helpers.fv
```

```formalang
// In main.fv
use types::User
use components::{Button, Text}
use utils::helpers::formatDate
```

## A Public Signature Names Public Types

A `pub` definition is what another module sees. Naming a private type
in one hands the reader a value whose type they cannot write down: they
can call the function, but they cannot declare a binding for the
result, pass it on, or name it in their own signature.

```formalang
struct Hidden { x: I32 }

pub fn f() -> Hidden { Hidden(x: 1) }   // error E141
pub fn g(h: Hidden) -> I32 { h.x }      // error E141
pub struct Shown { h: Hidden }          // error E141

fn build() -> Hidden { Hidden(x: 1) }   // ok: not public
```

The rule reaches inside containers, because `[Hidden]` and `Hidden?`
name the same type a bare `Hidden` does.

This pairs with the closure rule: a struct that holds a closure field
cannot be `pub`, so no public signature may name it either. Such a
struct is built and read inside its own module, and the module exposes
the results instead. See
[Closures / Rules](closures.md).
