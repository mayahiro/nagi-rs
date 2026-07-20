use std::collections::BTreeSet;

use nagi_tui::NodeId;

use crate::TreeItem;

/// Application-owned selection and expanded branch identities for a tree
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TreeState {
    selected: usize,
    expanded: BTreeSet<NodeId>,
}

impl TreeState {
    /// Creates collapsed state with the supplied preorder selection
    #[must_use]
    pub const fn new(selected: usize) -> Self {
        Self {
            selected,
            expanded: BTreeSet::new(),
        }
    }

    /// Creates state initialized from item expansion flags
    #[must_use]
    pub fn from_items(items: &[TreeItem], selected: usize) -> Self {
        Self {
            selected,
            expanded: items
                .iter()
                .filter(|item| item.has_children() && item.expanded())
                .map(|item| item.id().clone())
                .collect(),
        }
    }

    /// Returns the application-owned original preorder selection index
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// Replaces the original preorder selection index
    pub const fn select(&mut self, index: usize) {
        self.selected = index;
    }

    /// Reports whether a stable branch identity is expanded
    #[must_use]
    pub fn is_expanded(&self, id: &NodeId) -> bool {
        self.expanded.contains(id)
    }

    /// Replaces expansion state for a stable branch identity
    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        if expanded {
            self.expanded.insert(id);
        } else {
            self.expanded.remove(&id);
        }
    }

    /// Reverses and returns expansion state for a stable branch identity
    pub fn toggle(&mut self, id: NodeId) -> bool {
        let expanded = !self.is_expanded(&id);
        self.set_expanded(id, expanded);
        expanded
    }

    /// Returns independent items using identity-based expansion state
    #[must_use]
    pub fn apply(&self, items: impl IntoIterator<Item = TreeItem>) -> Vec<TreeItem> {
        items
            .into_iter()
            .map(|item| {
                let expanded = self.is_expanded(item.id());
                item.with_expanded(expanded)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::TreeState;
    use crate::TreeItem;

    #[test]
    fn expansion_follows_identity_after_reordering() {
        let items = [
            TreeItem::branch("a", "A", 0, true),
            TreeItem::branch("b", "B", 0, false),
        ];
        let state = TreeState::from_items(&items, 0);
        let applied = state.apply([items[1].clone(), items[0].clone()]);
        assert!(!applied[0].expanded());
        assert!(applied[1].expanded());
    }
}
