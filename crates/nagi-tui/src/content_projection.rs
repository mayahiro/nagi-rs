use std::error::Error;
use std::fmt;
use std::marker::PhantomData;

use nagi_content::{Content, ContentKind, Element};
use nagi_vt::Style;

use crate::{
    ComputedPresentation, Node, ParagraphOptions, PresentationDisplay, PresentationSheet,
    PresentationState, TextSpan,
};

/// Default maximum Content node occurrences visited by one projection
pub const DEFAULT_CONTENT_PROJECTION_MAX_CONTENT_NODES: u64 = 100_000;
/// Default maximum TUI Nodes created by one projection
pub const DEFAULT_CONTENT_PROJECTION_MAX_OUTPUT_NODES: u64 = 100_000;
/// Default maximum styled text spans created by one projection
pub const DEFAULT_CONTENT_PROJECTION_MAX_SPANS: u64 = 100_000;
/// Default maximum Content and output tree depth accepted by one projection
pub const DEFAULT_CONTENT_PROJECTION_MAX_DEPTH: u32 = 128;
/// Hard maximum projection depth supported by the recursive TUI Node backend
pub const MAX_CONTENT_PROJECTION_DEPTH: u32 = 256;
/// Default maximum emitted UTF-8 bytes accepted by one projection
pub const DEFAULT_CONTENT_PROJECTION_MAX_VISUAL_BYTES: u64 = 16 * 1024 * 1024;

/// Bounded eager-work limits for one Content-to-Node projection
///
/// A zero value passed to a `with_max_*` method restores that property's
/// default. Depth values above [`MAX_CONTENT_PROJECTION_DEPTH`] are capped at
/// that backend safety limit
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentProjectionLimits {
    max_content_nodes: u64,
    max_output_nodes: u64,
    max_spans: u64,
    max_depth: u32,
    max_visual_bytes: u64,
}

impl ContentProjectionLimits {
    /// Returns the maximum visited Content node occurrence count
    #[must_use]
    pub const fn max_content_nodes(&self) -> u64 {
        self.max_content_nodes
    }

    /// Returns these limits with a maximum Content node occurrence count
    #[must_use]
    pub const fn with_max_content_nodes(mut self, value: u64) -> Self {
        self.max_content_nodes =
            default_if_zero(value, DEFAULT_CONTENT_PROJECTION_MAX_CONTENT_NODES);
        self
    }

    /// Returns the maximum generated TUI Node count
    #[must_use]
    pub const fn max_output_nodes(&self) -> u64 {
        self.max_output_nodes
    }

    /// Returns these limits with a maximum generated TUI Node count
    #[must_use]
    pub const fn with_max_output_nodes(mut self, value: u64) -> Self {
        self.max_output_nodes = default_if_zero(value, DEFAULT_CONTENT_PROJECTION_MAX_OUTPUT_NODES);
        self
    }

    /// Returns the maximum generated styled text span count
    #[must_use]
    pub const fn max_spans(&self) -> u64 {
        self.max_spans
    }

    /// Returns these limits with a maximum generated styled text span count
    #[must_use]
    pub const fn with_max_spans(mut self, value: u64) -> Self {
        self.max_spans = default_if_zero(value, DEFAULT_CONTENT_PROJECTION_MAX_SPANS);
        self
    }

    /// Returns the maximum accepted Content and output tree depth
    #[must_use]
    pub const fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Returns these limits with a maximum Content and output tree depth
    ///
    /// Zero restores the default and values above the supported backend limit
    /// are capped
    #[must_use]
    pub const fn with_max_depth(mut self, value: u32) -> Self {
        let value = if value == 0 {
            DEFAULT_CONTENT_PROJECTION_MAX_DEPTH
        } else {
            value
        };
        self.max_depth = if value > MAX_CONTENT_PROJECTION_DEPTH {
            MAX_CONTENT_PROJECTION_DEPTH
        } else {
            value
        };
        self
    }

    /// Returns the maximum emitted UTF-8 byte count
    #[must_use]
    pub const fn max_visual_bytes(&self) -> u64 {
        self.max_visual_bytes
    }

