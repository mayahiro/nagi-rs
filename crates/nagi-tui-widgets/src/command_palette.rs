use std::sync::Arc;

use nagi_tui::{Action, ActionAvailability, ActionDescriptor, EventResult, Node, NodeId, Style};

use crate::action::{
    COLLECTION_ACTIONS, CollectionAction, activate_action_descriptor,
    vertical_collection_action_descriptors,
};
use crate::event::is_pointer_activation_event;
use crate::navigation::navigate;

/// One searchable command rendered by a [`CommandPalette`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    id: NodeId,
    label: String,
    keywords: Vec<String>,
}

impl Command {
    /// Creates a command with an application-defined stable identity
    #[must_use]
    pub fn new(id: impl Into<NodeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            keywords: Vec::new(),
        }
    }

    /// Adds terms considered during ASCII case-insensitive filtering
    #[must_use]
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the command's stable identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the displayed label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns additional search terms
    #[must_use]
    pub fn search_keywords(&self) -> &[String] {
        &self.keywords
    }
}

/// Visual styles used by a [`CommandPalette`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandPaletteStyle {
    /// Style used by the outer border
    pub border: Style,
    /// Style used by an optional title
    pub title: Style,
    /// Style used by query text
    pub query: Style,
    /// Style used by query placeholder text
    pub placeholder: Style,
    /// Style used by unselected commands
    pub normal: Style,
    /// Style used by the application-selected command
    pub selected: Style,
    /// Style merged over the command or input that owns focus
    pub focused: Style,
    /// Style used by the empty result notice
    pub empty: Style,
    /// Style used by query and command text while disabled
    pub disabled: Style,
}

