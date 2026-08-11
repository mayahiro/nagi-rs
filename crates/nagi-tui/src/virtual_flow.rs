use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::{Node, NodeId, ScrollState};

/// One stable item identity in a [`VirtualFlowItems`] order
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VirtualFlowItem {
    key: NodeId,
}

impl VirtualFlowItem {
    /// Creates an item with one stable key
    #[must_use]
    pub fn new(key: impl Into<NodeId>) -> Self {
        Self { key: key.into() }
    }

    /// Returns the stable item key
    #[must_use]
    pub const fn key(&self) -> &NodeId {
        &self.key
    }
}

/// Error returned when a virtual flow order contains the same key twice
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateVirtualFlowItemKey {
    key: NodeId,
}

impl DuplicateVirtualFlowItemKey {
    /// Returns the duplicated stable item key
    #[must_use]
    pub const fn key(&self) -> &NodeId {
        &self.key
    }
}

impl fmt::Display for DuplicateVirtualFlowItemKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate virtual flow item key {}", self.key)
    }
}

impl Error for DuplicateVirtualFlowItemKey {}

/// Immutable unique item order used by a variable-height virtual flow
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VirtualFlowItems {
    inner: Arc<[VirtualFlowItem]>,
}

impl VirtualFlowItems {
    /// Creates a validated immutable item order
    pub fn new(
        items: impl IntoIterator<Item = VirtualFlowItem>,
    ) -> Result<Self, DuplicateVirtualFlowItemKey> {
        let items: Vec<VirtualFlowItem> = items.into_iter().collect();
        let mut keys = HashSet::with_capacity(items.len());
        for item in &items {
            if !keys.insert(item.key.clone()) {
                return Err(DuplicateVirtualFlowItemKey {
                    key: item.key.clone(),
                });
            }
        }
        Ok(Self {
            inner: Arc::from(items),
        })
    }

    /// Returns the number of stable items
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Reports whether the item order is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns one item by current index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&VirtualFlowItem> {
        self.inner.get(index)
    }

    /// Returns the immutable ordered items
    #[must_use]
    pub fn as_slice(&self) -> &[VirtualFlowItem] {
        &self.inner
    }

    pub(crate) fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Application-declared invalidation for one virtual flow content revision
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualFlowUpdate {
    revision: u64,
    previous_revision: Option<u64>,
    changed: Range<usize>,
    reset: bool,
}

impl VirtualFlowUpdate {
    /// Invalidates every item for a new opaque content revision
    #[must_use]
    pub const fn reset(revision: u64) -> Self {
        Self {
            revision,
            previous_revision: None,
            changed: 0..usize::MAX,
            reset: true,
        }
    }

    /// Invalidates one current index range when the previous revision matches
    #[must_use]
    pub const fn changed(revision: u64, previous_revision: u64, changed: Range<usize>) -> Self {
        Self {
            revision,
            previous_revision: Some(previous_revision),
            changed,
            reset: false,
        }
    }

    /// Returns the current opaque content revision
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the required previous revision for a partial invalidation
    #[must_use]
    pub const fn previous_revision(&self) -> Option<u64> {
        self.previous_revision
    }

    /// Returns the current item index range that may require remeasurement
    #[must_use]
    pub fn changed_range(&self) -> Range<usize> {
        self.changed.clone()
    }

    pub(crate) const fn resets_all(&self) -> bool {
        self.reset
    }
}

impl Default for VirtualFlowUpdate {
    fn default() -> Self {
        Self::reset(0)
    }
}

/// Current item context supplied to virtual flow callbacks
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualFlowItemContext {
    index: usize,
    key: NodeId,
    width: u32,
}

impl VirtualFlowItemContext {
    pub(crate) fn new(index: usize, key: NodeId, width: u32) -> Self {
        Self { index, key, width }
    }

    /// Returns the current item index
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns the stable item key
    #[must_use]
    pub const fn key(&self) -> &NodeId {
        &self.key
    }