    /// Returns these limits with a maximum emitted UTF-8 byte count
    #[must_use]
    pub const fn with_max_visual_bytes(mut self, value: u64) -> Self {
        self.max_visual_bytes = default_if_zero(value, DEFAULT_CONTENT_PROJECTION_MAX_VISUAL_BYTES);
        self
    }
}

impl Default for ContentProjectionLimits {
    fn default() -> Self {
        Self {
            max_content_nodes: DEFAULT_CONTENT_PROJECTION_MAX_CONTENT_NODES,
            max_output_nodes: DEFAULT_CONTENT_PROJECTION_MAX_OUTPUT_NODES,
            max_spans: DEFAULT_CONTENT_PROJECTION_MAX_SPANS,
            max_depth: DEFAULT_CONTENT_PROJECTION_MAX_DEPTH,
            max_visual_bytes: DEFAULT_CONTENT_PROJECTION_MAX_VISUAL_BYTES,
        }
    }
}

const fn default_if_zero(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

/// Root style and eager-work limits for one Content-to-Node projection
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ContentProjectionOptions {
    base_style: Style,
    limits: ContentProjectionLimits,
}

impl ContentProjectionOptions {
    /// Returns the style inherited by the root Content node
    #[must_use]
    pub const fn base_style(&self) -> Style {
        self.base_style
    }

    /// Returns these options with a replacement root inherited style
    #[must_use]
    pub const fn with_base_style(mut self, value: Style) -> Self {
        self.base_style = value;
        self
    }

    /// Returns the eager-work limits
    #[must_use]
    pub const fn limits(&self) -> ContentProjectionLimits {
        self.limits
    }

    /// Returns these options with replacement eager-work limits
    #[must_use]
    pub const fn with_limits(mut self, value: ContentProjectionLimits) -> Self {
        self.limits = value;
        self
    }
}

/// Stable category of a Content-to-Node projection failure
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContentProjectionErrorKind {
    /// A block display occurred inside an inline formatting context
    InvalidLayoutTree,
    /// The visited Content node occurrence limit was exceeded
    ContentNodeLimit,
    /// The generated TUI Node limit was exceeded
    OutputNodeLimit,
    /// The generated styled text span limit was exceeded
    SpanLimit,
    /// The Content or output tree depth limit was exceeded
    DepthLimit,
    /// The emitted UTF-8 byte limit was exceeded
    VisualByteLimit,
}

impl ContentProjectionErrorKind {
    /// Returns the stable language-independent error identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidLayoutTree => "invalid-layout-tree",
            Self::ContentNodeLimit => "content-node-limit",
            Self::OutputNodeLimit => "output-node-limit",
            Self::SpanLimit => "span-limit",
            Self::DepthLimit => "depth-limit",
            Self::VisualByteLimit => "visual-byte-limit",
        }
    }
}

impl fmt::Display for ContentProjectionErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured Content-to-Node projection failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentProjectionError {
    kind: ContentProjectionErrorKind,
    depth: u32,
    limit: Option<u64>,
    observed: Option<u64>,
    rejected_display: Option<PresentationDisplay>,
}

impl ContentProjectionError {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> ContentProjectionErrorKind {
        self.kind
    }

    /// Returns the one-based Content or output depth where the error arose
    #[must_use]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    /// Returns the exceeded limit for a resource error
    #[must_use]
    pub const fn limit(&self) -> Option<u64> {
        self.limit
    }

    /// Returns the first observed value above a resource limit
    #[must_use]
    pub const fn observed(&self) -> Option<u64> {
        self.observed
    }

    /// Returns the block display rejected inside inline content
    #[must_use]
    pub const fn rejected_display(&self) -> Option<PresentationDisplay> {
        self.rejected_display
    }

    fn resource(kind: ContentProjectionErrorKind, depth: u32, limit: u64, observed: u64) -> Self {
        Self {
            kind,
            depth,
            limit: Some(limit),
            observed: Some(observed),
            rejected_display: None,
        }
    }