impl Default for CommandPaletteStyle {
    fn default() -> Self {
        Self {
            border: Style::default(),
            title: Style {
                bold: true,
                ..Style::default()
            },
            query: Style::default(),
            placeholder: Style {
                dim: true,
                ..Style::default()
            },
            normal: Style::default(),
            selected: Style {
                reverse: true,
                ..Style::default()
            },
            focused: Style {
                underline: true,
                ..Style::default()
            },
            empty: Style {
                dim: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A searchable command chooser with application-owned query and selection
pub struct CommandPalette<Message> {
    id: NodeId,
    input_id: NodeId,
    query: String,
    commands: Vec<Command>,
    selected: usize,
    enabled: bool,
    title: String,
    placeholder: String,
    empty_label: String,
    style: CommandPaletteStyle,
    on_query: Arc<dyn Fn(String) -> Message>,
    on_select: Arc<dyn Fn(usize) -> Message>,
    on_activate: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> CommandPalette<Message> {
    /// Creates an enabled palette using original command indices for selection
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<NodeId>,
        input_id: impl Into<NodeId>,
        query: impl Into<String>,
        commands: impl IntoIterator<Item = Command>,
        selected: usize,
        on_query: impl Fn(String) -> Message + 'static,
        on_select: impl Fn(usize) -> Message + 'static,
        on_activate: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            input_id: input_id.into(),
            query: query.into(),
            commands: commands.into_iter().collect(),
            selected,
            enabled: true,
            title: String::new(),
            placeholder: "Type to filter".to_owned(),
            empty_label: "No matching commands".to_owned(),
            style: CommandPaletteStyle::default(),
            on_query: Arc::new(on_query),
            on_select: Arc::new(on_select),
            on_activate: Arc::new(on_activate),
        }
    }

    /// Sets whether the input and command rows can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets an optional title rendered above the query
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets query placeholder text
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the notice rendered when no command matches
    #[must_use]
    pub fn empty_label(mut self, label: impl Into<String>) -> Self {
        self.empty_label = label.into();
        self
    }

    /// Replaces the command palette styles
    #[must_use]
    pub const fn style(mut self, style: CommandPaletteStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns the semantic activation descriptor declared by each command row
    ///
    /// The descriptor is disabled-pass-through when the palette is disabled or
    /// its current filter has no matching command
    #[must_use]
    pub fn command_action_descriptor(&self) -> ActionDescriptor {
        command_palette_command_action_descriptor(
            self.enabled && has_visible_command(&self.commands, &self.query),
        )
    }

    /// Returns the ordered semantic descriptors declared by the palette root
    ///
    /// The order is activate, previous, next, first, and last. Every descriptor
    /// is disabled-pass-through when the palette is disabled or its current
    /// filter has no matching command
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 5] {
        command_palette_action_descriptors(
            self.enabled && has_visible_command(&self.commands, &self.query),
        )
    }

    /// Builds the public semantic node for this command palette
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let visible = Arc::new(filtered_indices(&self.commands, &self.query));
        let selected_position = normalized_filtered_selection(&visible, self.selected);
        let actions_enabled = self.enabled && selected_position.is_some();
        let command_descriptor = command_palette_command_action_descriptor(actions_enabled);
        let action_descriptors = command_palette_action_descriptors(actions_enabled);
        let visible_ids: Arc<Vec<NodeId>> = Arc::new(
            visible
                .iter()
                .map(|index| self.commands[*index].id.clone())
                .collect(),
        );
        let mut children = Vec::with_capacity(visible.len().saturating_add(2));
        if !self.title.is_empty() {
            children.push(Node::styled_text(self.title, self.style.title));
        }
        if self.enabled {
            let on_query = Arc::clone(&self.on_query);
            children.push(
                Node::text_input_styled(
                    self.input_id,
                    self.query.clone(),
                    self.placeholder,
                    self.style.query,
                    self.style.placeholder,
                    move |query| on_query(query),
                )
                .with_focused_style(self.style.focused),
            );
        } else {
            let query = if self.query.is_empty() {
                self.placeholder
            } else {
                self.query.clone()
            };
            children.push(Node::styled_text(format!("> {query}"), self.style.disabled));
        }

        if visible.is_empty() {
            children.push(Node::styled_text(self.empty_label, self.style.empty));
        } else {
            for (position, original_index) in visible.iter().copied().enumerate() {
                let command = &self.commands[original_index];
                let is_selected = selected_position == Some(position);
                let marker = if is_selected { "> " } else { "  " };
                let style = if !self.enabled {
                    self.style.disabled
                } else if is_selected {
                    self.style.selected
                } else {
                    self.style.normal
                };
                let node = Node::styled_text(format!("{marker}{}", command.label), style);
                if !self.enabled {
                    let id = command.id.clone();
                    children.push(node.with_id(id.clone()).on_actions(
                        id,
                        [Action::new(command_descriptor.clone(), |_| {
                            EventResult::ignored()
                        })],
                    ));
                    continue;
                }
                let id = command.id.clone();
                let action_focus_id = id.clone();
                let action_on_select = Arc::clone(&self.on_select);
                let action_on_activate = Arc::clone(&self.on_activate);
                let pointer_focus_id = id.clone();
                let pointer_on_select = Arc::clone(&self.on_select);
                let pointer_on_activate = Arc::clone(&self.on_activate);
                children.push(
                    node.focusable(id.clone())
                        .with_focused_style(self.style.focused)
                        .on_actions(
                            id.clone(),
                            [Action::new(command_descriptor.clone(), move |_| {
                                command_activation_result(
                                    is_selected,
                                    original_index,
                                    &action_focus_id,
                                    action_on_select.as_ref(),
                                    action_on_activate.as_ref(),
                                )
                            })],
                        )
                        .on_event(id, move |event| {
                            if !is_pointer_activation_event(event) {
                                return EventResult::ignored();
                            }
                            command_activation_result(
                                is_selected,
                                original_index,
                                &pointer_focus_id,
                                pointer_on_select.as_ref(),
                                pointer_on_activate.as_ref(),
                            )
                        }),
                );
            }
        }

        let root_id = self.id;
        let root = Node::border(Node::column(children), self.style.border).with_id(root_id.clone());
        let Some(selected_position) = selected_position.filter(|_| self.enabled) else {
            return root.on_actions(
                root_id,
                action_descriptors
                    .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            );
        };
        let on_select = self.on_select;
        let on_activate = self.on_activate;
        let actions =
            action_descriptors
                .into_iter()
                .zip(COLLECTION_ACTIONS)
                .map(|(descriptor, action)| {
                    let visible = Arc::clone(&visible);
                    let visible_ids = Arc::clone(&visible_ids);
                    let on_select = Arc::clone(&on_select);
                    let on_activate = Arc::clone(&on_activate);
                    Action::new(descriptor, move |_| {
                        command_palette_action_result(
                            action,
                            selected_position,
                            visible.as_ref(),
                            visible_ids.as_ref(),
                            on_select.as_ref(),
                            on_activate.as_ref(),
                        )
                    })
                });
        root.on_actions(root_id, actions)
    }
}

fn command_palette_command_action_descriptor(enabled: bool) -> ActionDescriptor {
    activate_action_descriptor().with_availability(command_palette_action_availability(enabled))
}

fn command_palette_action_descriptors(enabled: bool) -> [ActionDescriptor; 5] {
    vertical_collection_action_descriptors(enabled)
}

const fn command_palette_action_availability(enabled: bool) -> ActionAvailability {
    if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    }
}

fn command_activation_result<Message>(
    is_selected: bool,
    original_index: usize,
    focus_id: &NodeId,
    on_select: &dyn Fn(usize) -> Message,
    on_activate: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let mut result = EventResult::consumed().focus(focus_id.clone());
    if !is_selected {
        result = result.emit(on_select(original_index));
    }
    result.emit(on_activate(original_index))
}

fn command_palette_action_result<Message>(
    action: CollectionAction,
    selected_position: usize,
    visible: &[usize],
    visible_ids: &[NodeId],
    on_select: &dyn Fn(usize) -> Message,
    on_activate: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let Some(navigation) = action.navigation() else {
        return EventResult::message(on_activate(visible[selected_position]));
    };
    let next = navigate(visible.len(), selected_position, navigation).unwrap_or(selected_position);
    let mut result = EventResult::consumed().focus(visible_ids[next].clone());
    if next != selected_position {
        result = result.emit(on_select(visible[next]));
    }
    result
}

fn filtered_indices(commands: &[Command], query: &str) -> Vec<usize> {
    commands
        .iter()
        .enumerate()
        .filter_map(|(index, command)| command_matches(command, query).then_some(index))
        .collect()
}

fn has_visible_command(commands: &[Command], query: &str) -> bool {
    commands
        .iter()
        .any(|command| command_matches(command, query))
}

fn command_matches(command: &Command, query: &str) -> bool {
    query.is_empty()
        || contains_ascii_case_insensitive(&command.label, query)
        || command
            .keywords
            .iter()
            .any(|keyword| contains_ascii_case_insensitive(keyword, query))
}

fn contains_ascii_case_insensitive(value: &str, query: &str) -> bool {
    value
        .as_bytes()
        .windows(query.len())
        .any(|window| window.eq_ignore_ascii_case(query.as_bytes()))
}

fn normalized_filtered_selection(visible: &[usize], selected: usize) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    Some(
        visible
            .iter()
            .rposition(|index| *index <= selected)
            .unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Command, command_matches, command_palette_action_descriptors,
        command_palette_command_action_descriptor,
    };

