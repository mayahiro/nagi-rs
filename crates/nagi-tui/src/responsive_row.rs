use nagi_surface::Rect;

use crate::Node;

/// Horizontal region used by one responsive-row item
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ResponsiveRowPlacement {
    /// Pack the item from the left edge
    #[default]
    Start,
    /// Center the item between retained start and end groups
    Center,
    /// Pack the item from the right edge
    End,
}

/// Layout options for a responsive row
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ResponsiveRowOptions {
    /// Empty Cells required between retained items
    pub gap: u32,
    /// Exact row height, or zero to use the greatest item height
    pub height: u32,
}

/// One eager child supplied to a responsive row
pub struct ResponsiveRowItem<Message> {
    pub(crate) node: Node<Message>,
    pub(crate) placement: ResponsiveRowPlacement,
    pub(crate) priority: u16,
}

impl<Message> ResponsiveRowItem<Message> {
    /// Creates a start-aligned item with priority zero
    #[must_use]
    pub const fn new(node: Node<Message>) -> Self {
        Self {
            node,
            placement: ResponsiveRowPlacement::Start,
            priority: 0,
        }
    }

    /// Sets the horizontal region used by the item
    #[must_use]
    pub const fn placement(mut self, placement: ResponsiveRowPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the retention priority used under insufficient width
    #[must_use]
    pub const fn priority(mut self, priority: u16) -> Self {
        self.priority = priority;
        self
    }

    /// Returns the configured horizontal region
    #[must_use]
    pub const fn configured_placement(&self) -> ResponsiveRowPlacement {
        self.placement
    }

    /// Returns the configured retention priority
    #[must_use]
    pub const fn configured_priority(&self) -> u16 {
        self.priority
    }
}

pub(crate) const INLINE_RESPONSIVE_ROW_ITEMS: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResponsiveRowMetric {
    pub(crate) placement: ResponsiveRowPlacement,
    pub(crate) priority: u16,
    pub(crate) desired_width: u32,
}

pub(crate) struct ResolvedResponsiveRowLayout {
    inline: [Option<Rect>; INLINE_RESPONSIVE_ROW_ITEMS],
    overflow: Vec<Option<Rect>>,
    len: usize,
}

impl ResolvedResponsiveRowLayout {
    fn new(len: usize) -> Self {
        Self {
            inline: [None; INLINE_RESPONSIVE_ROW_ITEMS],
            overflow: if len > INLINE_RESPONSIVE_ROW_ITEMS {
                vec![None; len]
            } else {
                Vec::new()
            },
            len,
        }
    }

    pub(crate) fn as_slice(&self) -> &[Option<Rect>] {
        if self.overflow.is_empty() {
            &self.inline[..self.len]
        } else {
            &self.overflow
        }
    }

    fn get(&self, index: usize) -> Option<Rect> {
        self.as_slice()[index]
    }

