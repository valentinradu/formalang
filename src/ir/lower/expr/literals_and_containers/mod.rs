//! Lowering for literal, container and instantiation expressions:
//! `Literal`, `Array`, `Tuple`, `Dict{Literal,Access}`, struct/enum
//! instantiation and bare-function/struct invocation paths.

mod containers;
mod enums;
mod invocation;
mod literal;
