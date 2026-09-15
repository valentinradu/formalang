use super::super::types::{IrEnum, IrStruct};
use super::super::{EnumId, ResolvedType, StructId};
use super::IrModule;

impl IrModule {
    /// Prelude-defined `Array<T>` struct id. The four built-in compound
    /// types live in `src/prelude.fv` as ordinary generic definitions;
    /// these accessors hand back the ids the lowering and IR walkers
    /// need to identify built-ins without hardcoding name strings.
    #[must_use]
    pub fn prelude_array_id(&self) -> Option<StructId> {
        self.struct_id("Array")
    }

    /// Prelude-defined `Seq<T>` struct id.
    ///
    /// A backend uses this to recognise the sequence combinators. They
    /// wear the `extern impl` shape but are **not** host-provided:
    /// every one takes a closure, and a closure has no representation
    /// a host can call. Lower them as loop structure; binding one to a
    /// host symbol is a bug.
    #[must_use]
    pub fn prelude_seq_id(&self) -> Option<StructId> {
        self.struct_id("Seq")
    }

    /// True iff `impl_id` names the prelude's `extern impl Seq<T>`
    /// block, whose methods the backend must lower rather than bind.
    #[must_use]
    pub fn is_seq_intrinsic_impl(&self, impl_id: crate::ir::ImplId) -> bool {
        let Some(seq) = self.prelude_seq_id() else {
            return false;
        };
        self.impls
            .get(impl_id.0 as usize)
            .is_some_and(|i| i.struct_id() == Some(seq))
    }

    /// Prelude-defined `Dictionary<K, V>` struct id.
    #[must_use]
    pub fn prelude_dictionary_id(&self) -> Option<StructId> {
        self.struct_id("Dictionary")
    }

    /// Prelude-defined `Range<T>` struct id.
    #[must_use]
    pub fn prelude_range_id(&self) -> Option<StructId> {
        self.struct_id("Range")
    }

    /// Prelude-defined `Optional<T>` enum id.
    #[must_use]
    pub fn prelude_optional_id(&self) -> Option<EnumId> {
        self.enum_id("Optional")
    }

    /// True iff `id` points at a prelude-defined built-in struct
    /// (`Array`, `Seq`, `Dictionary`, `Range`).
    #[must_use]
    pub fn is_prelude_struct(&self, id: StructId) -> bool {
        Some(id) == self.prelude_array_id()
            || Some(id) == self.prelude_seq_id()
            || Some(id) == self.prelude_dictionary_id()
            || Some(id) == self.prelude_range_id()
    }

    /// True iff `id` points at the prelude-defined `Optional<T>` enum.
    #[must_use]
    pub fn is_prelude_enum(&self, id: EnumId) -> bool {
        Some(id) == self.prelude_optional_id()
    }

    /// Iterate over user-defined structs only, skipping the prelude
    /// built-ins (`Array`, `Seq`, `Dictionary`, `Range`). Use this when
    /// a test wants the user-authored structs without indexing past the
    /// prelude's leading slots.
    pub fn user_structs(&self) -> impl Iterator<Item = &IrStruct> {
        self.structs.iter().enumerate().filter_map(move |(i, s)| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "struct count fits in u32 by construction (add_struct guards the cast)"
            )]
            let id = StructId(i as u32);
            if self.is_prelude_struct(id) {
                None
            } else {
                Some(s)
            }
        })
    }

    /// Iterate over user-defined enums only, skipping the prelude-built-in
    /// `Optional<T>` enum.
    pub fn user_enums(&self) -> impl Iterator<Item = &IrEnum> {
        let optional = self.prelude_optional_id();
        self.enums.iter().enumerate().filter_map(move |(i, e)| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "enum count fits in u32 by construction (add_enum guards the cast)"
            )]
            let id = EnumId(i as u32);
            if Some(id) == optional {
                None
            } else {
                Some(e)
            }
        })
    }

    /// If `ty` is `Seq<T>` (the prelude-defined struct), return `T`.
    #[must_use]
    pub fn seq_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let seq = self.prelude_seq_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == seq && args.len() == 1 {
                return args.first();
            }
        }
        None
    }

    /// If `ty` is `Array<T>` (the prelude-defined struct), return `T`.
    /// Built-in compound types share the `Generic` variant with user-
    /// defined generics; these helpers let callers introspect by shape
    /// without needing prelude IDs themselves.
    #[must_use]
    pub fn array_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let arr = self.prelude_array_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == arr && args.len() == 1 {
                return args.first();
            }
        }
        None
    }

    /// If `ty` is `Dictionary<K, V>`, return `(K, V)`.
    #[must_use]
    pub fn dictionary_kv_ty<'a>(
        &self,
        ty: &'a ResolvedType,
    ) -> Option<(&'a ResolvedType, &'a ResolvedType)> {
        let did = self.prelude_dictionary_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == did {
                if let [k, v] = args.as_slice() {
                    return Some((k, v));
                }
            }
        }
        None
    }

    /// If `ty` is `Range<T>`, return `T`.
    #[must_use]
    pub fn range_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let rid = self.prelude_range_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == rid && args.len() == 1 {
                return args.first();
            }
        }
        None
    }

    /// If `ty` is `Optional<T>`, return `T`.
    #[must_use]
    pub fn optional_inner_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let opt = self.prelude_optional_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Enum(id),
            args,
        } = ty
        {
            if *id == opt && args.len() == 1 {
                return args.first();
            }
        }
        None
    }
}
