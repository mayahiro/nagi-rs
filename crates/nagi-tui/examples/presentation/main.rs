//! Resolves Terminal Presentation Rules and projects source-neutral Content

use std::error::Error;

use nagi_content::{Content, Element, ElementKind, Role};
use nagi_tui::{
    Color, ContentProjectionOptions, DeclarationValue, Node, PresentationDeclaration,
    PresentationDisplay, PresentationRule, PresentationSelector, PresentationSheet,
    PresentationState, Style, TextStyleDeclaration, project_content_with_states,
};

fn main() -> Result<(), Box<dyn Error>> {
    let heading = Role::new("heading")?;
    let muted = PresentationState::new("muted")?;
    let element = Element::new(ElementKind::Paragraph, [Content::text("Nagi")])
        .with_roles([heading.clone()])?;

    let heading_rule = PresentationRule::new(
        PresentationSelector::Role(heading.clone()),
        PresentationDeclaration::default().with_text_style(
            TextStyleDeclaration::default()
                .with_foreground(DeclarationValue::Set(Color::Indexed(6)))
                .with_bold(DeclarationValue::Set(true)),
        ),
    );
    let muted_rule = PresentationRule::new(
        PresentationSelector::Role(heading),
        PresentationDeclaration::default().with_text_style(
            TextStyleDeclaration::default()
                .with_foreground(DeclarationValue::Initial)
                .with_bold(DeclarationValue::Set(false))
                .with_dim(DeclarationValue::Set(true)),
        ),
    )
    .with_required_states([muted.clone()])?;
    let sheet = PresentationSheet::new([heading_rule, muted_rule]);

    let active_states = [muted];
    let computed = sheet.resolve(&element, Style::default(), &active_states);
    let style = computed.style();
    println!("display={}", display_name(computed.display()));
    println!("foreground={}", color_name(style.foreground));
    println!("bold={} dim={}", style.bold, style.dim);
    let _node: Node<()> = project_content_with_states(
        &element.into_content(),
        &sheet,
        ContentProjectionOptions::default(),
        |_| active_states.as_slice(),
    )?;
    println!("projection=ok");
    Ok(())
}

fn display_name(display: PresentationDisplay) -> &'static str {
    match display {
        PresentationDisplay::Inline => "inline",
        PresentationDisplay::Flow => "flow",
        PresentationDisplay::Paragraph => "paragraph",
        PresentationDisplay::Sequence => "sequence",
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::Default => "default",
        Color::Indexed(_) => "indexed",
        Color::Rgb { .. } => "rgb",
    }
}
