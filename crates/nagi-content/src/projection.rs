use std::ops::Range;

use crate::tree::ContentNode;
use crate::{AnnotationId, Content, SemanticBoundary};

/// One application annotation projected onto a semantic UTF-8 byte range
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationRange {
    annotation: AnnotationId,
    range: Range<usize>,
}

impl AnnotationRange {
    /// Returns the application-resolved annotation identity
    #[must_use]
    pub const fn annotation(&self) -> &AnnotationId {
        &self.annotation
    }

    /// Returns the half-open UTF-8 byte range
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// Returns the inclusive start byte offset
    #[must_use]
    pub const fn start(&self) -> usize {
        self.range.start
    }

    /// Returns the exclusive end byte offset
    #[must_use]
    pub const fn end(&self) -> usize {
        self.range.end
    }
}

/// Owned semantic UTF-8 text and annotation ranges
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticText {
    text: String,
    annotations: Vec<AnnotationRange>,
}

impl SemanticText {
    /// Returns the projected semantic UTF-8 text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns annotation ranges in element preorder
    #[must_use]
    pub fn annotations(&self) -> &[AnnotationRange] {
        &self.annotations
    }

    /// Consumes this projection and returns its semantic UTF-8 text
    #[must_use]
    pub fn into_text(self) -> String {
        self.text
    }
}

/// Projects source-neutral content into semantic text and annotation ranges
#[must_use]
pub fn semantic_text(content: &Content) -> SemanticText {
    let mut projected = SemanticText::default();
    let mut stack = vec![Event::Node(content)];
    while let Some(event) = stack.pop() {
        match event {
            Event::Node(node) => match node.node() {
                ContentNode::Text(value) => projected.text.push_str(value),
                ContentNode::HardBreak => projected.text.push('\n'),
                ContentNode::Element(element) => {
                    if let Some(annotation) = element.annotation() {
                        let range_index = projected.annotations.len();
                        projected.annotations.push(AnnotationRange {
                            annotation: annotation.clone(),
                            range: projected.text.len()..projected.text.len(),
                        });
                        stack.push(Event::CloseAnnotation(range_index));
                    }
                    push_children(&mut stack, element.children(), element.boundary());
                }
            },
            Event::Boundary(boundary) => projected.text.push_str(boundary.as_str()),
            Event::CloseAnnotation(index) => {
                projected.annotations[index].range.end = projected.text.len();
            }
        }
    }
    projected
}

fn push_children<'a>(
    stack: &mut Vec<Event<'a>>,
    children: &'a [Content],
    boundary: SemanticBoundary,
) {
    for index in (0..children.len()).rev() {
        stack.push(Event::Node(&children[index]));
        if index > 0 {
            stack.push(Event::Boundary(boundary));
        }
    }
}

enum Event<'a> {
    Node(&'a Content),
    Boundary(SemanticBoundary),
    CloseAnnotation(usize),
}