    /// Returns the flow width available to the item in terminal cells
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
}

type VirtualFlowEstimate = dyn Fn(&VirtualFlowItemContext) -> u32;
type VirtualFlowBuilder<Message> = dyn Fn(&VirtualFlowItemContext) -> Node<Message>;

/// Immutable item callbacks and update metadata consumed by a virtual flow
pub struct VirtualFlowSource<Message> {
    items: VirtualFlowItems,
    update: VirtualFlowUpdate,
    estimate: Arc<VirtualFlowEstimate>,
    builder: Arc<VirtualFlowBuilder<Message>>,
}

impl<Message> Clone for VirtualFlowSource<Message> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            update: self.update.clone(),
            estimate: Arc::clone(&self.estimate),
            builder: Arc::clone(&self.builder),
        }
    }
}

impl<Message> VirtualFlowSource<Message> {
    /// Creates a source with one-cell estimates and revision zero
    #[must_use]
    pub fn new(
        items: VirtualFlowItems,
        builder: impl Fn(&VirtualFlowItemContext) -> Node<Message> + 'static,
    ) -> Self {
        Self {
            items,
            update: VirtualFlowUpdate::default(),
            estimate: Arc::new(|_| 1),
            builder: Arc::new(builder),
        }
    }

    /// Sets the current content invalidation metadata
    #[must_use]
    pub fn update(mut self, update: VirtualFlowUpdate) -> Self {
        self.update = update;
        self
    }

    /// Sets the width-aware estimated item height callback
    #[must_use]
    pub fn estimated_height(
        mut self,
        estimate: impl Fn(&VirtualFlowItemContext) -> u32 + 'static,
    ) -> Self {
        self.estimate = Arc::new(estimate);
        self
    }

    /// Returns the immutable stable item order
    #[must_use]
    pub const fn items(&self) -> &VirtualFlowItems {
        &self.items
    }

    /// Returns the current content invalidation metadata
    #[must_use]
    pub const fn current_update(&self) -> &VirtualFlowUpdate {
        &self.update
    }

    pub(crate) fn context(&self, index: usize, width: u32) -> VirtualFlowItemContext {
        VirtualFlowItemContext::new(
            index,
            self.items
                .get(index)
                .expect("virtual flow index belongs to the source")
                .key
                .clone(),
            width,
        )
    }

    pub(crate) fn estimate(&self, index: usize, width: u32) -> u32 {
        (self.estimate)(&self.context(index, width)).max(1)
    }

    pub(crate) fn build(&self, index: usize, width: u32) -> Node<Message> {
        (self.builder)(&self.context(index, width))
    }
}

/// Behavior of a vertical variable-height virtual flow
pub struct VirtualFlowOptions<Message> {
    /// Extra terminal cells built before and after the visible range
    pub overscan: u32,
    /// Whether a viewport at the end follows content growth
    pub stick_to_end: bool,
    /// Whether focus movement scrolls a built descendant into view
    pub ensure_focused_visible: bool,
    /// Optional application message created after user scrolling changes state
    pub on_scroll: Option<Box<dyn Fn(ScrollState) -> Message>>,
}

impl<Message> Default for VirtualFlowOptions<Message> {
    fn default() -> Self {
        Self {
            overscan: 1,
            stick_to_end: false,
            ensure_focused_visible: false,
            on_scroll: None,
        }
    }
}

/// Semantic edge represented by a [`VirtualFlowAnchor`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VirtualFlowAnchorAffinity {
    /// The viewport start is positioned inside an item
    Start,
    /// The viewport end is positioned relative to an item end
    End,
}

/// Stable item and intra-item cell position retained by a virtual flow
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualFlowAnchor {
    key: NodeId,
    offset: u32,
    affinity: VirtualFlowAnchorAffinity,
}

impl VirtualFlowAnchor {
    /// Returns the stable anchor item key
    #[must_use]
    pub const fn key(&self) -> &NodeId {
        &self.key
    }

