use std::error::Error;
use std::fmt;
use std::sync::Arc;

use nagi_content::{Class, Element, ElementKind, IdentifierError, Role};
use nagi_vt::{Color, Style};

use crate::{HorizontalAlignment, Length, WrapMode};

/// An application-supplied state used by presentation rule conditions
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PresentationState(Role);

impl PresentationState {
    /// Creates a state using the portable content-token grammar
    pub fn new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        Role::new(value).map(Self)
    }

    /// Creates a state from bytes without repairing invalid UTF-8
    pub fn from_bytes(value: &[u8]) -> Result<Self, IdentifierError> {
        Role::from_bytes(value).map(Self)
    }

    /// Returns the portable state token
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for PresentationState {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PresentationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An exact source-neutral selector for one presentation rule
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum PresentationSelector {
    /// Matches every content element
    #[default]
    Any,
    /// Matches an element carrying the exact semantic role
    Role(Role),
    /// Matches an element carrying the exact presentation class
    Class(Class),
}

impl PresentationSelector {
    /// Reports whether this selector matches `element`
    #[must_use]
    pub fn matches(&self, element: &Element) -> bool {
        match self {
            Self::Any => true,
            Self::Role(role) => element.roles().contains(role),
            Self::Class(class) => element.classes().contains(class),
        }
    }
}

/// One cascade value in a presentation declaration
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum DeclarationValue<T> {
    /// Leaves the inherited or previously resolved property unchanged
    #[default]
    Unspecified,
    /// Replaces the property with a concrete value
    Set(T),
    /// Restores the property's backend-defined initial value
    Initial,
}

/// Terminal text properties contributed by one presentation rule
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct TextStyleDeclaration {
    foreground: DeclarationValue<Color>,
    background: DeclarationValue<Color>,
    underline_color: DeclarationValue<Option<Color>>,
    bold: DeclarationValue<bool>,
    dim: DeclarationValue<bool>,
    italic: DeclarationValue<bool>,
    underline: DeclarationValue<bool>,
    blink: DeclarationValue<bool>,
    reverse: DeclarationValue<bool>,
    hidden: DeclarationValue<bool>,
    strikethrough: DeclarationValue<bool>,
}

impl TextStyleDeclaration {
    /// Returns the foreground color declaration
    #[must_use]
    pub const fn foreground(&self) -> &DeclarationValue<Color> {
        &self.foreground
    }

    /// Returns this declaration with a foreground color property
    #[must_use]
    pub fn with_foreground(mut self, value: DeclarationValue<Color>) -> Self {
        self.foreground = value;
        self
    }

    /// Returns the background color declaration
    #[must_use]
    pub const fn background(&self) -> &DeclarationValue<Color> {
        &self.background
    }

    /// Returns this declaration with a background color property
    #[must_use]
    pub fn with_background(mut self, value: DeclarationValue<Color>) -> Self {
        self.background = value;
        self
    }

    /// Returns the underline color declaration
    #[must_use]
    pub const fn underline_color(&self) -> &DeclarationValue<Option<Color>> {
        &self.underline_color
    }

    /// Returns this declaration with an underline color property
    #[must_use]
    pub fn with_underline_color(mut self, value: DeclarationValue<Option<Color>>) -> Self {
        self.underline_color = value;
        self
    }

    /// Returns the bold declaration
    #[must_use]
    pub const fn bold(&self) -> &DeclarationValue<bool> {
        &self.bold
    }

    /// Returns this declaration with a bold property
    #[must_use]
    pub fn with_bold(mut self, value: DeclarationValue<bool>) -> Self {
        self.bold = value;
        self
    }

    /// Returns the dim declaration
    #[must_use]
    pub const fn dim(&self) -> &DeclarationValue<bool> {
        &self.dim
    }

    /// Returns this declaration with a dim property
    #[must_use]
    pub fn with_dim(mut self, value: DeclarationValue<bool>) -> Self {
        self.dim = value;
        self
    }

    /// Returns the italic declaration
    #[must_use]
    pub const fn italic(&self) -> &DeclarationValue<bool> {
        &self.italic
    }

    /// Returns this declaration with an italic property
    #[must_use]
    pub fn with_italic(mut self, value: DeclarationValue<bool>) -> Self {
        self.italic = value;
        self
    }

    /// Returns the underline declaration
    #[must_use]
    pub const fn underline(&self) -> &DeclarationValue<bool> {
        &self.underline
    }

    /// Returns this declaration with an underline property
    #[must_use]
    pub fn with_underline(mut self, value: DeclarationValue<bool>) -> Self {
        self.underline = value;
        self
    }

    /// Returns the blink declaration
    #[must_use]
    pub const fn blink(&self) -> &DeclarationValue<bool> {
        &self.blink
    }

    /// Returns this declaration with a blink property
    #[must_use]
    pub fn with_blink(mut self, value: DeclarationValue<bool>) -> Self {
        self.blink = value;
        self
    }

    /// Returns the reverse declaration
    #[must_use]
    pub const fn reverse(&self) -> &DeclarationValue<bool> {
        &self.reverse
    }

    /// Returns this declaration with a reverse property
    #[must_use]
    pub fn with_reverse(mut self, value: DeclarationValue<bool>) -> Self {
        self.reverse = value;
        self
    }

    /// Returns the hidden declaration
    #[must_use]
    pub const fn hidden(&self) -> &DeclarationValue<bool> {
        &self.hidden
    }

    /// Returns this declaration with a hidden property
    #[must_use]
    pub fn with_hidden(mut self, value: DeclarationValue<bool>) -> Self {
        self.hidden = value;
        self
    }

    /// Returns the strikethrough declaration
    #[must_use]
    pub const fn strikethrough(&self) -> &DeclarationValue<bool> {
        &self.strikethrough
    }

    /// Returns this declaration with a strikethrough property
    #[must_use]
    pub fn with_strikethrough(mut self, value: DeclarationValue<bool>) -> Self {
        self.strikethrough = value;
        self
    }
}

/// Terminal layout shape selected for one content element
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PresentationDisplay {
    /// Presents content inline
    #[default]
    Inline,
    /// Presents content as a flow
    Flow,
    /// Presents content as a paragraph
    Paragraph,
    /// Presents content as an ordered sequence
    Sequence,
}

impl PresentationDisplay {
    /// Returns the initial terminal display for a mechanical content kind
    #[must_use]
    pub const fn from_element_kind(kind: ElementKind) -> Self {
        match kind {
            ElementKind::Inline => Self::Inline,
            ElementKind::Flow => Self::Flow,
            ElementKind::Paragraph => Self::Paragraph,
            ElementKind::Sequence => Self::Sequence,
        }
    }
}

impl From<ElementKind> for PresentationDisplay {
    fn from(kind: ElementKind) -> Self {
        Self::from_element_kind(kind)
    }
}

/// Terminal layout and text properties contributed by one presentation rule
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PresentationDeclaration {
    display: DeclarationValue<PresentationDisplay>,
    length: DeclarationValue<Length>,
    gap: DeclarationValue<u32>,
    visual_separator: DeclarationValue<String>,
    wrap: DeclarationValue<WrapMode>,
    alignment: DeclarationValue<HorizontalAlignment>,
    text_style: TextStyleDeclaration,
}

impl PresentationDeclaration {
    /// Returns the display declaration
    #[must_use]
    pub const fn display(&self) -> &DeclarationValue<PresentationDisplay> {
        &self.display
    }

    /// Returns this declaration with a display property
    #[must_use]
    pub fn with_display(mut self, value: DeclarationValue<PresentationDisplay>) -> Self {
        self.display = value;
        self
    }

    /// Returns the main-axis length declaration
    #[must_use]
    pub const fn length(&self) -> &DeclarationValue<Length> {
        &self.length
    }

    /// Returns this declaration with a main-axis length property
    #[must_use]
    pub fn with_length(mut self, value: DeclarationValue<Length>) -> Self {
        self.length = value;
        self
    }

    /// Returns the inter-child gap declaration
    #[must_use]
    pub const fn gap(&self) -> &DeclarationValue<u32> {
        &self.gap
    }

    /// Returns this declaration with an inter-child gap property
    #[must_use]
    pub fn with_gap(mut self, value: DeclarationValue<u32>) -> Self {
        self.gap = value;
        self
    }

    /// Returns the visual separator declaration
    #[must_use]
    pub const fn visual_separator(&self) -> &DeclarationValue<String> {
        &self.visual_separator
    }

    /// Returns this declaration with a visual separator property
    #[must_use]
    pub fn with_visual_separator(mut self, value: DeclarationValue<String>) -> Self {
        self.visual_separator = value;
        self
    }

    /// Returns the paragraph wrapping declaration
    #[must_use]
    pub const fn wrap(&self) -> &DeclarationValue<WrapMode> {
        &self.wrap
    }

    /// Returns this declaration with a paragraph wrapping property
    #[must_use]
    pub fn with_wrap(mut self, value: DeclarationValue<WrapMode>) -> Self {
        self.wrap = value;
        self
    }

    /// Returns the horizontal alignment declaration
    #[must_use]
    pub const fn alignment(&self) -> &DeclarationValue<HorizontalAlignment> {
        &self.alignment
    }

    /// Returns this declaration with a horizontal alignment property
    #[must_use]
    pub fn with_alignment(mut self, value: DeclarationValue<HorizontalAlignment>) -> Self {
        self.alignment = value;
        self
    }

    /// Returns the terminal text-style declaration
    #[must_use]
    pub const fn text_style(&self) -> &TextStyleDeclaration {
        &self.text_style
    }

    /// Returns this declaration with terminal text-style properties
    #[must_use]
    pub fn with_text_style(mut self, value: TextStyleDeclaration) -> Self {
        self.text_style = value;
        self
    }
}

/// Error returned when a rule requires the same state more than once
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicatePresentationState {
    state: PresentationState,
}

impl DuplicatePresentationState {
    /// Returns the repeated required state
    #[must_use]
    pub const fn state(&self) -> &PresentationState {
        &self.state
    }
}

impl fmt::Display for DuplicatePresentationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "duplicate required presentation state {}",
            self.state
        )
    }
}

