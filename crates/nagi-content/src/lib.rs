//! Immutable source-neutral structured text for Nagi renderers
//!
//! Content preserves text, mechanical structure, semantic roles,
//! presentation classes, stable identity, and application-resolved
//! annotations without depending on a parser, terminal backend, or TUI

mod identifier;
mod projection;
mod tree;
mod validation;

pub use identifier::{AnnotationId, Class, ElementId, IdentifierError, IdentifierErrorKind, Role};
pub use projection::{AnnotationRange, SemanticText, semantic_text};
pub use tree::{
    Content, ContentKind, DuplicateClass, DuplicateRole, Element, ElementKind, SemanticBoundary,
};
pub use validation::{Limits, Stats, ValidationError, ValidationErrorKind, validate};
