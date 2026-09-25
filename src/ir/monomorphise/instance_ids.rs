//! Keeping the id of a struct or enum instance in step with its type.

use crate::ir::{GenericBase, IrExpr, IrModule, ResolvedType};

use super::expr_walk::for_each_module_expr_mut;

/// Make the `struct_id` of every `StructInst`, and the `enum_id` of
/// every `EnumInst`, name the definition that its type names.
///
/// Phase 2 rewrites the type of an instance of `Box<I32>` to
/// `Box__I32`, but the id still names the generic `Box`. Compaction
/// then drops `Box` and renumbers what is left, so the id named
/// whatever took its index: another specialisation, or an unrelated
/// struct. The type is correct after both steps, so the id follows it.
///
/// Run after compaction, when every type holds its final id. An
/// instance of an external type keeps its id: its type names no local
/// definition.
pub(in crate::ir::monomorphise) fn resync_instance_ids(module: &mut IrModule) {
    for_each_module_expr_mut(module, &mut |expr| {
        if let IrExpr::StructInst {
            struct_id,
            ty:
                ResolvedType::Struct(id)
                | ResolvedType::Generic {
                    base: GenericBase::Struct(id),
                    ..
                },
            ..
        } = expr
        {
            *struct_id = Some(*id);
        } else if let IrExpr::EnumInst {
            enum_id,
            ty:
                ResolvedType::Enum(id)
                | ResolvedType::Generic {
                    base: GenericBase::Enum(id),
                    ..
                },
            ..
        } = expr
        {
            *enum_id = Some(*id);
        }
    });
}
