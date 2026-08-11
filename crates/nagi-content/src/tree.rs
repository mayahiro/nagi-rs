use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use nagi_text::normalize_utf8;

use crate::{AnnotationId, Class, ElementId, Role};

/// The closed kind of one content node
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContentKind {
    /// Normalized UTF-8 text
    Text,
    /// One explicit semantic U+000A
    HardBreak,
    /// An ordered element with metadata and children
    Element,
}

/// Mechanical structure of a content element
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ElementKind {
    /// Inline children forming one semantic run
    Inline,
    /// Ordered block children
    Flow,
    /// Inline children forming one paragraph
    Paragraph,
    /// Ordered fields or cells
    Sequence,
}

impl ElementKind {
    /// Returns the default semantic boundary for this element kind
    #[must_use]
    pub const fn default_boundary(self) -> SemanticBoundary {
        match self {
            Self::Inline | Self::Paragraph => SemanticBoundary::None,
            Self::Flow => SemanticBoundary::Line,
            Self::Sequence => SemanticBoundary::Tab,
        }
    }
}

/// Text inserted between adjacent direct children during semantic projection
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticBoundary {
    /// Inserts no text
    None,
    /// Inserts one U+0020
    Space,
    /// Inserts one U+000A
    Line,
    /// Inserts one U+0009
    Tab,
}

impl SemanticBoundary {
    /// Returns the UTF-8 text represented by this boundary
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Space => " ",
            Self::Line => "\n",
            Self::Tab => "\t",
        }
    }
}

/// Immutable source-neutral content node
#[derive(Clone, Debug)]
pub struct Content {
    pub(crate) inner: Arc<ContentNode>,
}

#[derive(Debug)]
pub(crate) enum ContentNode {
    Text(String),
    HardBreak,
    Element(Element),
}

impl Content {
    /// Creates a text node from valid UTF-8
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(ContentNode::Text(value.into())),
        }
    }

    /// Creates a text node and replaces each invalid UTF-8 run with U+FFFD
    #[must_use]
    pub fn text_bytes(value: &[u8]) -> Self {
        Self::text(normalize_utf8(value).into_owned())
    }

    /// Creates one explicit semantic hard break
    #[must_use]
    pub fn hard_break() -> Self {
        Self {
            inner: Arc::new(ContentNode::HardBreak),
        }
    }

    /// Creates a content node from a configured element
    #[must_use]
    pub fn element(element: Element) -> Self {
        Self {
            inner: Arc::new(ContentNode::Element(element)),
        }
    }

    /// Returns this node's closed kind
    #[must_use]
    pub fn kind(&self) -> ContentKind {
        match self.inner.as_ref() {
            ContentNode::Text(_) => ContentKind::Text,
            ContentNode::HardBreak => ContentKind::HardBreak,
            ContentNode::Element(_) => ContentKind::Element,
        }
    }

    /// Returns this node's text, or `None` for another node kind
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self.inner.as_ref() {
            ContentNode::Text(value) => Some(value),
            ContentNode::HardBreak | ContentNode::Element(_) => None,
        }
    }

    /// Returns this node's element, or `None` for another node kind
    #[must_use]
    pub fn as_element(&self) -> Option<&Element> {
        match self.inner.as_ref() {
            ContentNode::Element(element) => Some(element),
            ContentNode::Text(_) | ContentNode::HardBreak => None,
        }
    }

    pub(crate) fn node(&self) -> &ContentNode {
        self.inner.as_ref()
    }

    #[cfg(test)]
    fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::text("")
    }
}

impl From<Element> for Content {
    fn from(element: Element) -> Self {
        Self::element(element)
    }
}

impl From<String> for Content {
    fn from(value: String) -> Self {
        Self::text(value)
    }
}

impl From<&str> for Content {
    fn from(value: &str) -> Self {
        Self::text(value)
    }
}

/// Immutable ordered content element and its source-neutral metadata
#[derive(Clone, Debug)]
pub struct Element {
    kind: ElementKind,
    id: Option<ElementId>,
    revision: u64,
    roles: Arc<[Role]>,
    classes: Arc<[Class]>,
    annotation: Option<AnnotationId>,
    boundary: SemanticBoundary,
    children: Arc<[Content]>,
}

impl Element {
    /// Creates an element using its kind's default semantic boundary
    #[must_use]
    pub fn new(kind: ElementKind, children: impl IntoIterator<Item = Content>) -> Self {
        Self {
            kind,
            id: None,
            revision: 0,
            roles: Arc::from([]),
            classes: Arc::from([]),
            annotation: None,
            boundary: kind.default_boundary(),
            children: Arc::from(children.into_iter().collect::<Vec<_>>()),
        }
    }

