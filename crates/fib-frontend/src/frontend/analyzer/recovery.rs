//! Collect independent diagnostics without treating failed annotations as success.
use crate::{
    diagnostics::{CompilerError, ErrorKind, Span},
    frontend::analysis_annotation::BindingId,
    plan::FileId,
};
use std::{cell::RefCell, collections::HashSet, path::PathBuf, rc::Rc};

#[derive(Clone, Default)]
pub struct Recovery(Rc<RefCell<State>>);
#[derive(Default)]
struct State {
    diagnostics: Vec<CompilerError>,
    failed: HashSet<(FileId, Span)>,
    bindings: HashSet<BindingId>,
    paths: std::collections::HashMap<FileId, PathBuf>,
    unavailable: HashSet<FileId>,
}

#[derive(Debug)]
pub(super) struct Blocked;
impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("analysis blocked by an earlier error")
    }
}
impl std::error::Error for Blocked {}

impl Recovery {
    pub(super) fn with_paths(paths: std::collections::HashMap<FileId, PathBuf>) -> Self {
        Self(Rc::new(RefCell::new(State {
            paths,
            ..State::default()
        })))
    }
    pub(super) fn record(
        &self,
        error: anyhow::Error,
        file: FileId,
        span: Span,
        path: Option<&PathBuf>,
    ) {
        self.0.borrow_mut().failed.insert((file, span));
        self.record_global(error, file, span, path);
    }
    pub(super) fn record_global(
        &self,
        error: anyhow::Error,
        file: FileId,
        span: Span,
        path: Option<&PathBuf>,
    ) {
        if error.downcast_ref::<Blocked>().is_some() {
            return;
        }
        let mut diagnostic = match error.downcast::<CompilerError>() {
            Ok(diagnostic) => diagnostic,
            Err(error) => CompilerError::new(ErrorKind::Type, format!("{error:#}")),
        };
        if diagnostic.span.is_none() {
            diagnostic.span = Some(span);
        }
        if diagnostic.filename.is_none() {
            diagnostic.filename = path
                .cloned()
                .or_else(|| self.0.borrow().paths.get(&file).cloned());
        }
        let mut state = self.0.borrow_mut();
        if !state.diagnostics.iter().any(|d| {
            d.kind == diagnostic.kind
                && d.span == diagnostic.span
                && d.filename == diagnostic.filename
                && d.message == diagnostic.message
        }) {
            state.diagnostics.push(diagnostic);
        }
    }
    pub(super) fn failed(&self, file: FileId, span: Span) -> bool {
        self.0.borrow().failed.contains(&(file, span))
    }
    pub(super) fn block_binding(&self, binding: BindingId) {
        self.0.borrow_mut().bindings.insert(binding);
    }
    pub(super) fn blocked(&self, binding: BindingId) -> bool {
        self.0.borrow().bindings.contains(&binding)
    }
    pub(super) fn set_unavailable(&self, files: HashSet<FileId>) {
        self.0.borrow_mut().unavailable = files;
    }
    pub(super) fn unavailable(&self, file: FileId) -> bool {
        self.0.borrow().unavailable.contains(&file)
    }
    pub(super) fn statement_failed(
        &self,
        file: FileId,
        node: &crate::frontend::ast::statement::Statement,
    ) -> bool {
        use crate::frontend::ast::statement::StatementKind as S;
        self.failed(file, node.span)
            || match &node.kind {
                S::VariableDeclaration(v) => {
                    v.annotation.binding.is_some_and(|id| self.blocked(id))
                }
                S::MultiVariableDeclaration { bindings, .. } => bindings
                    .iter()
                    .any(|v| v.annotation.binding.is_some_and(|id| self.blocked(id))),
                S::FunctionDeclaration(f) => f
                    .signature
                    .annotation
                    .binding
                    .is_some_and(|id| self.blocked(id)),
                _ => false,
            }
    }
    pub(super) fn region_failed(
        &self,
        file: FileId,
        nodes: &[crate::frontend::ast::statement::Statement],
    ) -> bool {
        struct Failed<'a>(&'a Recovery, FileId, bool);
        impl super::walk::Visitor for Failed<'_> {
            fn statement(
                &mut self,
                node: &mut crate::frontend::ast::statement::Statement,
            ) -> anyhow::Result<()> {
                self.2 |= self.0.statement_failed(self.1, node);
                Ok(())
            }
        }
        let mut visitor = Failed(self, file, false);
        let _ = super::walk::statements(&mut visitor, &mut nodes.to_vec(), false);
        visitor.2
    }
    pub(super) fn diagnostics(&self) -> Vec<CompilerError> {
        self.0.borrow().diagnostics.clone()
    }
    pub(super) fn has_errors(&self) -> bool {
        let state = self.0.borrow();
        !state.diagnostics.is_empty() || !state.failed.is_empty() || !state.unavailable.is_empty()
    }
    pub(super) fn block_statement(&self, node: &crate::frontend::ast::statement::Statement) {
        use crate::frontend::ast::statement::StatementKind as S;
        match &node.kind {
            S::VariableDeclaration(v) => {
                if let Some(id) = v.annotation.binding {
                    self.block_binding(id);
                }
            }
            S::MultiVariableDeclaration { bindings, .. } => {
                for v in bindings {
                    if let Some(id) = v.annotation.binding {
                        self.block_binding(id);
                    }
                }
            }
            S::FunctionDeclaration(f) => {
                if let Some(id) = f.signature.annotation.binding {
                    self.block_binding(id);
                }
            }
            _ => {}
        }
    }
}