    /// Returns the intra-item cell distance from the semantic edge
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// Returns the viewport edge affinity
    #[must_use]
    pub const fn affinity(&self) -> VirtualFlowAnchorAffinity {
        self.affinity
    }
}

/// Resolved scroll and semantic position of one virtual flow
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualFlowState {
    scroll: ScrollState,
    anchor: Option<VirtualFlowAnchor>,
    visible: Range<usize>,
    item_count: usize,
}

impl VirtualFlowState {
    /// Returns the underlying resolved vertical scroll state
    #[must_use]
    pub const fn scroll(&self) -> ScrollState {
        self.scroll
    }

    /// Returns the retained stable semantic anchor
    #[must_use]
    pub const fn anchor(&self) -> Option<&VirtualFlowAnchor> {
        self.anchor.as_ref()
    }

    /// Returns the visible current item index range without overscan
    #[must_use]
    pub fn visible_range(&self) -> Range<usize> {
        self.visible.clone()
    }

    /// Returns the current number of stable items
    #[must_use]
    pub const fn item_count(&self) -> usize {
        self.item_count
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VirtualFlowInteraction {
    items: VirtualFlowItems,
    update_revision: Option<u64>,
    width: u32,
    heights: HeightIndex,
    measured: Vec<bool>,
    positions: HashMap<NodeId, usize>,
    state: Option<VirtualFlowState>,
    generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreservedVirtualFlowAnchor {
    anchor: VirtualFlowAnchor,
    old_index: usize,
    old_items: VirtualFlowItems,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VirtualFlowWindow {
    pub(crate) offset: u32,
    pub(crate) content_height: u32,
    pub(crate) visible: Range<usize>,
    pub(crate) built: Range<usize>,
    pub(crate) generation: u64,
}

impl VirtualFlowInteraction {
    pub(crate) fn state(&self) -> Option<&VirtualFlowState> {
        self.state.as_ref()
    }

    pub(crate) fn total_height(&self) -> u32 {
        self.heights.total()
    }

    pub(crate) fn height(&self, index: usize) -> u32 {
        self.heights.height(index)
    }

    pub(crate) fn origin(&self, index: usize) -> u32 {
        self.heights.prefix(index)
    }

    pub(crate) fn is_measured(&self, index: usize) -> bool {
        self.measured.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn capture_anchor(
        &self,
        offset: u32,
        viewport_height: u32,
        following_end: bool,
    ) -> Option<PreservedVirtualFlowAnchor> {
        if self.items.is_empty() {
            return None;
        }
        let (index, anchor) = if following_end {
            let index = self.items.len().saturating_sub(1);
            (
                index,
                VirtualFlowAnchor {
                    key: self.items.get(index)?.key.clone(),
                    offset: self
                        .total_height()
                        .saturating_sub(offset.saturating_add(viewport_height)),
                    affinity: VirtualFlowAnchorAffinity::End,
                },
            )
        } else {
            let index = self.heights.item_at(offset)?;
            (
                index,
                VirtualFlowAnchor {
                    key: self.items.get(index)?.key.clone(),
                    offset: offset.saturating_sub(self.heights.prefix(index)),
                    affinity: VirtualFlowAnchorAffinity::Start,
                },
            )
        };
        Some(PreservedVirtualFlowAnchor {
            anchor,
            old_index: index,
            old_items: self.items.clone(),
        })
    }

    pub(crate) fn reconcile<Message>(
        &mut self,
        source: &VirtualFlowSource<Message>,
        width: u32,
    ) -> bool {
        let same_items = self.items.shares_storage(source.items());
        let width_changed = self.width != width;
        let mut changed = false;
        let mut estimates_current = false;

        if !same_items {
            let old = self
                .items
                .as_slice()
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    (
                        item.key.clone(),
                        (self.heights.height(index), self.is_measured(index)),
                    )
                })
                .collect::<HashMap<_, _>>();
            let mut heights = Vec::with_capacity(source.items().len());
            let mut measured = Vec::with_capacity(source.items().len());
            let mut reused = false;
            for (index, item) in source.items().as_slice().iter().enumerate() {
                if !width_changed {
                    if let Some(&(height, was_measured)) = old.get(item.key()) {
                        heights.push(height);
                        measured.push(was_measured);
                        reused = true;
                        continue;
                    }
                }
                heights.push(source.estimate(index, width));
                measured.push(false);
            }
            self.items = source.items().clone();
            self.heights = HeightIndex::new(heights);
            self.measured = measured;
            self.rebuild_positions();
            changed = true;
            estimates_current = !reused;
        } else if width_changed {
            self.reset_estimates(source, width, 0..source.items().len());
            changed = true;
            estimates_current = true;
        }

        if self.update_revision != Some(source.current_update().revision()) {
            let update = source.current_update();
            if !estimates_current {
                let partial = !update.resets_all()
                    && update.previous_revision() == self.update_revision
                    && !width_changed;
                let range = if partial {
                    update.changed_range()
                } else {
                    0..source.items().len()
                };
                changed |= self.reset_estimates(source, width, range);
            }
            self.update_revision = Some(update.revision());
        }

        self.width = width;
        if changed {
            self.generation = self.generation.wrapping_add(1);
        }
        changed
    }

    pub(crate) fn set_measured(&mut self, index: usize, height: u32) -> bool {
        if index >= self.measured.len() {
            return false;
        }
        let height = height.max(1);
        let changed = !self.measured[index] || self.heights.height(index) != height;
        self.measured[index] = true;
        if self.heights.update(index, height) {
            self.generation = self.generation.wrapping_add(1);
        }
        changed
    }

    pub(crate) fn resolve_anchor(
        &self,
        preserved: &PreservedVirtualFlowAnchor,
        viewport_height: u32,
    ) -> u32 {
        let index = self
            .positions
            .get(preserved.anchor.key())
            .copied()
            .or_else(|| {
                preserved.old_items.as_slice()[preserved.old_index.saturating_add(1)..]
                    .iter()
                    .find_map(|item| self.positions.get(item.key()).copied())
            })
            .or_else(|| {
                preserved.old_items.as_slice()[..preserved.old_index]
                    .iter()
                    .rev()
                    .find_map(|item| self.positions.get(item.key()).copied())
            });
        let Some(index) = index else {
            return 0;
        };
        match preserved.anchor.affinity {
            VirtualFlowAnchorAffinity::Start => self.heights.prefix(index).saturating_add(
                preserved
                    .anchor
                    .offset
                    .min(self.heights.height(index).saturating_sub(1)),
            ),
            VirtualFlowAnchorAffinity::End => self
                .heights
                .prefix(index.saturating_add(1))
                .saturating_sub(preserved.anchor.offset)
                .saturating_sub(viewport_height),
        }
    }

    pub(crate) fn resolve_window(
        &mut self,
        scroll: ScrollState,
        viewport_height: u32,
        overscan: u32,
        following_end: bool,
    ) -> VirtualFlowWindow {
        let offset = scroll.offset.y;
        let visible = self.heights.range(offset, viewport_height);
        let built_start = offset.saturating_sub(overscan);
        let built_end = offset
            .saturating_add(viewport_height)
            .saturating_add(overscan);
        let built_height = built_end.saturating_sub(built_start);
        let built = self.heights.range(built_start, built_height);
        let anchor = self
            .capture_anchor(offset, viewport_height, following_end)
            .map(|preserved| preserved.anchor);
        self.state = Some(VirtualFlowState {
            scroll,
            anchor,
            visible: visible.clone(),
            item_count: self.items.len(),
        });
        VirtualFlowWindow {
            offset,
            content_height: self.total_height(),
            visible,
            built,
            generation: self.generation,
        }
    }

    fn reset_estimates<Message>(
        &mut self,
        source: &VirtualFlowSource<Message>,
        width: u32,
        range: Range<usize>,
    ) -> bool {
        let start = range.start.min(source.items().len());
        let end = range.end.min(source.items().len()).max(start);
        let mut changed = false;
        for index in start..end {
            let estimate = source.estimate(index, width);
            changed |= self.heights.update(index, estimate) || self.measured[index];
            self.measured[index] = false;
        }
        changed
    }

    fn rebuild_positions(&mut self) {
        self.positions = HashMap::with_capacity(self.items.len());
        for (index, item) in self.items.as_slice().iter().enumerate() {
            self.positions.insert(item.key.clone(), index);
        }
    }
}

#[derive(Clone, Debug, Default)]
struct HeightIndex {
    heights: Vec<u32>,
    tree: Vec<u64>,
}

impl HeightIndex {
    fn new(heights: Vec<u32>) -> Self {
        let mut index = Self {
            tree: vec![0; heights.len().saturating_add(1)],
            heights,
        };
        for position in 0..index.heights.len() {
            index.add(position, u64::from(index.heights[position]));
        }
        index
    }

    fn height(&self, index: usize) -> u32 {
        self.heights.get(index).copied().unwrap_or(0)
    }

    fn update(&mut self, index: usize, height: u32) -> bool {
        let Some(previous) = self.heights.get_mut(index) else {
            return false;
        };
        if *previous == height {
            return false;
        }
        let old = *previous;
        *previous = height;
        if height > old {
            self.add(index, u64::from(height - old));
        } else {
            self.subtract(index, u64::from(old - height));
        }
        true
    }

    fn total(&self) -> u32 {
        clamp_u64_to_u32(self.sum(self.heights.len()))
    }

    fn prefix(&self, end: usize) -> u32 {
        clamp_u64_to_u32(self.sum(end.min(self.heights.len())))
    }

    fn item_at(&self, offset: u32) -> Option<usize> {
        if self.heights.is_empty() || u64::from(offset) >= self.sum(self.heights.len()) {
            return None;
        }
        let target = u64::from(offset);
        let mut position = 0_usize;
        let mut accumulated = 0_u64;
        let mut step = 1_usize;
        while step < self.tree.len() {
            step <<= 1;
        }
        while step > 0 {
            let next = position.saturating_add(step);
            if next < self.tree.len() && accumulated.saturating_add(self.tree[next]) <= target {
                accumulated = accumulated.saturating_add(self.tree[next]);
                position = next;
            }
            step >>= 1;
        }
        Some(position.min(self.heights.len().saturating_sub(1)))
    }

    fn range(&self, offset: u32, extent: u32) -> Range<usize> {
        if extent == 0 || self.heights.is_empty() {
            return 0..0;
        }
        let total = self.total();
        if offset >= total {
            return self.heights.len()..self.heights.len();
        }
        let start = self.item_at(offset).unwrap_or(self.heights.len());
        let end_offset = offset.saturating_add(extent).min(total);
        let end = if end_offset == 0 {
            start
        } else {
            self.item_at(end_offset.saturating_sub(1))
                .map_or(self.heights.len(), |index| index.saturating_add(1))
        };
        start..end.max(start)
    }

    fn sum(&self, mut end: usize) -> u64 {
        let mut total = 0_u64;
        while end > 0 {
            total = total.saturating_add(self.tree[end]);
            end &= end - 1;
        }
        total
    }

    fn add(&mut self, index: usize, value: u64) {
        let mut position = index.saturating_add(1);
        while position < self.tree.len() {
            self.tree[position] = self.tree[position].saturating_add(value);
            position = position.saturating_add(position & position.wrapping_neg());
        }
    }

    fn subtract(&mut self, index: usize, value: u64) {
        let mut position = index.saturating_add(1);
        while position < self.tree.len() {
            self.tree[position] = self.tree[position].saturating_sub(value);
            position = position.saturating_add(position & position.wrapping_neg());
        }
    }
}

fn clamp_u64_to_u32(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{InteractionState, ScrollOffset};

    use super::*;

    #[test]
    fn transitions_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "interaction/virtual-flow.txt",
            "virtual-flow-transition",
            &[
                "initial",
                "next",
                "viewport",
                "initial-width",
                "next-width",
                "stick",
                "request",
                "overscan",
                "update",
                "expected-offset",
                "expected-maximum",
                "expected-at-end",
                "expected-anchor",
                "expected-visible",
                "expected-built",
            ],
        ) else {
            return;
        };
        for record in records {
            let initial = fixture_source(record.field("initial"), VirtualFlowUpdate::reset(1));
            let next = fixture_source(record.field("next"), fixture_update(record.field("update")));
            let viewport = fixture_u32(record.field("viewport"));
            let initial_width = fixture_u32(record.field("initial-width"));
            let next_width = fixture_u32(record.field("next-width"));
            let overscan = fixture_u32(record.field("overscan"));
            let stick_to_end = fixture_bool(record.field("stick"));
            let id = NodeId::from("flow");
            let mut interaction = InteractionState::new();

            interaction.prepare_virtual_flow(
                &id,
                &initial,
                initial_width,
                viewport,
                overscan,
                stick_to_end,
            );
            if record.field("request") != "-" {
                interaction.request_scroll(
                    &id,
                    ScrollOffset::new(0, fixture_u32(record.field("request"))),
                );
                interaction.prepare_virtual_flow(
                    &id,
                    &initial,
                    initial_width,
                    viewport,
                    overscan,
                    stick_to_end,
                );
            }

            let window = interaction.prepare_virtual_flow(
                &id,
                &next,
                next_width,
                viewport,
                overscan,
                stick_to_end,
            );
            let state = interaction
                .virtual_flow_state(&id)
                .expect("fixture flow state exists");
            assert_eq!(
                state.scroll().offset.y,
                fixture_u32(record.field("expected-offset")),
                "{} offset",
                record.id
            );
            assert_eq!(
                state.scroll().maximum.y,
                fixture_u32(record.field("expected-maximum")),
                "{} maximum",
                record.id
            );
            assert_eq!(
                state.scroll().at_end,
                fixture_bool(record.field("expected-at-end")),
                "{} at-end",
                record.id
            );
            assert_eq!(
                state.visible_range(),
                fixture_range(record.field("expected-visible")),
                "{} visible",
                record.id
            );
            assert_eq!(
                window.built,
                fixture_range(record.field("expected-built")),
                "{} built",
                record.id
            );
            assert_fixture_anchor(&record.id, state.anchor(), record.field("expected-anchor"));
        }
    }

    fn fixture_source(value: &str, update: VirtualFlowUpdate) -> VirtualFlowSource<()> {
        let mut heights = HashMap::new();
        let mut items = Vec::new();
        if value != "-" {
            for entry in value.split(',') {
                let (key, height) = entry
                    .split_once(':')
                    .unwrap_or_else(|| panic!("invalid virtual flow item {entry}"));
                let key = NodeId::from(key);
                heights.insert(key.clone(), fixture_u32(height));
                items.push(VirtualFlowItem::new(key));
            }
        }
        let heights = Arc::new(heights);
        VirtualFlowSource::new(
            VirtualFlowItems::new(items).expect("fixture item keys are unique"),
            |_| Node::spacer(0, 1),
        )
        .estimated_height(move |context| {
            *heights
                .get(context.key())
                .expect("fixture item has an estimated height")
        })
        .update(update)
    }

    fn fixture_update(value: &str) -> VirtualFlowUpdate {
        let parts = value.split(':').collect::<Vec<_>>();
        match parts.as_slice() {
            ["same"] => VirtualFlowUpdate::changed(1, 1, 0..0),
            ["reset", revision] => VirtualFlowUpdate::reset(fixture_u64(revision)),
            ["changed", previous, revision, start, end] => VirtualFlowUpdate::changed(
                fixture_u64(revision),
                fixture_u64(previous),
                fixture_usize(start)..fixture_usize(end),
            ),
            _ => panic!("invalid virtual flow update {value}"),
        }
    }

    fn assert_fixture_anchor(case: &str, actual: Option<&VirtualFlowAnchor>, expected: &str) {
        if expected == "-" {
            assert!(actual.is_none(), "{case} anchor = {actual:?}, want none");
            return;
        }
        let parts = expected.split(':').collect::<Vec<_>>();
        let [key, affinity, offset] = parts.as_slice() else {
            panic!("invalid virtual flow anchor {expected}");
        };
        let affinity = match *affinity {
            "start" => VirtualFlowAnchorAffinity::Start,
            "end" => VirtualFlowAnchorAffinity::End,
            value => panic!("invalid virtual flow affinity {value}"),
        };
        let expected = VirtualFlowAnchor {
            key: NodeId::from(*key),
            offset: fixture_u32(offset),
            affinity,
        };
        assert_eq!(actual, Some(&expected), "{case} anchor");
    }

    fn fixture_range(value: &str) -> Range<usize> {
        let (start, end) = value
            .split_once(':')
            .unwrap_or_else(|| panic!("invalid virtual flow range {value}"));
        fixture_usize(start)..fixture_usize(end)
    }

    fn fixture_bool(value: &str) -> bool {
        match value {
            "0" => false,
            "1" => true,
            _ => panic!("invalid fixture boolean {value}"),
        }
    }

    fn fixture_u32(value: &str) -> u32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid fixture u32 {value}: {error}"))
    }