impl Error for DuplicatePresentationState {}

/// One ordered exact-match presentation rule
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PresentationRule {
    selector: PresentationSelector,
    required_states: Arc<[PresentationState]>,
    declaration: PresentationDeclaration,
}

impl PresentationRule {
    /// Creates an unconditional rule for `selector`
    #[must_use]
    pub fn new(selector: PresentationSelector, declaration: PresentationDeclaration) -> Self {
        Self {
            selector,
            required_states: Arc::from([]),
            declaration,
        }
    }

    /// Returns the rule selector
    #[must_use]
    pub const fn selector(&self) -> &PresentationSelector {
        &self.selector
    }

    /// Returns the states that must all be active for this rule to match
    #[must_use]
    pub fn required_states(&self) -> &[PresentationState] {
        &self.required_states
    }

    /// Returns the rule declaration
    #[must_use]
    pub const fn declaration(&self) -> &PresentationDeclaration {
        &self.declaration
    }

    /// Returns this rule with a validated all-of required-state condition
    pub fn with_required_states(
        mut self,
        states: impl IntoIterator<Item = PresentationState>,
    ) -> Result<Self, DuplicatePresentationState> {
        let states: Vec<PresentationState> = states.into_iter().collect();
        for (index, state) in states.iter().enumerate() {
            if states[..index].contains(state) {
                return Err(DuplicatePresentationState {
                    state: state.clone(),
                });
            }
        }
        self.required_states = Arc::from(states);
        Ok(self)
    }