    fn set(&mut self, index: usize, value: Option<Rect>) {
        if self.overflow.is_empty() {
            self.inline[index] = value;
        } else {
            self.overflow[index] = value;
        }
    }
}

pub(crate) fn resolve_responsive_row_layout(
    rect: Rect,
    metrics: &[ResponsiveRowMetric],
    gap: u32,
) -> ResolvedResponsiveRowLayout {
    let mut layout = ResolvedResponsiveRowLayout::new(metrics.len());
    if rect.width == 0 || metrics.is_empty() {
        return layout;
    }

    let mut inline_order = [0_usize; INLINE_RESPONSIVE_ROW_ITEMS];
    let mut overflow_order = if metrics.len() > INLINE_RESPONSIVE_ROW_ITEMS {
        (0..metrics.len()).collect()
    } else {
        Vec::new()
    };
    let priority_order = if overflow_order.is_empty() {
        for (index, value) in inline_order[..metrics.len()].iter_mut().enumerate() {
            *value = index;
        }
        &mut inline_order[..metrics.len()]
    } else {
        overflow_order.as_mut_slice()
    };
    priority_order.sort_by(|left, right| {
        metrics[*right]
            .priority
            .cmp(&metrics[*left].priority)
            .then_with(|| left.cmp(right))
    });

    let mut remaining = rect.width;
    let mut retained = 0_u32;
    for &index in priority_order.iter() {
        let desired = metrics[index].desired_width.max(1);
        if retained == 0 {
            let allocated = desired.min(remaining);
            layout.set(index, Some(Rect::new(0, 0, allocated, rect.height)));
            remaining = remaining.saturating_sub(allocated);
            retained = 1;
            continue;
        }
        let required = gap.saturating_add(desired);
        if required <= remaining {
            layout.set(index, Some(Rect::new(0, 0, desired, rect.height)));
            remaining -= required;
            retained = retained.saturating_add(1);
        }
    }

    let (start_width, start_count) =
        group_width(metrics, &layout, ResponsiveRowPlacement::Start, gap);
    let (center_width, center_count) =
        group_width(metrics, &layout, ResponsiveRowPlacement::Center, gap);
    let (end_width, end_count) = group_width(metrics, &layout, ResponsiveRowPlacement::End, gap);

    place_group(
        rect,
        metrics,
        ResponsiveRowPlacement::Start,
        gap,
        0,
        &mut layout,
    );

    let end_offset = rect.width.saturating_sub(end_width);
    place_group(
        rect,
        metrics,
        ResponsiveRowPlacement::End,
        gap,
        end_offset,
        &mut layout,
    );

    if center_count != 0 {
        let ideal = rect.width.saturating_sub(center_width) / 2;
        let minimum = start_width.saturating_add(if start_count == 0 { 0 } else { gap });
        let maximum = end_offset
            .saturating_sub(if end_count == 0 { 0 } else { gap })
            .saturating_sub(center_width);
        let offset = if minimum <= maximum {
            ideal.clamp(minimum, maximum)
        } else {
            minimum.min(rect.width.saturating_sub(center_width))
        };
        place_group(
            rect,
            metrics,
            ResponsiveRowPlacement::Center,
            gap,
            offset,
            &mut layout,
        );
    }

    layout
}

fn group_width(
    metrics: &[ResponsiveRowMetric],
    layout: &ResolvedResponsiveRowLayout,
    placement: ResponsiveRowPlacement,
    gap: u32,
) -> (u32, usize) {
    let mut width = 0_u32;
    let mut count = 0_usize;
    for (index, metric) in metrics.iter().enumerate() {
        let Some(item_rect) = layout.get(index) else {
            continue;
        };
        if metric.placement != placement {
            continue;
        }
        if count != 0 {
            width = width.saturating_add(gap);
        }
        width = width.saturating_add(item_rect.width);
        count += 1;
    }
    (width, count)
}

fn place_group(
    parent: Rect,
    metrics: &[ResponsiveRowMetric],
    placement: ResponsiveRowPlacement,
    gap: u32,
    mut offset: u32,
    layout: &mut ResolvedResponsiveRowLayout,
) {
    let mut positioned = 0_usize;
    for (index, metric) in metrics.iter().enumerate() {
        let Some(item_rect) = layout.get(index) else {
            continue;
        };
        if metric.placement != placement {
            continue;
        }
        if positioned != 0 {
            offset = offset.saturating_add(gap);
        }
        let x = (i64::from(parent.x) + i64::from(offset))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        layout.set(
            index,
            Some(Rect::new(x, parent.y, item_rect.width, parent.height)),
        );
        offset = offset.saturating_add(item_rect.width);
        positioned += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "layout/responsive-row.txt",
            "responsive-row-layout",
            &[
                "rect",
                "gap",
                "placements",
                "priorities",
                "widths",
                "expected",
            ],
        ) else {
            return;
        };

        for record in records {
            let placements = fixture_list(record.field("placements"), fixture_placement);
            let priorities = fixture_list(record.field("priorities"), |value| {
                value.parse::<u16>().expect("priority")
            });
            let widths = fixture_list(record.field("widths"), |value| {
                value.parse::<u32>().expect("width")
            });
            let expected = fixture_list(record.field("expected"), |value| {
                (value != "none").then(|| fixture_rect(value))
            });
            assert_eq!(placements.len(), priorities.len(), "case {}", record.id);
            assert_eq!(placements.len(), widths.len(), "case {}", record.id);
            let metrics: Vec<_> = placements
                .into_iter()
                .zip(priorities)
                .zip(widths)
                .map(
                    |((placement, priority), desired_width)| ResponsiveRowMetric {
                        placement,
                        priority,
                        desired_width,
                    },
                )
                .collect();
            let actual = resolve_responsive_row_layout(
                fixture_rect(record.field("rect")),
                &metrics,
                record.field("gap").parse().expect("gap"),
            );
            assert_eq!(actual.as_slice(), expected, "case {}", record.id);
        }
    }

