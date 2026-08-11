use std::collections::HashMap;
use std::rc::Rc;

use crate::core_action::{
    CoreAction, action_group as core_action_group,
    resolve_action_group as resolve_core_action_group,
};
use crate::keymap::{Action, ActionEvent};
use crate::routing::TreeIndex;
use crate::{
    ActionDescriptor, BindingConflict, Event, EventResult, KeyMap, KeyScope, KeyScopePropagation,
    KeyStroke, NodeId, ResolvedActions, resolve_actions,
};

pub(crate) struct NodeKeyInteraction<Message> {
    actions: Rc<[Action<Message>]>,
    scope: Option<NodeKeyScope>,
    reveal_target: Option<NodeId>,
    focus_fallback: Option<NodeId>,
}

impl<Message> NodeKeyInteraction<Message> {
    pub(crate) fn new() -> Self {
        Self {
            actions: Rc::from([]),
            scope: None,
            reveal_target: None,
            focus_fallback: None,
        }
    }

    pub(crate) fn set_actions(&mut self, actions: impl IntoIterator<Item = Action<Message>>) {
        self.actions = actions.into_iter().collect();
    }

    pub(crate) fn set_scope(&mut self, key_map: KeyMap, propagation: KeyScopePropagation) {
        self.scope = Some(NodeKeyScope {
            key_map,
            propagation,
        });
    }

    pub(crate) fn set_reveal_target(&mut self, target: NodeId) {
        self.reveal_target = Some(target);
    }

    pub(crate) fn reveal_target(&self) -> Option<&NodeId> {
        self.reveal_target.as_ref()
    }

    pub(crate) fn set_focus_fallback(&mut self, target: NodeId) {
        self.focus_fallback = Some(target);
    }

    pub(crate) fn focus_fallback(&self) -> Option<&NodeId> {
        self.focus_fallback.as_ref()
    }

    fn is_empty(&self) -> bool {
        self.actions.is_empty() && self.scope.is_none()
    }
}

#[derive(Clone)]
struct NodeKeyScope {
    key_map: KeyMap,
    propagation: KeyScopePropagation,
}

pub(crate) struct ActionIndex<Message> {
    records: Vec<ActionNode<Message>>,
    by_id: HashMap<NodeId, usize>,
    has_actions: bool,
}

impl<Message> Default for ActionIndex<Message> {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            by_id: HashMap::new(),
            has_actions: false,
        }
    }
}

impl<Message> ActionIndex<Message> {
    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.by_id.clear();
        self.has_actions = false;
    }

    pub(crate) fn register(&mut self, id: &NodeId, interaction: &NodeKeyInteraction<Message>) {
        if interaction.is_empty() {
            return;
        }
        self.has_actions |= !interaction.actions.is_empty();
        self.by_id.insert(id.clone(), self.records.len());
        let descriptors = interaction
            .actions
            .iter()
            .map(|action| action.descriptor().clone())
            .collect();
        self.records.push(ActionNode {
            id: id.clone(),
            actions: interaction.actions.clone(),
            descriptors,
            scope: interaction.scope.clone(),
        });
    }

    pub(crate) const fn has_actions(&self) -> bool {
        self.has_actions
    }

    fn record(&self, id: &NodeId) -> Option<&ActionNode<Message>> {
        self.by_id.get(id).map(|index| &self.records[*index])
    }
}

struct ActionNode<Message> {
    id: NodeId,
    actions: Rc<[Action<Message>]>,
    descriptors: Rc<[ActionDescriptor]>,
    scope: Option<NodeKeyScope>,
}

pub(crate) fn validate_action_owners<Message>(
    tree: &TreeIndex,
    actions: &ActionIndex<Message>,
) -> Result<(), BindingConflict> {
    if !actions.has_actions() {
        return Ok(());
    }
    for owner in actions
        .records
        .iter()
        .filter(|record| !record.actions.is_empty())
    {
        let route = tree.raw_route(Some(&owner.id));
        let scopes = scopes_for_route(actions, &route);
        resolve_actions(&owner.id, &owner.descriptors, &scopes)?;
    }
    Ok(())
}