    fn matches(&self, element: &Element, states: &[PresentationState]) -> bool {
        self.selector.matches(element)
            && self
                .required_states
                .iter()
                .all(|required| states.contains(required))
    }
}

/// An immutable source-ordered presentation rule sheet
#[derive(Clone, Debug, Default)]
pub struct PresentationSheet {
    rules: Arc<[PresentationRule]>,
}

impl PresentationSheet {
    /// Creates a sheet preserving the supplied source order
    #[must_use]
    pub fn new(rules: impl IntoIterator<Item = PresentationRule>) -> Self {
        Self {
            rules: Arc::from(rules.into_iter().collect::<Vec<_>>()),
        }
    }

    /// Returns the source-ordered rules
    #[must_use]
    pub fn rules(&self) -> &[PresentationRule] {
        &self.rules
    }

    /// Reports whether this sheet contains no rules
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Returns the number of source-ordered rules
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Resolves one element without allocating
    ///
    /// Text properties begin with `inherited_style`. Layout properties begin
    /// with backend initial values and are not inherited. Matching rules are
    /// applied in source order, independently for every property
    #[must_use]
    pub fn resolve<'sheet>(
        &'sheet self,
        element: &Element,
        inherited_style: Style,
        states: &[PresentationState],
    ) -> ComputedPresentation<'sheet> {
        let initial_display = PresentationDisplay::from_element_kind(element.kind());
        let mut computed = ComputedPresentation::initial(initial_display, inherited_style);
        for rule in self.rules.iter() {
            if rule.matches(element, states) {
                computed.apply(rule.declaration(), initial_display);
            }
        }
        computed
    }
}

