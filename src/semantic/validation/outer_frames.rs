//! The bindings that a loop body or a closure body reaches from
//! outside it.
//!
//! A `for` body runs once per element, and a closure body runs once per
//! call. So a `sink` of an outer binding inside one gives the value away
//! again on the second pass: that is a use after the sink. A closure is
//! also pure (`docs/user/closures.md`): it does not assign to a binding
//! that it captures, and it does not pass one to a `mut` parameter.
//!
//! Each loop body and closure body pushes a frame with the names that
//! are in scope where it starts. A name in a frame is an outer binding
//! for everything inside that frame.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use std::collections::HashSet;

/// The outer bindings of one loop body or closure body.
#[derive(Debug, Default)]
pub(in crate::semantic) struct OuterFrame {
    /// True for a closure body, false for a loop body.
    closure: bool,
    names: HashSet<String>,
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Start a loop body (`closure` false) or a closure body.
    pub(in crate::semantic::validation) fn push_outer_frame(&mut self, closure: bool) {
        let mut names: HashSet<String> = self.local_let_bindings.keys().cloned().collect();
        names.extend(self.closure_param_scopes.iter().flatten().cloned());
        names.extend(self.loop_var_scopes.iter().flat_map(|s| s.keys().cloned()));
        names.extend(
            self.inference_scope_stack
                .borrow()
                .iter()
                .flat_map(|s| s.keys().cloned()),
        );
        self.outer_frames.push(OuterFrame { closure, names });
    }

    /// End the innermost loop body or closure body.
    pub(in crate::semantic::validation) fn pop_outer_frame(&mut self) {
        self.outer_frames.pop();
    }

    /// True when a `sink` of `name` here would run more than once: the
    /// name is from outside a loop body or a closure body.
    pub(in crate::semantic::validation) fn sink_repeats(&self, name: &str) -> bool {
        self.outer_frames.iter().any(|f| f.names.contains(name))
    }

    /// True when `name` is a binding that a closure around this point
    /// captures.
    pub(in crate::semantic::validation) fn is_closure_capture(&self, name: &str) -> bool {
        self.outer_frames
            .iter()
            .any(|f| f.closure && f.names.contains(name))
    }
}
