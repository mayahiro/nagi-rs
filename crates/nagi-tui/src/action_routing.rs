use std::collections::HashMap;
use std::rc::Rc;

use crate::keymap::{Action, ActionEvent};
use crate::routing::TreeIndex;
use crate::{
    ActionDescriptor, BindingConflict, Event, EventResult, KeyMap, KeyScope, KeyScopePropagation,
    KeyStroke, NodeId, ResolvedActions, resolve_actions,
};

pub(crate) struct NodeKeyInteraction<Message> {
    actions: Rc<[Action<Message>]>,
    scope: Option<NodeKeyScope>,
}

impl<Message> NodeKeyInteraction<Message> {
    pub(crate) fn new() -> Self {
        Self {
            actions: Rc::from([]),
            scope: None,
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
    groups: Vec<Option<ResolvedActionGroup<Message>>>,
}

impl<Message> ResolvedActionRoute<Message> {
    pub(crate) fn resolve(
        route: &[NodeId],
        actions: &ActionIndex<Message>,
    ) -> Result<Self, BindingConflict> {
        let scopes = scopes_for_route(actions, route);
        let stop = route.iter().position(|id| {
            actions.record(id).is_some_and(|record| {
                record
                    .scope
                    .as_ref()
                    .is_some_and(|scope| scope.propagation == KeyScopePropagation::StopAtScope)
            })
        });
        let allowed = stop.map_or(route.len(), |index| index + 1);
        let mut groups: Vec<Option<ResolvedActionGroup<Message>>> =
            std::iter::repeat_with(|| None).take(route.len()).collect();

        // Action records follow semantic tree order, which also fixes which
        // active-route conflict is returned when multiple groups are invalid
        for owner in actions
            .records
            .iter()
            .filter(|record| !record.actions.is_empty())
        {
            let Some(route_index) = route[..allowed].iter().position(|id| id == &owner.id) else {
                continue;
            };
            let resolved = resolve_actions(&owner.id, &owner.descriptors, &scopes)?;
            groups[route_index] = Some(ResolvedActionGroup {
                actions: owner.actions.clone(),
                resolved,
            });
        }
        Ok(Self {
            route: route.to_vec(),
            groups,
        })
    }

    pub(crate) fn matches_route(&self, route: &[NodeId]) -> bool {
        self.route == route
    }

    pub(crate) fn match_event(&self, route_index: usize, event: &Event) -> ActionMatch<Message> {
        let Some(group) = self.groups.get(route_index).and_then(Option::as_ref) else {
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

    pub(crate) fn groups(&self) -> Vec<ResolvedActions> {
        self.groups
            .iter()
            .filter_map(|group| group.as_ref().map(|group| group.resolved.clone()))
            .collect()
    }
}

struct ResolvedActionGroup<Message> {
    actions: Rc<[Action<Message>]>,
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
