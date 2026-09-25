//! Id translation for items that move from one module into another.
//!
//! An imported module keeps its own id space. When the linker copies
//! an item into the importing module, each id in the item gets the id
//! of the same item in the importing module. This file does that
//! translation for every id slot and every file id in an item.

use crate::ir::{
    DispatchKind, EnumId, FileId, FunctionId, GenericBase, ImplId, ImplTarget, IrBlockStatement,
    IrEnum, IrExpr, IrField, IrFunction, IrFunctionParam, IrFunctionSig, IrGenericParam, IrImpl,
    IrLet, IrSpan, IrStruct, IrTrait, IrTraitRef, LetId, ReferenceTarget, ResolvedType, StructId,
    TraitId,
};

use crate::ir::monomorphise::walkers::walk_expr_children_mut;

/// The new id of each old id, one table for each kind of item.
///
/// An index is an old id. The value at the index is the new id. The
/// file table maps an old `FileId` to a new one; index 0 is the
/// synthetic file and maps to itself.
#[derive(Debug, Default)]
pub(super) struct IdMaps {
    pub(super) structs: Vec<u32>,
    pub(super) enums: Vec<u32>,
    pub(super) traits: Vec<u32>,
    pub(super) impls: Vec<u32>,
    pub(super) functions: Vec<u32>,
    pub(super) lets: Vec<u32>,
    pub(super) files: Vec<u32>,
}

/// The new id of `old` in `table`. An id outside the table does not
/// name an item of the source module, so it stays as it is: the IR
/// verifier reports such an id, and a second error here adds nothing.
fn map(table: &[u32], old: u32) -> u32 {
    usize::try_from(old)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .unwrap_or(old)
}

impl IdMaps {
    fn structure(&self, id: &mut StructId) {
        id.0 = map(&self.structs, id.0);
    }
    fn enumeration(&self, id: &mut EnumId) {
        id.0 = map(&self.enums, id.0);
    }
    fn trait_id(&self, id: &mut TraitId) {
        id.0 = map(&self.traits, id.0);
    }
    fn implementation(&self, id: &mut ImplId) {
        id.0 = map(&self.impls, id.0);
    }
    fn function_id(&self, id: &mut FunctionId) {
        id.0 = map(&self.functions, id.0);
    }
    fn module_let(&self, id: &mut LetId) {
        id.0 = map(&self.lets, id.0);
    }
    fn span(&self, span: &mut IrSpan) {
        let FileId(old) = span.file;
        span.file = FileId(map(&self.files, old));
    }

    pub(super) fn ty(&self, ty: &mut ResolvedType) {
        match ty {
            ResolvedType::Struct(id) => self.structure(id),
            ResolvedType::Enum(id) => self.enumeration(id),
            ResolvedType::Trait(id) => self.trait_id(id),
            ResolvedType::Generic { base, args } => {
                match base {
                    GenericBase::Struct(id) => self.structure(id),
                    GenericBase::Enum(id) => self.enumeration(id),
                    GenericBase::Trait(id) => self.trait_id(id),
                }
                for arg in args {
                    self.ty(arg);
                }
            }
            ResolvedType::Tuple(fields) => {
                for (_, field) in fields {
                    self.ty(field);
                }
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                for (_, param) in param_tys {
                    self.ty(param);
                }
                self.ty(return_ty);
            }
            ResolvedType::External { type_args, .. } => {
                for arg in type_args {
                    self.ty(arg);
                }
            }
            ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
        }
    }

    fn trait_ref(&self, tr: &mut IrTraitRef) {
        self.trait_id(&mut tr.trait_id);
        for arg in &mut tr.args {
            self.ty(arg);
        }
    }

    fn generics(&self, params: &mut [IrGenericParam]) {
        for param in params {
            for constraint in &mut param.constraints {
                self.trait_ref(constraint);
            }
        }
    }

    fn field(&self, field: &mut IrField) {
        self.ty(&mut field.ty);
        if let Some(default) = &mut field.default {
            self.expr(default);
        }
        self.span(&mut field.span);
    }

    fn param(&self, param: &mut IrFunctionParam) {
        if let Some(ty) = &mut param.ty {
            self.ty(ty);
        }
        if let Some(default) = &mut param.default {
            self.expr(default);
        }
        self.span(&mut param.span);
    }

    pub(super) fn structure_def(&self, s: &mut IrStruct) {
        for tr in &mut s.traits {
            self.trait_ref(tr);
        }
        for field in &mut s.fields {
            self.field(field);
        }
        self.generics(&mut s.generic_params);
        self.span(&mut s.span);
    }

