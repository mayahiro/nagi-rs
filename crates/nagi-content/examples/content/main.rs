//! Source-neutral structured content without a renderer dependency

use std::error::Error;

use nagi_content::{
    AnnotationId, Content, Element, ElementId, ElementKind, Limits, Role, semantic_text, validate,
};

fn main() -> Result<(), Box<dyn Error>> {
    let heading = Element::new(ElementKind::Paragraph, [Content::text("Nagi")])
        .with_roles([Role::new("heading")?])?
        .into_content();
    let description = Element::new(
        ElementKind::Paragraph,
        [Content::text("Source-neutral content")],
    )
    .with_annotation(AnnotationId::new("guide.content")?)
    .into_content();
    let document = Element::new(ElementKind::Flow, [heading, description])
        .with_id(ElementId::new("example-document")?)
        .with_revision(1)
        .into_content();

    let projected = semantic_text(&document);
    let stats = validate(&document, Limits::UNLIMITED)?;
    println!("{}", projected.text());
    println!("nodes={} depth={}", stats.nodes, stats.max_depth);
    Ok(())
}