    fn invalid_layout(depth: u32, display: PresentationDisplay) -> Self {
        Self {
            kind: ContentProjectionErrorKind::InvalidLayoutTree,
            depth,
            limit: None,
            observed: None,
            rejected_display: Some(display),
        }
    }
}

impl fmt::Display for ContentProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(limit), Some(observed)) = (self.limit, self.observed) {
            return write!(
                formatter,
                "content projection {} at depth {}: limit {}, observed {}",
                self.kind, self.depth, limit, observed
            );
        }
        write!(
            formatter,
            "content projection {} at depth {}",
            self.kind, self.depth
        )
    }
}

impl Error for ContentProjectionError {}

/// Projects Content through a Presentation Sheet without application states
///
/// The returned Node has no implicit Node ID or annotation handler. Inline
/// roots and inline children directly under Flow or Sequence are promoted to
/// anonymous Paragraph Nodes
pub fn project_content<Message>(
    content: &Content,
    sheet: &PresentationSheet,
    options: ContentProjectionOptions,
) -> Result<Node<Message>, ContentProjectionError> {
    project_content_with_states(content, sheet, options, |_| &[])
}

/// Projects Content through a Presentation Sheet with per-element states
///
/// `states_for` is called synchronously once for every visited Element. Its
/// returned slice is used only for that element's rule resolution and is not
/// retained
pub fn project_content_with_states<'states, Message, States>(
    content: &Content,
    sheet: &PresentationSheet,
    options: ContentProjectionOptions,
    states_for: States,
) -> Result<Node<Message>, ContentProjectionError>
where
    States: FnMut(&Element) -> &'states [PresentationState],
{
    Projector::new(sheet, options, states_for).project(content)
}

struct Projector<'sheet, 'states, States> {
    sheet: &'sheet PresentationSheet,
    options: ContentProjectionOptions,
    states_for: States,
    content_nodes: u64,
    output_nodes: u64,
    spans: u64,
    visual_bytes: u64,
    states_lifetime: PhantomData<&'states [PresentationState]>,
}

