use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use crate::tree::ContentNode;
use crate::{Content, ElementId, SemanticBoundary};

/// Inclusive resource limits used by explicit content validation
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Limits {
    /// Maximum root-relative node depth, where the root has depth one
    pub max_depth: u64,
    /// Maximum number of node occurrences
    pub max_nodes: u64,
    /// Maximum projected semantic UTF-8 byte count
    pub max_semantic_bytes: u64,
    /// Maximum identifier and token UTF-8 byte count
    pub max_metadata_bytes: u64,
    /// Maximum combined role and class count on one element
    pub max_tokens_per_element: u64,
}

impl Limits {
    /// Limits that accept every representable resource count
    pub const UNLIMITED: Self = Self {
        max_depth: u64::MAX,
        max_nodes: u64::MAX,
        max_semantic_bytes: u64::MAX,
        max_metadata_bytes: u64::MAX,
        max_tokens_per_element: u64::MAX,
    };
}

/// Complete resource statistics for one validated content tree
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    /// Number of node occurrences
    pub nodes: u64,
    /// Greatest root-relative node depth
    pub max_depth: u64,
    /// Projected semantic UTF-8 byte count
    pub semantic_bytes: u64,
    /// Identifier and token UTF-8 byte count
    pub metadata_bytes: u64,
    /// Number of annotated element occurrences
    pub annotations: u64,
    /// Number of identified element occurrences
    pub identified_elements: u64,
    /// Greatest combined role and class count on one element
    pub max_tokens_per_element: u64,
}

/// Stable reason that content validation stopped
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValidationErrorKind {
    /// The node occurrence limit was exceeded
    NodeLimit,
    /// The root-relative depth limit was exceeded
    DepthLimit,
    /// The semantic UTF-8 byte limit was exceeded
    SemanticByteLimit,
    /// The metadata UTF-8 byte limit was exceeded
    MetadataByteLimit,
    /// One element's role-plus-class limit was exceeded
    TokenLimit,
    /// A stable element ID occurred more than once
    DuplicateElementId,
}

impl ValidationErrorKind {
    /// Returns the stable specification name for this failure kind
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NodeLimit => "node-limit",
            Self::DepthLimit => "depth-limit",
            Self::SemanticByteLimit => "semantic-byte-limit",
            Self::MetadataByteLimit => "metadata-byte-limit",
            Self::TokenLimit => "token-limit",
            Self::DuplicateElementId => "duplicate-element-id",
        }
    }
}

/// Structured content validation failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    kind: ValidationErrorKind,
    observed: Option<u64>,
    limit: Option<u64>,
    element_id: Option<ElementId>,
}

impl ValidationError {
    fn limit(kind: ValidationErrorKind, observed: u64, limit: u64) -> Self {
        Self {
            kind,
            observed: Some(observed),
            limit: Some(limit),
            element_id: None,
        }
    }

    fn duplicate(element_id: ElementId) -> Self {
        Self {
            kind: ValidationErrorKind::DuplicateElementId,
            observed: None,
            limit: None,
            element_id: Some(element_id),
        }
    }

    /// Returns the stable validation failure kind
    #[must_use]
    pub const fn kind(&self) -> ValidationErrorKind {
        self.kind
    }

    /// Returns the first count that exceeded a limit
    #[must_use]
    pub const fn observed(&self) -> Option<u64> {
        self.observed
    }

    /// Returns the configured limit for a resource failure
    #[must_use]
    pub const fn limit_value(&self) -> Option<u64> {
        self.limit
    }

    /// Returns the repeated element ID for a duplicate failure
    #[must_use]
    pub const fn element_id(&self) -> Option<&ElementId> {
        self.element_id.as_ref()
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(observed), Some(limit)) = (self.observed, self.limit) {
            return write!(
                formatter,
                "content {}: observed {observed}, limit {limit}",
                self.kind.as_str()
            );
        }
        if let Some(element_id) = &self.element_id {
            return write!(formatter, "duplicate content element ID {element_id}");
        }
        formatter.write_str(self.kind.as_str())
    }
}