/// Fully cascaded terminal presentation for one content element
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ComputedPresentation<'sheet> {
    style: Style,
    display: PresentationDisplay,
    length: Length,
    gap: u32,
    visual_separator: Option<&'sheet str>,
    wrap: WrapMode,
    alignment: HorizontalAlignment,
}

impl<'sheet> ComputedPresentation<'sheet> {
    fn initial(display: PresentationDisplay, inherited_style: Style) -> Self {
        Self {
            style: inherited_style,
            display,
            length: Length::Auto,
            gap: 0,
            visual_separator: None,
            wrap: WrapMode::Word,
            alignment: HorizontalAlignment::Start,
        }
    }

    fn apply(
        &mut self,
        declaration: &'sheet PresentationDeclaration,
        initial_display: PresentationDisplay,
    ) {
        apply_value(&mut self.display, declaration.display(), initial_display);
        apply_value(&mut self.length, declaration.length(), Length::Auto);
        apply_value(&mut self.gap, declaration.gap(), 0);
        match declaration.visual_separator() {
            DeclarationValue::Unspecified => {}
            DeclarationValue::Set(value) => self.visual_separator = Some(value.as_str()),
            DeclarationValue::Initial => self.visual_separator = None,
        }
        apply_value(&mut self.wrap, declaration.wrap(), WrapMode::Word);
        apply_value(
            &mut self.alignment,
            declaration.alignment(),
            HorizontalAlignment::Start,
        );
        apply_text_style(&mut self.style, declaration.text_style());
    }

    /// Returns the computed terminal cell style
    #[must_use]
    pub const fn style(&self) -> Style {
        self.style
    }

    /// Returns the computed terminal display
    #[must_use]
    pub const fn display(&self) -> PresentationDisplay {
        self.display
    }

    /// Returns the computed main-axis length
    #[must_use]
    pub const fn length(&self) -> Length {
        self.length
    }

    /// Returns the computed inter-child gap in cells
    #[must_use]
    pub const fn gap(&self) -> u32 {
        self.gap
    }

    /// Returns the computed visual separator, if any
    #[must_use]
    pub const fn visual_separator(&self) -> Option<&'sheet str> {
        self.visual_separator
    }

    /// Returns the computed paragraph wrapping behavior
    #[must_use]
    pub const fn wrap(&self) -> WrapMode {
        self.wrap
    }

    /// Returns the computed horizontal alignment
    #[must_use]
    pub const fn alignment(&self) -> HorizontalAlignment {
        self.alignment
    }
}

fn apply_value<T: Copy>(current: &mut T, declaration: &DeclarationValue<T>, initial: T) {
    match declaration {
        DeclarationValue::Unspecified => {}
        DeclarationValue::Set(value) => *current = *value,
        DeclarationValue::Initial => *current = initial,
    }
}

