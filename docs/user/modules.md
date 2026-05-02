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