    #[test]
    fn higher_priority_is_retained_and_regions_use_edges() {
        let metrics = [
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::Start,
                priority: 2,
                desired_width: 3,
            },
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::Center,
                priority: 0,
                desired_width: 4,
            },
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::End,
                priority: 1,
                desired_width: 2,
            },
        ];

        let layout = resolve_responsive_row_layout(Rect::new(4, 2, 8, 1), &metrics, 1);

        assert_eq!(layout.as_slice()[0], Some(Rect::new(4, 2, 3, 1)));
        assert_eq!(layout.as_slice()[1], None);
        assert_eq!(layout.as_slice()[2], Some(Rect::new(10, 2, 2, 1)));
    }

    #[test]
    fn too_wide_lower_priority_item_does_not_block_a_narrower_item() {
        let metrics = [
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::Start,
                priority: 3,
                desired_width: 3,
            },
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::Start,
                priority: 2,
                desired_width: 9,
            },
            ResponsiveRowMetric {
                placement: ResponsiveRowPlacement::Start,
                priority: 1,
                desired_width: 2,
            },
        ];

        let layout = resolve_responsive_row_layout(Rect::new(0, 0, 6, 1), &metrics, 1);

        assert!(layout.as_slice()[0].is_some());
        assert!(layout.as_slice()[1].is_none());
        assert!(layout.as_slice()[2].is_some());
    }

    #[test]
    fn overflow_storage_keeps_every_item() {
        let metrics = vec![
            ResponsiveRowMetric {
                desired_width: 1,
                ..ResponsiveRowMetric::default()
            };
            INLINE_RESPONSIVE_ROW_ITEMS + 8
        ];
        let layout =
            resolve_responsive_row_layout(Rect::new(0, 0, metrics.len() as u32, 1), &metrics, 0);

        assert_eq!(layout.as_slice().len(), metrics.len());
        for (index, item_rect) in layout.as_slice().iter().enumerate() {
            assert_eq!(
                *item_rect,
                Some(Rect::new(index as i32, 0, 1, 1)),
                "item {index}"
            );
        }
    }

    fn fixture_list<T>(value: &str, parse: impl Fn(&str) -> T) -> Vec<T> {
        value.split(',').map(parse).collect()
    }

    fn fixture_placement(value: &str) -> ResponsiveRowPlacement {
        match value {
            "start" => ResponsiveRowPlacement::Start,
            "center" => ResponsiveRowPlacement::Center,
            "end" => ResponsiveRowPlacement::End,
            _ => panic!("invalid placement {value}"),
        }
    }

    fn fixture_rect(value: &str) -> Rect {
        let values: Vec<_> = value.split(':').collect();
        let [x, y, width, height] = values.as_slice() else {
            panic!("invalid rect {value}");
        };
        Rect::new(
            x.parse().expect("x"),
            y.parse().expect("y"),
            width.parse().expect("width"),
            height.parse().expect("height"),
        )
    }
}