    #[test]
    fn filtering_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/command-filter.txt",
            "widget-command-filter",
            &["label", "keywords", "query", "match"],
        ) else {
            return;
        };
        for record in records {
            let keywords = match record.field("keywords") {
                "-" => Vec::new(),
                value => value.split(',').map(str::to_owned).collect(),
            };
            let command = Command::new("command", record.text("label")).keywords(keywords);
            assert_eq!(
                command_matches(&command, &record.text("query")),
                boolean(record.field("match")),
                "case {}",
                record.id
            );
        }
    }

    #[test]
    fn descriptor_clones_reuse_immutable_storage() {
        let enabled_command = command_palette_command_action_descriptor(true);
        let disabled_command = command_palette_command_action_descriptor(false);
        assert!(std::ptr::eq(
            enabled_command.default_bindings(),
            disabled_command.default_bindings()
        ));

        let enabled_root = command_palette_action_descriptors(true);
        let disabled_root = command_palette_action_descriptors(false);
        for index in 0..enabled_root.len() {
            assert!(std::ptr::eq(
                enabled_root[index].id().as_str(),
                disabled_root[index].id().as_str()
            ));
            assert!(std::ptr::eq(
                enabled_root[index].label(),
                disabled_root[index].label()
            ));
            assert!(std::ptr::eq(
                enabled_root[index].default_bindings(),
                disabled_root[index].default_bindings()
            ));
        }
    }

    fn boolean(value: &str) -> bool {
        match value {
            "true" => true,
            "false" => false,
            _ => panic!("invalid Boolean {value}"),
        }
    }
}