fn apply_text_style(style: &mut Style, declaration: &TextStyleDeclaration) {
    let initial = Style::default();
    apply_value(
        &mut style.foreground,
        declaration.foreground(),
        initial.foreground,
    );
    apply_value(
        &mut style.background,
        declaration.background(),
        initial.background,
    );
    apply_value(
        &mut style.underline_color,
        declaration.underline_color(),
        initial.underline_color,
    );
    apply_value(&mut style.bold, declaration.bold(), initial.bold);
    apply_value(&mut style.dim, declaration.dim(), initial.dim);
    apply_value(&mut style.italic, declaration.italic(), initial.italic);
    apply_value(
        &mut style.underline,
        declaration.underline(),
        initial.underline,
    );
    apply_value(&mut style.blink, declaration.blink(), initial.blink);
    apply_value(&mut style.reverse, declaration.reverse(), initial.reverse);
    apply_value(&mut style.hidden, declaration.hidden(), initial.hidden);
    apply_value(
        &mut style.strikethrough,
        declaration.strikethrough(),
        initial.strikethrough,
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nagi_content::{Class, Content, Element, ElementKind, IdentifierErrorKind, Role};

    use super::*;

    #[test]
    fn presentation_state_reuses_the_portable_token_grammar() {
        assert_eq!(PresentationState::new("active").unwrap().as_str(), "active");
        assert_eq!(
            PresentationState::new("Active").unwrap_err().kind(),
            IdentifierErrorKind::InvalidSegmentStart
        );
    }

    #[test]
    fn selectors_keep_any_role_and_class_namespaces_distinct() {
        let shared_role = Role::new("shared").unwrap();
        let shared_class = Class::new("shared").unwrap();
        let sheet = PresentationSheet::new([
            rule(
                PresentationSelector::Any,
                text(TextStyleDeclaration::default().with_dim(DeclarationValue::Set(true))),
            ),
            rule(
                PresentationSelector::Role(shared_role.clone()),
                text(TextStyleDeclaration::default().with_bold(DeclarationValue::Set(true))),
            ),
            rule(
                PresentationSelector::Class(shared_class.clone()),
                text(TextStyleDeclaration::default().with_italic(DeclarationValue::Set(true))),
            ),
        ]);
        let role_element = Element::new(ElementKind::Inline, [])
            .with_roles([shared_role])
            .unwrap();
        let class_element = Element::new(ElementKind::Inline, [])
            .with_classes([shared_class])
            .unwrap();

        let role_style = sheet.resolve(&role_element, Style::default(), &[]).style();
        let class_style = sheet.resolve(&class_element, Style::default(), &[]).style();

        assert!(role_style.dim);
        assert!(role_style.bold);
        assert!(!role_style.italic);
        assert!(class_style.dim);
        assert!(!class_style.bold);
        assert!(class_style.italic);
    }

    #[test]
    fn required_states_preserve_input_order_and_reject_duplicates() {
        let active = PresentationState::new("active").unwrap();
        let selected = PresentationState::new("selected").unwrap();
        let conditional = rule(
            PresentationSelector::Any,
            text(TextStyleDeclaration::default().with_underline(DeclarationValue::Set(true))),
        )
        .with_required_states([selected.clone(), active.clone()])
        .unwrap();

        assert_eq!(
            conditional.required_states(),
            &[selected.clone(), active.clone()]
        );

        let error = rule(
            PresentationSelector::Any,
            PresentationDeclaration::default(),
        )
        .with_required_states([active.clone(), active.clone()])
        .unwrap_err();
        assert_eq!(error.state(), &active);
    }

    #[test]
    fn required_states_match_as_all_of_independent_of_active_order() {
        let active = PresentationState::new("active").unwrap();
        let selected = PresentationState::new("selected").unwrap();
        let conditional = rule(
            PresentationSelector::Any,
            text(TextStyleDeclaration::default().with_underline(DeclarationValue::Set(true))),
        )
        .with_required_states([selected.clone(), active.clone()])
        .unwrap();
        let sheet = PresentationSheet::new([conditional]);
        let element = Element::default();

        assert!(
            !sheet
                .resolve(&element, Style::default(), std::slice::from_ref(&active))
                .style()
                .underline
        );
        assert!(
            sheet
                .resolve(
                    &element,
                    Style::default(),
                    &[active.clone(), selected.clone()],
                )
                .style()
                .underline
        );
        assert!(
            sheet
                .resolve(&element, Style::default(), &[selected, active])
                .style()
                .underline
        );
    }

    #[test]
    fn source_order_is_property_specific_and_later_rules_win() {
        let role = Role::new("status").unwrap();
        let class = Class::new("emphasis").unwrap();
        let sheet = PresentationSheet::new([
            rule(
                PresentationSelector::Any,
                PresentationDeclaration::default()
                    .with_visual_separator(DeclarationValue::Set(" | ".to_owned()))
                    .with_text_style(
                        TextStyleDeclaration::default()
                            .with_foreground(DeclarationValue::Set(Color::Indexed(1)))
                            .with_bold(DeclarationValue::Set(true)),
                    ),
            ),
            rule(
                PresentationSelector::Class(class.clone()),
                text(
                    TextStyleDeclaration::default()
                        .with_foreground(DeclarationValue::Set(Color::Indexed(2))),
                ),
            ),
            rule(
                PresentationSelector::Role(role.clone()),
                text(TextStyleDeclaration::default().with_bold(DeclarationValue::Set(false))),
            ),
        ]);
        let element = Element::new(ElementKind::Inline, [])
            .with_roles([role])
            .unwrap()
            .with_classes([class])
            .unwrap();

        let computed = sheet.resolve(&element, Style::default(), &[]);

        assert_eq!(computed.style().foreground, Color::Indexed(2));
        assert!(!computed.style().bold);
        assert_eq!(computed.visual_separator(), Some(" | "));
    }

    #[test]
    fn text_properties_inherit_and_initial_resets_every_style_field() {
        let inherited = Style {
            foreground: Color::Indexed(1),
            background: Color::Indexed(2),
            underline_color: Some(Color::Indexed(3)),
            bold: true,
            dim: true,
            italic: true,
            underline: true,
            blink: true,
            reverse: true,
            hidden: true,
            strikethrough: true,
        };
        let element = Element::default();

        assert_eq!(
            PresentationSheet::default()
                .resolve(&element, inherited, &[])
                .style(),
            inherited
        );

        let reset = TextStyleDeclaration::default()
            .with_foreground(DeclarationValue::Initial)
            .with_background(DeclarationValue::Initial)
            .with_underline_color(DeclarationValue::Initial)
            .with_bold(DeclarationValue::Initial)
            .with_dim(DeclarationValue::Initial)
            .with_italic(DeclarationValue::Initial)
            .with_underline(DeclarationValue::Initial)
            .with_blink(DeclarationValue::Initial)
            .with_reverse(DeclarationValue::Initial)
            .with_hidden(DeclarationValue::Initial)
            .with_strikethrough(DeclarationValue::Initial);
        let sheet = PresentationSheet::new([rule(PresentationSelector::Any, text(reset))]);

        assert_eq!(
            sheet.resolve(&element, inherited, &[]).style(),
            Style::default()
        );
    }

    #[test]
    fn layout_defaults_follow_element_kind_and_initial_restores_them() {
        for (kind, expected) in [
            (ElementKind::Inline, PresentationDisplay::Inline),
            (ElementKind::Flow, PresentationDisplay::Flow),
            (ElementKind::Paragraph, PresentationDisplay::Paragraph),
            (ElementKind::Sequence, PresentationDisplay::Sequence),
        ] {
            let element = Element::new(kind, [Content::text("value")]);
            let sheet = PresentationSheet::default();
            let computed = sheet.resolve(&element, Style::default(), &[]);
            assert_eq!(computed.display(), expected);
            assert_eq!(computed.length(), Length::Auto);
            assert_eq!(computed.gap(), 0);
            assert_eq!(computed.visual_separator(), None);
            assert_eq!(computed.wrap(), WrapMode::Word);
            assert_eq!(computed.alignment(), HorizontalAlignment::Start);
        }

        let set = PresentationDeclaration::default()
            .with_display(DeclarationValue::Set(PresentationDisplay::Sequence))
            .with_length(DeclarationValue::Set(Length::Fixed(9)))
            .with_gap(DeclarationValue::Set(3))
            .with_visual_separator(DeclarationValue::Set("separator".to_owned()))
            .with_wrap(DeclarationValue::Set(WrapMode::None))
            .with_alignment(DeclarationValue::Set(HorizontalAlignment::End));
        let reset = PresentationDeclaration::default()
            .with_display(DeclarationValue::Initial)
            .with_length(DeclarationValue::Initial)
            .with_gap(DeclarationValue::Initial)
            .with_visual_separator(DeclarationValue::Initial)
            .with_wrap(DeclarationValue::Initial)
            .with_alignment(DeclarationValue::Initial);
        let sheet = PresentationSheet::new([
            rule(PresentationSelector::Any, set),
            rule(PresentationSelector::Any, reset),
        ]);
        let flow = Element::new(ElementKind::Flow, []);
        let computed = sheet.resolve(&flow, Style::default(), &[]);

        assert_eq!(computed.display(), PresentationDisplay::Flow);
        assert_eq!(computed.length(), Length::Auto);
        assert_eq!(computed.gap(), 0);
        assert_eq!(computed.visual_separator(), None);
        assert_eq!(computed.wrap(), WrapMode::Word);
        assert_eq!(computed.alignment(), HorizontalAlignment::Start);
    }

    #[test]
    fn sheet_clones_share_immutable_rule_storage() {
        let sheet = PresentationSheet::new([rule(
            PresentationSelector::Any,
            PresentationDeclaration::default(),
        )]);
        let clone = sheet.clone();

        assert!(Arc::ptr_eq(&sheet.rules, &clone.rules));
        assert_eq!(sheet.len(), 1);
        assert!(!sheet.is_empty());
    }

    fn rule(
        selector: PresentationSelector,
        declaration: PresentationDeclaration,
    ) -> PresentationRule {
        PresentationRule::new(selector, declaration)
    }

    fn text(text_style: TextStyleDeclaration) -> PresentationDeclaration {
        PresentationDeclaration::default().with_text_style(text_style)
    }
}