impl<'sheet, 'states, States> Projector<'sheet, 'states, States>
where
    States: FnMut(&Element) -> &'states [PresentationState],
{
    fn new(
        sheet: &'sheet PresentationSheet,
        options: ContentProjectionOptions,
        states_for: States,
    ) -> Self {
        Self {
            sheet,
            options,
            states_for,
            content_nodes: 0,
            output_nodes: 0,
            spans: 0,
            visual_bytes: 0,
            states_lifetime: PhantomData,
        }
    }

    fn project<Message>(
        mut self,
        content: &Content,
    ) -> Result<Node<Message>, ContentProjectionError> {
        self.project_box(content, self.options.base_style, 1, 1)
    }

    fn project_box<Message>(
        &mut self,
        content: &Content,
        inherited_style: Style,
        content_depth: u32,
        output_depth: u32,
    ) -> Result<Node<Message>, ContentProjectionError> {
        self.enter_content(content_depth)?;
        match content.kind() {
            ContentKind::Text => {
                self.enter_output(output_depth)?;
                let mut spans = Vec::with_capacity(1);
                self.push_span(
                    &mut spans,
                    content.as_text().unwrap_or_default(),
                    inherited_style,
                    content_depth,
                )?;
                Ok(Node::paragraph(spans, ParagraphOptions::default()))
            }
            ContentKind::HardBreak => {
                self.enter_output(output_depth)?;
                let mut spans = Vec::with_capacity(1);
                self.push_span(&mut spans, "\n", inherited_style, content_depth)?;
                Ok(Node::paragraph(spans, ParagraphOptions::default()))
            }
            ContentKind::Element => {
                let element = content
                    .as_element()
                    .expect("ContentKind::Element has an element");
                let computed = self.resolve(element, inherited_style);
                match computed.display() {
                    PresentationDisplay::Inline => {
                        self.project_inline_box(element, computed, content_depth, output_depth)
                    }
                    PresentationDisplay::Paragraph => {
                        self.project_paragraph(element, computed, content_depth, output_depth)
                    }
                    PresentationDisplay::Flow | PresentationDisplay::Sequence => {
                        self.project_container(element, computed, content_depth, output_depth)
                    }
                }
            }
        }
    }

    fn project_inline_box<Message>(
        &mut self,
        element: &Element,
        computed: ComputedPresentation<'sheet>,
        content_depth: u32,
        output_depth: u32,
    ) -> Result<Node<Message>, ContentProjectionError> {
        self.enter_output(output_depth)?;
        let mut spans = Vec::new();
        self.append_inline_children(element, computed, content_depth, &mut spans)?;
        Ok(Node::paragraph(spans, ParagraphOptions::default()))
    }

    fn project_paragraph<Message>(
        &mut self,
        element: &Element,
        computed: ComputedPresentation<'sheet>,
        content_depth: u32,
        output_depth: u32,
    ) -> Result<Node<Message>, ContentProjectionError> {
        self.enter_output(output_depth)?;
        let mut spans = Vec::new();
        self.append_inline_children(element, computed, content_depth, &mut spans)?;
        Ok(Node::paragraph(
            spans,
            ParagraphOptions {
                wrap: computed.wrap(),
                alignment: computed.alignment(),
            },
        )
        .with_length(computed.length()))
    }

    fn project_container<Message>(
        &mut self,
        element: &Element,
        computed: ComputedPresentation<'sheet>,
        content_depth: u32,
        output_depth: u32,
    ) -> Result<Node<Message>, ContentProjectionError> {
        self.enter_output(output_depth)?;
        let child_depth = content_depth.saturating_add(1);
        let output_child_depth = output_depth.saturating_add(1);
        let mut children = Vec::with_capacity(element.children().len().min(64));
        for (index, child) in element.children().iter().enumerate() {
            if index != 0 {
                if let Some(separator) = computed
                    .visual_separator()
                    .filter(|value| !value.is_empty())
                {
                    self.enter_output(output_child_depth)?;
                    let mut spans = Vec::with_capacity(1);
                    self.push_span(&mut spans, separator, computed.style(), output_child_depth)?;
                    children.push(Node::paragraph(spans, ParagraphOptions::default()));
                }
                if computed.gap() != 0 {
                    self.enter_output(output_child_depth)?;
                    children.push(Node::gap(computed.gap()));
                }
            }
            children.push(self.project_box(
                child,
                computed.style(),
                child_depth,
                output_child_depth,
            )?);
        }
        let node = match computed.display() {
            PresentationDisplay::Flow => Node::column(children),
            PresentationDisplay::Sequence => Node::row(children),
            PresentationDisplay::Inline | PresentationDisplay::Paragraph => {
                unreachable!("container projection only receives block containers")
            }
        };
        Ok(node.with_length(computed.length()))
    }

    fn append_inline_children(
        &mut self,
        element: &Element,
        computed: ComputedPresentation<'sheet>,
        content_depth: u32,
        spans: &mut Vec<TextSpan>,
    ) -> Result<(), ContentProjectionError> {
        let child_depth = content_depth.saturating_add(1);
        for (index, child) in element.children().iter().enumerate() {
            if index != 0 {
                if let Some(separator) = computed
                    .visual_separator()
                    .filter(|value| !value.is_empty())
                {
                    self.push_span(spans, separator, computed.style(), child_depth)?;
                }
            }
            self.append_inline(child, computed.style(), child_depth, spans)?;
        }
        Ok(())
    }

    fn append_inline(
        &mut self,
        content: &Content,
        inherited_style: Style,
        content_depth: u32,
        spans: &mut Vec<TextSpan>,
    ) -> Result<(), ContentProjectionError> {
        self.enter_content(content_depth)?;
        match content.kind() {
            ContentKind::Text => self.push_span(
                spans,
                content.as_text().unwrap_or_default(),
                inherited_style,
                content_depth,
            ),
            ContentKind::HardBreak => self.push_span(spans, "\n", inherited_style, content_depth),
            ContentKind::Element => {
                let element = content
                    .as_element()
                    .expect("ContentKind::Element has an element");
                let computed = self.resolve(element, inherited_style);
                if computed.display() != PresentationDisplay::Inline {
                    return Err(ContentProjectionError::invalid_layout(
                        content_depth,
                        computed.display(),
                    ));
                }
                self.append_inline_children(element, computed, content_depth, spans)
            }
        }
    }

    fn resolve(
        &mut self,
        element: &Element,
        inherited_style: Style,
    ) -> ComputedPresentation<'sheet> {
        let states = (self.states_for)(element);
        self.sheet.resolve(element, inherited_style, states)
    }

    fn enter_content(&mut self, depth: u32) -> Result<(), ContentProjectionError> {
        self.check_depth(depth)?;
        let observed = self.content_nodes.saturating_add(1);
        let limit = self.options.limits.max_content_nodes;
        if observed > limit {
            return Err(ContentProjectionError::resource(
                ContentProjectionErrorKind::ContentNodeLimit,
                depth,
                limit,
                observed,
            ));
        }
        self.content_nodes = observed;
        Ok(())
    }

    fn enter_output(&mut self, depth: u32) -> Result<(), ContentProjectionError> {
        self.check_depth(depth)?;
        let observed = self.output_nodes.saturating_add(1);
        let limit = self.options.limits.max_output_nodes;
        if observed > limit {
            return Err(ContentProjectionError::resource(
                ContentProjectionErrorKind::OutputNodeLimit,
                depth,
                limit,
                observed,
            ));
        }
        self.output_nodes = observed;
        Ok(())
    }

    fn check_depth(&self, depth: u32) -> Result<(), ContentProjectionError> {
        let limit = self.options.limits.max_depth;
        if depth > limit {
            return Err(ContentProjectionError::resource(
                ContentProjectionErrorKind::DepthLimit,
                depth,
                u64::from(limit),
                u64::from(depth),
            ));
        }
        Ok(())
    }

    fn push_span(
        &mut self,
        spans: &mut Vec<TextSpan>,
        text: &str,
        style: Style,
        depth: u32,
    ) -> Result<(), ContentProjectionError> {
        let byte_count = u64::try_from(text.len()).unwrap_or(u64::MAX);
        let observed_bytes = self.visual_bytes.saturating_add(byte_count);
        let byte_limit = self.options.limits.max_visual_bytes;
        if observed_bytes > byte_limit {
            return Err(ContentProjectionError::resource(
                ContentProjectionErrorKind::VisualByteLimit,
                depth,
                byte_limit,
                observed_bytes,
            ));
        }
        let observed_spans = self.spans.saturating_add(1);
        let span_limit = self.options.limits.max_spans;
        if observed_spans > span_limit {
            return Err(ContentProjectionError::resource(
                ContentProjectionErrorKind::SpanLimit,
                depth,
                span_limit,
                observed_spans,
            ));
        }
        self.visual_bytes = observed_bytes;
        self.spans = observed_spans;
        spans.push(TextSpan::new(text, style));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use nagi_content::{Content, Element, ElementKind, Role};

    use super::*;
    use crate::{
        DeclarationValue, PresentationDeclaration, PresentationRule, PresentationSelector,
        TextStyleDeclaration,
    };

    #[test]
    fn limits_have_bounded_zero_and_depth_behavior() {
        let defaults = ContentProjectionLimits::default();
        assert_eq!(
            defaults.with_max_content_nodes(0).max_content_nodes(),
            DEFAULT_CONTENT_PROJECTION_MAX_CONTENT_NODES
        );
        assert_eq!(
            defaults.with_max_depth(u32::MAX).max_depth(),
            MAX_CONTENT_PROJECTION_DEPTH
        );
    }

    #[test]
    fn zero_content_and_element_project_safely() {
        project_content::<()>(
            &Content::default(),
            &PresentationSheet::default(),
            Default::default(),
        )
        .expect("zero Content is empty text");
        project_content::<()>(
            &Content::from(Element::default()),
            &PresentationSheet::default(),
            Default::default(),
        )
        .expect("zero Element equivalent is empty Inline");
    }

    #[test]
    fn block_display_inside_inline_content_is_rejected() {
        let content = Content::from(Element::new(
            ElementKind::Paragraph,
            [Content::from(Element::new(ElementKind::Flow, []))],
        ));
        let error =
            project_content::<()>(&content, &PresentationSheet::default(), Default::default())
                .err()
                .expect("block content must not flatten into a paragraph");
        assert_eq!(error.kind(), ContentProjectionErrorKind::InvalidLayoutTree);
        assert_eq!(error.depth(), 2);
        assert_eq!(error.rejected_display(), Some(PresentationDisplay::Flow));
    }

    #[test]
    fn state_resolver_runs_once_per_visited_element() {
        let role = Role::new("item").unwrap();
        let selected = PresentationState::new("selected").unwrap();
        let rule = PresentationRule::new(
            PresentationSelector::Role(role.clone()),
            PresentationDeclaration::default().with_text_style(
                TextStyleDeclaration::default().with_bold(DeclarationValue::Set(true)),
            ),
        )
        .with_required_states([selected.clone()])
        .unwrap();
        let child = Element::new(ElementKind::Inline, [Content::text("value")])
            .with_roles([role])
            .unwrap();
        let root = Content::from(Element::new(ElementKind::Paragraph, [Content::from(child)]));
        let calls = Cell::new(0_u32);
        let active = [selected];
        let _node = project_content_with_states::<(), _>(
            &root,
            &PresentationSheet::new([rule]),
            Default::default(),
            |_| {
                calls.set(calls.get() + 1);
                &active
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn every_resource_limit_reports_the_first_observed_value() {
        let paragraph = Content::from(Element::new(
            ElementKind::Paragraph,
            [Content::text("a"), Content::text("b")],
        ));
        let flow = Content::from(Element::new(
            ElementKind::Flow,
            [Content::text("a"), Content::text("b")],
        ));
        let cases = [
            (
                &paragraph,
                ContentProjectionLimits::default().with_max_content_nodes(2),
                ContentProjectionErrorKind::ContentNodeLimit,
            ),
            (
                &flow,
                ContentProjectionLimits::default().with_max_output_nodes(1),
                ContentProjectionErrorKind::OutputNodeLimit,
            ),
            (
                &paragraph,
                ContentProjectionLimits::default().with_max_spans(1),
                ContentProjectionErrorKind::SpanLimit,
            ),
            (
                &paragraph,
                ContentProjectionLimits::default().with_max_depth(1),
                ContentProjectionErrorKind::DepthLimit,
            ),
            (
                &paragraph,
                ContentProjectionLimits::default().with_max_visual_bytes(1),
                ContentProjectionErrorKind::VisualByteLimit,
            ),
        ];
        for (content, limits, expected) in cases {
            let error = project_content::<()>(
                content,
                &PresentationSheet::default(),
                ContentProjectionOptions::default().with_limits(limits),
            )
            .err()
            .expect("fixture must exceed its configured limit");
            assert_eq!(error.kind(), expected);
            assert_eq!(error.observed(), error.limit().map(|limit| limit + 1));
        }
    }

    #[test]
    fn concrete_empty_separator_creates_no_output_node_or_span() {
        let sequence_role = Role::new("sequence").unwrap();
        let root = Content::from(
            Element::new(
                ElementKind::Sequence,
                [Content::text("a"), Content::text("b")],
            )
            .with_roles([sequence_role.clone()])
            .unwrap(),
        );
        let sheet = PresentationSheet::new([PresentationRule::new(
            PresentationSelector::Role(sequence_role),
            PresentationDeclaration::default()
                .with_visual_separator(DeclarationValue::Set(String::new())),
        )]);
        project_content::<()>(
            &root,
            &sheet,
            ContentProjectionOptions::default().with_limits(
                ContentProjectionLimits::default()
                    .with_max_output_nodes(3)
                    .with_max_spans(2),
            ),
        )
        .expect("empty separator must not consume output resources");
    }
}