    pub(super) fn enum_def(&self, e: &mut IrEnum) {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                self.field(field);
            }
            self.span(&mut variant.span);
        }
        self.generics(&mut e.generic_params);
        self.span(&mut e.span);
    }

    fn signature(&self, sig: &mut IrFunctionSig) {
        for param in &mut sig.params {
            self.param(param);
        }
        if let Some(ret) = &mut sig.return_type {
            self.ty(ret);
        }
        self.span(&mut sig.span);
    }

    pub(super) fn trait_def(&self, t: &mut IrTrait) {
        for id in &mut t.composed_traits {
            self.trait_id(id);
        }
        for field in &mut t.fields {
            self.field(field);
        }
        for sig in &mut t.methods {
            self.signature(sig);
        }
        self.generics(&mut t.generic_params);
        self.span(&mut t.span);
    }

    pub(super) fn impl_def(&self, imp: &mut IrImpl) {
        match &mut imp.target {
            ImplTarget::Struct(id) => self.structure(id),
            ImplTarget::Enum(id) => self.enumeration(id),
            ImplTarget::Primitive(_) => {}
        }
        if let Some(tr) = &mut imp.trait_ref {
            self.trait_ref(tr);
        }
        self.generics(&mut imp.generic_params);
        for f in &mut imp.functions {
            self.function(f);
        }
        self.span(&mut imp.span);
    }

    pub(super) fn function_def(&self, f: &mut IrFunction) {
        self.function(f);
    }

    fn function(&self, f: &mut IrFunction) {
        self.generics(&mut f.generic_params);
        for param in &mut f.params {
            self.param(param);
        }
        if let Some(ret) = &mut f.return_type {
            self.ty(ret);
        }
        if let Some(body) = &mut f.body {
            self.expr(body);
        }
        self.span(&mut f.span);
    }

    pub(super) fn let_def(&self, l: &mut IrLet) {
        self.ty(&mut l.ty);
        self.expr(&mut l.value);
        self.span(&mut l.span);
    }

    fn target(&self, target: &mut ReferenceTarget) {
        match target {
            ReferenceTarget::Function(id) => self.function_id(id),
            ReferenceTarget::Struct(id) => self.structure(id),
            ReferenceTarget::Enum(id) => self.enumeration(id),
            ReferenceTarget::Trait(id) => self.trait_id(id),
            ReferenceTarget::ModuleLet(id) => self.module_let(id),
            ReferenceTarget::Local(_)
            | ReferenceTarget::Param(_)
            | ReferenceTarget::External { .. }
            | ReferenceTarget::Unresolved => {}
        }
    }

    /// Translate every id, type and span in `expr` and in each
    /// expression under it.
    pub(super) fn expr(&self, expr: &mut IrExpr) {
        self.ty(expr.ty_mut());
        self.span(expr.span_mut());
        match expr {
            IrExpr::StructInst {
                struct_id,
                type_args,
                ..
            } => {
                if let Some(id) = struct_id {
                    self.structure(id);
                }
                for arg in type_args {
                    self.ty(arg);
                }
            }
            IrExpr::EnumInst {
                enum_id: Some(id), ..
            } => self.enumeration(id),
            IrExpr::Reference { target, .. } => self.target(target),
            IrExpr::For { var_ty, .. } => self.ty(var_ty),
            IrExpr::Match { arms, .. } => {
                for arm in arms {
                    for (_, _, ty) in &mut arm.bindings {
                        self.ty(ty);
                    }
                }
            }
            IrExpr::FunctionCall {
                function_id: Some(id),
                ..
            } => self.function_id(id),
            IrExpr::MethodCall { dispatch, .. } => match dispatch {
                DispatchKind::Static { impl_id } => self.implementation(impl_id),
                DispatchKind::Virtual {
                    trait_id,
                    trait_args,
                    ..
                } => {
                    self.trait_id(trait_id);
                    for ty in trait_args {
                        self.ty(ty);
                    }
                }
            },
            IrExpr::Closure {
                params, captures, ..
            } => {
                for (_, _, _, ty) in params {
                    self.ty(ty);
                }
                for (_, _, _, ty) in captures {
                    self.ty(ty);
                }
            }
            IrExpr::Block { statements, .. } => {
                for stmt in statements {
                    self.statement(stmt);
                }
            }
            IrExpr::EnumInst { .. }
            | IrExpr::FunctionCall { .. }
            | IrExpr::Literal { .. }
            | IrExpr::Array { .. }
            | IrExpr::Tuple { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::FieldAccess { .. }
            | IrExpr::LetRef { .. }
            | IrExpr::BinaryOp { .. }
            | IrExpr::UnaryOp { .. }
            | IrExpr::If { .. }
            | IrExpr::CallClosure { .. }
            | IrExpr::ClosureRef { .. }
            | IrExpr::DictLiteral { .. }
            | IrExpr::DictAccess { .. } => {}
        }
        walk_expr_children_mut(expr, &mut |child| self.expr(child));
    }

    /// The slots of a block statement that are not expressions. The
    /// expressions in it are children of the block, and the walk in
    /// [`Self::expr`] visits them.
    fn statement(&self, stmt: &mut IrBlockStatement) {
        match stmt {
            IrBlockStatement::Let { ty, span, .. } => {
                if let Some(ty) = ty {
                    self.ty(ty);
                }
                self.span(span);
            }
            IrBlockStatement::Assign { span, .. } => self.span(span),
            IrBlockStatement::Expr(_) => {}
        }
    }
}