    /// Returns the element's mechanical kind
    #[must_use]
    pub const fn kind(&self) -> ElementKind {
        self.kind
    }

    /// Returns the optional stable identity
    #[must_use]
    pub const fn id(&self) -> Option<&ElementId> {
        self.id.as_ref()
    }

    /// Returns the caller-owned opaque revision
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the ordered unique semantic roles
    #[must_use]
    pub fn roles(&self) -> &[Role] {
        &self.roles
    }

    /// Returns the ordered unique presentation classes
    #[must_use]
    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    /// Returns the optional application-resolved annotation identity
    #[must_use]
    pub const fn annotation(&self) -> Option<&AnnotationId> {
        self.annotation.as_ref()
    }

    /// Returns the semantic boundary inserted between adjacent children
    #[must_use]
    pub const fn boundary(&self) -> SemanticBoundary {
        self.boundary
    }

    /// Returns the immutable ordered children
    #[must_use]
    pub fn children(&self) -> &[Content] {
        &self.children
    }

    /// Returns this element with a stable identity
    #[must_use]
    pub fn with_id(mut self, id: ElementId) -> Self {
        self.id = Some(id);
        self
    }

    /// Returns this element with a caller-owned opaque revision
    #[must_use]
    pub const fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    /// Returns this element with a validated ordered role set
    pub fn with_roles(
        mut self,
        roles: impl IntoIterator<Item = Role>,
    ) -> Result<Self, DuplicateRole> {
        let roles: Vec<Role> = roles.into_iter().collect();
        let mut seen = HashSet::with_capacity(roles.len());
        for role in &roles {
            if !seen.insert(role.as_str()) {
                return Err(DuplicateRole { role: role.clone() });
            }
        }
        self.roles = Arc::from(roles);
        Ok(self)
    }

    /// Returns this element with a validated ordered class set
    pub fn with_classes(
        mut self,
        classes: impl IntoIterator<Item = Class>,
    ) -> Result<Self, DuplicateClass> {
        let classes: Vec<Class> = classes.into_iter().collect();
        let mut seen = HashSet::with_capacity(classes.len());
        for class in &classes {
            if !seen.insert(class.as_str()) {
                return Err(DuplicateClass {
                    class: class.clone(),
                });
            }
        }
        self.classes = Arc::from(classes);
        Ok(self)
    }

    /// Returns this element with an application-resolved annotation
    #[must_use]
    pub fn with_annotation(mut self, annotation: AnnotationId) -> Self {
        self.annotation = Some(annotation);
        self
    }

    /// Returns this element with an explicit semantic child boundary
    #[must_use]
    pub const fn with_boundary(mut self, boundary: SemanticBoundary) -> Self {
        self.boundary = boundary;
        self
    }

    /// Converts this configured element into a content node
    #[must_use]
    pub fn into_content(self) -> Content {
        Content::element(self)
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::new(ElementKind::Inline, [])
    }
}

/// Error returned when one role occurs more than once on an element
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateRole {
    role: Role,
}

impl DuplicateRole {
    /// Returns the repeated role
    #[must_use]
    pub const fn role(&self) -> &Role {
        &self.role
    }
}

impl fmt::Display for DuplicateRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate content role {}", self.role)
    }
}

impl Error for DuplicateRole {}

/// Error returned when one class occurs more than once on an element
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateClass {
    class: Class,
}

impl DuplicateClass {
    /// Returns the repeated class
    #[must_use]
    pub const fn class(&self) -> &Class {
        &self.class
    }
}

impl fmt::Display for DuplicateClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate content class {}", self.class)
    }
}

impl Error for DuplicateClass {}

#[cfg(test)]
mod tests {
    use super::{Content, Element, ElementKind};
    use crate::{Class, Role};

    #[test]
    fn cloned_content_shares_immutable_storage() {
        let content = Element::new(ElementKind::Inline, [Content::text("shared")]).into_content();
        assert!(content.shares_storage(&content.clone()));
    }

    #[test]
    fn duplicate_metadata_is_rejected() {
        let role = Role::new("heading").unwrap();
        let class = Class::new("source.bold").unwrap();
        assert!(
            Element::new(ElementKind::Inline, [])
                .with_roles([role.clone(), role])
                .is_err()
        );
        assert!(
            Element::new(ElementKind::Inline, [])
                .with_classes([class.clone(), class])
                .is_err()
        );
    }
}
