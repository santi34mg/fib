use crate::frontend::analysis_annotation::NameAnnotation;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedIdentifier {
    pub segments: Vec<String>,
    pub annotation: NameAnnotation,
}

impl QualifiedIdentifier {
    pub fn at(mut self, span: crate::diagnostics::Span) -> Self {
        self.annotation.source_span = Some(span.into());
        self
    }

    pub fn new(segments: Vec<String>) -> Self {
        Self {
            segments,
            annotation: NameAnnotation::default(),
        }
    }
}