pub(crate) struct ResolvedActionRoute<Message> {
    route: Vec<NodeId>,
    focus_owner: Option<NodeId>,
    groups: Vec<ResolvedRouteGroups<Message>>,
}

impl<Message> Default for ResolvedActionRoute<Message> {
    fn default() -> Self {
        Self {
            route: Vec::new(),
            focus_owner: None,
            groups: Vec::new(),
        }
    }
}

impl<Message> ResolvedActionRoute<Message> {
    pub(crate) fn resolve_into(
        &mut self,
        route: &[NodeId],
        actions: &ActionIndex<Message>,
        tree: &TreeIndex,
        focus_owner: Option<&NodeId>,
    ) -> Result<(), BindingConflict> {
        self.route.clear();
        self.route.extend_from_slice(route);
        self.resolve_groups(actions, tree, focus_owner)
    }

    pub(crate) fn resolve_tree_route_into(
        &mut self,
        target: Option<&NodeId>,
        actions: &ActionIndex<Message>,
        tree: &TreeIndex,
        focus_owner: Option<&NodeId>,
    ) -> Result<bool, BindingConflict> {
        tree.route_into(target, &mut self.route);
        if !route_needs_action_resolution(&self.route, actions, tree, focus_owner) {
            self.focus_owner = None;
            self.groups.clear();
            return Ok(false);
        }
        self.resolve_groups(actions, tree, focus_owner)?;
        Ok(true)
    }

    fn resolve_groups(
        &mut self,
        actions: &ActionIndex<Message>,
        tree: &TreeIndex,
        focus_owner: Option<&NodeId>,
    ) -> Result<(), BindingConflict> {
        let scopes = scopes_for_route(actions, &self.route);
        let allowed = allowed_route_len(&self.route, actions);
        self.groups.clear();
        self.groups
            .resize_with(self.route.len(), ResolvedRouteGroups::default);

        // Action records follow semantic tree order, which also fixes which
        // active-route conflict is returned when multiple groups are invalid
        for owner in actions
            .records
            .iter()
            .filter(|record| !record.actions.is_empty())
        {
            let Some(route_index) = self.route[..allowed].iter().position(|id| id == &owner.id)
            else {
                continue;
            };
            let resolved = resolve_actions(&owner.id, &owner.descriptors, &scopes)?;
            self.groups[route_index].declared = Some(ResolvedActionGroup {
                actions: owner.actions.clone(),
                resolved,
            });
        }

        for (route_index, owner) in self.route[..allowed].iter().enumerate() {
            let includes_focus = focus_owner == Some(owner);
            let scroll_axis = tree
                .record(owner)
                .and_then(|record| record.kind.scroll_axis());
            let Some(group) = core_action_group(includes_focus, scroll_axis) else {
                continue;
            };
            self.groups[route_index].core = Some(ResolvedCoreActionGroup {
                actions: group.actions,
                resolved: resolve_core_action_group(owner, &group, &scopes)?,
            });
        }
        self.focus_owner.clone_from(&focus_owner.cloned());
        Ok(())
    }

    pub(crate) fn matches_route(&self, route: &[NodeId], focus_owner: Option<&NodeId>) -> bool {
        self.route == route && self.focus_owner.as_ref() == focus_owner
    }

    pub(crate) fn match_declared_event(
        &self,
        route_index: usize,
        event: &Event,
    ) -> ActionMatch<Message> {
        let Some(group) = self
            .groups
            .get(route_index)
            .and_then(|groups| groups.declared.as_ref())
        else {
            return ActionMatch::None;
        };
        let Some(stroke) = KeyStroke::from_event(event) else {
            return ActionMatch::None;
        };
        for (action, resolved) in group.actions.iter().zip(group.resolved.actions()) {
            if !resolved
                .bindings()
                .iter()
                .any(|binding| binding.matches(event))
            {
                continue;
            }
            return match resolved.availability() {
                crate::ActionAvailability::Enabled => ActionMatch::Invoke {
                    action: action.clone(),
                    event: ActionEvent::new(resolved.id().clone(), stroke),
                },
                crate::ActionAvailability::DisabledConsume => ActionMatch::Consume,
                crate::ActionAvailability::DisabledPassThrough => continue,
            };
        }
        ActionMatch::None
    }