impl Error for ValidationError {}

/// Validates one immutable content tree against explicit resource limits
pub fn validate(content: &Content, limits: Limits) -> Result<Stats, ValidationError> {
    enum Event<'a> {
        Node(&'a Content, u64),
        Boundary(SemanticBoundary),
    }

    let mut stats = Stats::default();
    let mut ids = HashSet::new();
    let mut stack = vec![Event::Node(content, 1)];
    while let Some(event) = stack.pop() {
        match event {
            Event::Node(node, depth) => {
                stats.nodes = stats.nodes.saturating_add(1);
                check_limit(
                    ValidationErrorKind::NodeLimit,
                    stats.nodes,
                    limits.max_nodes,
                )?;

                stats.max_depth = stats.max_depth.max(depth);
                check_limit(ValidationErrorKind::DepthLimit, depth, limits.max_depth)?;

                match node.node() {
                    ContentNode::Text(value) => add_semantic_bytes(
                        &mut stats,
                        byte_count(value.len()),
                        limits.max_semantic_bytes,
                    )?,
                    ContentNode::HardBreak => {
                        add_semantic_bytes(&mut stats, 1, limits.max_semantic_bytes)?;
                    }
                    ContentNode::Element(element) => {
                        let tokens = byte_count(element.roles().len())
                            .saturating_add(byte_count(element.classes().len()));
                        stats.max_tokens_per_element = stats.max_tokens_per_element.max(tokens);
                        check_limit(
                            ValidationErrorKind::TokenLimit,
                            tokens,
                            limits.max_tokens_per_element,
                        )?;

                        let metadata_bytes = element
                            .id()
                            .map_or(0, |id| byte_count(id.as_str().len()))
                            .saturating_add(
                                element
                                    .roles()
                                    .iter()
                                    .map(|role| byte_count(role.as_str().len()))
                                    .fold(0, u64::saturating_add),
                            )
                            .saturating_add(
                                element
                                    .classes()
                                    .iter()
                                    .map(|class| byte_count(class.as_str().len()))
                                    .fold(0, u64::saturating_add),
                            )
                            .saturating_add(
                                element
                                    .annotation()
                                    .map_or(0, |annotation| byte_count(annotation.as_str().len())),
                            );
                        stats.metadata_bytes = stats.metadata_bytes.saturating_add(metadata_bytes);
                        check_limit(
                            ValidationErrorKind::MetadataByteLimit,
                            stats.metadata_bytes,
                            limits.max_metadata_bytes,
                        )?;

                        if let Some(id) = element.id() {
                            stats.identified_elements = stats.identified_elements.saturating_add(1);
                            if !ids.insert(id.as_str()) {
                                return Err(ValidationError::duplicate(id.clone()));
                            }
                        }
                        if element.annotation().is_some() {
                            stats.annotations = stats.annotations.saturating_add(1);
                        }

                        for index in (0..element.children().len()).rev() {
                            stack.push(Event::Node(
                                &element.children()[index],
                                depth.saturating_add(1),
                            ));
                            if index > 0 {
                                stack.push(Event::Boundary(element.boundary()));
                            }
                        }
                    }
                }
            }
            Event::Boundary(boundary) => add_semantic_bytes(
                &mut stats,
                byte_count(boundary.as_str().len()),
                limits.max_semantic_bytes,
            )?,
        }
    }
    Ok(stats)
}

fn add_semantic_bytes(stats: &mut Stats, bytes: u64, limit: u64) -> Result<(), ValidationError> {
    stats.semantic_bytes = stats.semantic_bytes.saturating_add(bytes);
    check_limit(
        ValidationErrorKind::SemanticByteLimit,
        stats.semantic_bytes,
        limit,
    )
}

fn check_limit(
    kind: ValidationErrorKind,
    observed: u64,
    limit: u64,
) -> Result<(), ValidationError> {
    if observed > limit {
        return Err(ValidationError::limit(kind, observed, limit));
    }
    Ok(())
}

fn byte_count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
