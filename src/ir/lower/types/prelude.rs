use super::super::IrLowerer;
use crate::ir::ResolvedType;

impl IrLowerer<'_> {
    /// Construct `Optional<inner>` against the prelude-defined enum.
    /// Returns `None` if the prelude hasn't been registered yet.
    pub(in crate::ir::lower) fn optional_of(&self, inner: ResolvedType) -> Option<ResolvedType> {
        let id = self.module.prelude_optional_id()?;
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Enum(id),
            args: vec![inner],
        })
    }

    /// Construct `Array<inner>` against the prelude-defined struct.
    pub(in crate::ir::lower) fn array_of(&self, inner: ResolvedType) -> Option<ResolvedType> {
        let id = self.module.prelude_array_id()?;
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args: vec![inner],
        })
    }

    /// Construct `Seq<inner>` against the prelude-defined struct.
    pub(in crate::ir::lower) fn seq_of(&self, inner: ResolvedType) -> Option<ResolvedType> {
        let id = self.module.prelude_seq_id()?;
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args: vec![inner],
        })
    }

    /// If `ty` is `Seq<T>`, return `T`.
    pub(in crate::ir::lower) fn seq_element_ty(&self, ty: &ResolvedType) -> Option<ResolvedType> {
        self.module.seq_element_ty(ty).cloned()
    }

    /// Construct `Dictionary<key, value>` against the prelude-defined struct.
    pub(in crate::ir::lower) fn dictionary_of(
        &self,
        key: ResolvedType,
        value: ResolvedType,
    ) -> Option<ResolvedType> {
        let id = self.module.prelude_dictionary_id()?;
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args: vec![key, value],
        })
    }

    /// Construct `Range<inner>` against the prelude-defined struct.
    pub(in crate::ir::lower) fn range_of(&self, inner: ResolvedType) -> Option<ResolvedType> {
        let id = self.module.prelude_range_id()?;
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args: vec![inner],
        })
    }

    /// If `ty` is `Array<T>`, return `T`.
    pub(in crate::ir::lower) fn array_element_ty(&self, ty: &ResolvedType) -> Option<ResolvedType> {
        let arr = self.module.prelude_array_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == arr {
                if let [t] = args.as_slice() {
                    return Some(t.clone());
                }
            }
        }
        None
    }

    /// If `ty` is `Dictionary<K, V>`, return `(K, V)`.
    pub(in crate::ir::lower) fn dictionary_kv_ty(
        &self,
        ty: &ResolvedType,
    ) -> Option<(ResolvedType, ResolvedType)> {
        let did = self.module.prelude_dictionary_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == did {
                if let [k, v] = args.as_slice() {
                    return Some((k.clone(), v.clone()));
                }
            }
        }
        None
    }

    /// If `ty` is `Range<T>`, return `T`.
    pub(in crate::ir::lower) fn range_element_ty(&self, ty: &ResolvedType) -> Option<ResolvedType> {
        let rid = self.module.prelude_range_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == rid {
                if let [t] = args.as_slice() {
                    return Some(t.clone());
                }
            }
        }
        None
    }

    /// Element type for any iterable receiver (`Array<T>` or `Range<T>`).
    pub(in crate::ir::lower) fn iterator_element_ty(
        &self,
        ty: &ResolvedType,
    ) -> Option<ResolvedType> {
        // A `for` iterates an array, a range, or another sequence, so
        // pipelines chain: `for y in (for x in xs { x * 2 }) { ... }`.
        self.array_element_ty(ty)
            .or_else(|| self.range_element_ty(ty))
            .or_else(|| self.seq_element_ty(ty))
    }
}