    pub(crate) fn match_core_event(&self, route_index: usize, event: &Event) -> CoreActionMatch {
        let Some(group) = self
            .groups
            .get(route_index)
            .and_then(|groups| groups.core.as_ref())
        else {
            return CoreActionMatch::None;
        };
        for (action, resolved) in group.actions.iter().zip(group.resolved.actions()) {
            if !resolved
                .bindings()
                .iter()
                .any(|binding| binding.matches(event))
            {
                continue;
            }
            return match resolved.availability() {
                crate::ActionAvailability::Enabled => CoreActionMatch::Invoke(*action),
                crate::ActionAvailability::DisabledConsume => CoreActionMatch::Consume,
                crate::ActionAvailability::DisabledPassThrough => continue,
            };
        }
        CoreActionMatch::None
    }

    pub(crate) fn groups(&self) -> Vec<ResolvedActions> {
        let mut resolved = Vec::with_capacity(self.groups.len().saturating_mul(2));
        for groups in &self.groups {
            if let Some(group) = &groups.declared {
                resolved.push(group.resolved.clone());
            }
            if let Some(group) = &groups.core {
                resolved.push(group.resolved.clone());
            }
        }
        resolved
    }
}

pub(crate) fn route_needs_action_resolution<Message>(
    route: &[NodeId],
    actions: &ActionIndex<Message>,
    tree: &TreeIndex,
    focus_owner: Option<&NodeId>,
) -> bool {
    if actions.has_actions() {
        return true;
    }
    route[..allowed_route_len(route, actions)].iter().any(|id| {
        focus_owner == Some(id)
            || tree
                .record(id)
                .is_some_and(|record| record.kind.is_scroll_viewport())
    })
}

struct ResolvedRouteGroups<Message> {
    declared: Option<ResolvedActionGroup<Message>>,
    core: Option<ResolvedCoreActionGroup>,
}

impl<Message> Default for ResolvedRouteGroups<Message> {
    fn default() -> Self {
        Self {
            declared: None,
            core: None,
        }
    }
}

struct ResolvedActionGroup<Message> {
    actions: Rc<[Action<Message>]>,
    resolved: ResolvedActions,
}

struct ResolvedCoreActionGroup {
    actions: &'static [CoreAction],
    resolved: ResolvedActions,
}

pub(crate) enum ActionMatch<Message> {
    None,
    Consume,
    Invoke {
        action: Action<Message>,
        event: ActionEvent,
    },
}

pub(crate) enum CoreActionMatch {
    None,
    Consume,
    Invoke(CoreAction),
}

fn allowed_route_len<Message>(route: &[NodeId], actions: &ActionIndex<Message>) -> usize {
    route
        .iter()
        .position(|id| {
            actions.record(id).is_some_and(|record| {
                record
                    .scope
                    .as_ref()
                    .is_some_and(|scope| scope.propagation == KeyScopePropagation::StopAtScope)
            })
        })
        .map_or(route.len(), |index| index + 1)
}

impl<Message> ActionMatch<Message> {
    pub(crate) fn invoke(self) -> Option<EventResult<Message>> {
        match self {
            Self::None => None,
            Self::Consume => Some(EventResult::consumed()),
            Self::Invoke { action, event } => Some(action.invoke(&event)),
        }
    }
}

fn scopes_for_route<Message>(
    actions: &ActionIndex<Message>,
    target_to_root: &[NodeId],
) -> Vec<KeyScope> {
    target_to_root
        .iter()
        .rev()
        .filter_map(|id| {
            let scope = actions.record(id)?.scope.as_ref()?;
            Some(
                KeyScope::new(id.clone(), scope.key_map.clone())
                    .with_propagation(scope.propagation),
            )
        })
        .collect()
}