    fn fixture_u64(value: &str) -> u64 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid fixture u64 {value}: {error}"))
    }

    fn fixture_usize(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid fixture usize {value}: {error}"))
    }

    #[test]
    fn height_index_updates_prefixes_and_ranges() {
        let mut index = HeightIndex::new(vec![2, 3, 1, 4]);
        assert_eq!(index.total(), 10);
        assert_eq!(index.prefix(2), 5);
        assert_eq!(index.item_at(0), Some(0));
        assert_eq!(index.item_at(2), Some(1));
        assert_eq!(index.item_at(9), Some(3));
        assert_eq!(index.item_at(10), None);
        assert_eq!(index.range(2, 4), 1..3);
        assert!(index.update(1, 1));
        assert_eq!(index.total(), 8);
        assert_eq!(index.prefix(2), 3);
        assert_eq!(index.range(2, 4), 1..4);
    }

    #[test]
    fn estimates_are_scanned_once_per_structural_or_width_change() {
        let items = VirtualFlowItems::new([
            VirtualFlowItem::new("a"),
            VirtualFlowItem::new("b"),
            VirtualFlowItem::new("c"),
        ])
        .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let estimate_calls = Arc::clone(&calls);
        let source: VirtualFlowSource<()> = VirtualFlowSource::new(items, |_| Node::spacer(0, 1))
            .estimated_height(move |_| {
                estimate_calls.fetch_add(1, Ordering::Relaxed);
                1
            });
        let mut flow = VirtualFlowInteraction::default();

        assert!(flow.reconcile(&source, 8));
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert!(!flow.reconcile(&source, 8));
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert!(flow.reconcile(&source, 4));
        assert_eq!(calls.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn item_orders_reject_duplicates_and_share_storage() {
        let items =
            VirtualFlowItems::new([VirtualFlowItem::new("a"), VirtualFlowItem::new("b")]).unwrap();
        let clone = items.clone();
        assert!(items.shares_storage(&clone));
        let error =
            VirtualFlowItems::new([VirtualFlowItem::new("same"), VirtualFlowItem::new("same")])
                .unwrap_err();
        assert_eq!(error.key().as_str(), "same");
    }

    #[test]
    fn order_shrink_releases_removed_position_capacity() {
        let large = fixture_source(
            &(0..1024)
                .map(|index| format!("item-{index}:1"))
                .collect::<Vec<_>>()
                .join(","),
            VirtualFlowUpdate::reset(1),
        );
        let small = fixture_source("remaining:1", VirtualFlowUpdate::reset(2));
        let mut flow = VirtualFlowInteraction::default();

        flow.reconcile(&large, 8);
        let large_capacity = flow.positions.capacity();
        flow.reconcile(&small, 8);

        assert!(flow.positions.capacity() < large_capacity);
    }
}
