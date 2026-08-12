use nagi_surface::Rect;
use nagi_vt::Style;

/// Main-axis direction used by a split-pane layout
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SplitPaneAxis {
    /// Place the primary pane before the secondary pane from left to right
    #[default]
    Horizontal,
    /// Place the primary pane before the secondary pane from top to bottom
    Vertical,
}

/// Pane omitted when the assigned main-axis extent cannot satisfy both minima
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SplitPaneCollapse {
    /// Omit the primary pane and preserve the secondary pane
    Primary,
    /// Omit the secondary pane and preserve the primary pane
    #[default]
    Secondary,
}

/// Layout policy used by the Core split-pane node
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SplitPaneOptions {
    /// Direction in which the panes are arranged
    pub axis: SplitPaneAxis,
    /// Primary-pane share in basis points, clamped to 0 through 10,000
    pub ratio: u16,
    /// Smallest expanded primary-pane extent, normalized to at least one Cell
    pub primary_minimum: u32,
    /// Smallest expanded secondary-pane extent, normalized to at least one Cell
    pub secondary_minimum: u32,
    /// Pane omitted when the assigned extent is below both minima plus divider
    pub collapse: SplitPaneCollapse,
    /// Style used by the one-Cell divider
    pub divider_style: Style,
}

impl Default for SplitPaneOptions {
    fn default() -> Self {
        Self {
            axis: SplitPaneAxis::Horizontal,
            ratio: 5_000,
            primary_minimum: 1,
            secondary_minimum: 1,
            collapse: SplitPaneCollapse::Secondary,
            divider_style: Style::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedSplitPaneLayout {
    pub(crate) primary: Rect,
    pub(crate) divider: Option<Rect>,
    pub(crate) secondary: Rect,
    pub(crate) collapsed: Option<SplitPaneCollapse>,
}

pub(crate) fn resolve_split_pane_layout(
    rect: Rect,
    options: SplitPaneOptions,
) -> ResolvedSplitPaneLayout {
    let main = match options.axis {
        SplitPaneAxis::Horizontal => rect.width,
        SplitPaneAxis::Vertical => rect.height,
    };
    let primary_minimum = options.primary_minimum.max(1);
    let secondary_minimum = options.secondary_minimum.max(1);
    let required = u64::from(primary_minimum) + 1 + u64::from(secondary_minimum);
    if u64::from(main) < required {
        return collapsed_layout(rect, options.axis, options.collapse);
    }

    let usable = main - 1;
    let ideal = u64::from(usable).saturating_mul(u64::from(options.ratio.min(10_000))) / 10_000;
    let primary_extent = u32::try_from(ideal)
        .unwrap_or(u32::MAX)
        .clamp(primary_minimum, usable - secondary_minimum);
    expanded_layout(rect, options.axis, primary_extent)
}

fn expanded_layout(
    rect: Rect,
    axis: SplitPaneAxis,
    primary_extent: u32,
) -> ResolvedSplitPaneLayout {
    match axis {
        SplitPaneAxis::Horizontal => {
            let divider_x = add_origin(rect.x, primary_extent);
            ResolvedSplitPaneLayout {
                primary: Rect::new(rect.x, rect.y, primary_extent, rect.height),
                divider: Some(Rect::new(divider_x, rect.y, 1, rect.height)),
                secondary: Rect::new(
                    add_origin(divider_x, 1),
                    rect.y,
                    rect.width.saturating_sub(primary_extent).saturating_sub(1),
                    rect.height,
                ),
                collapsed: None,
            }
        }
        SplitPaneAxis::Vertical => {
            let divider_y = add_origin(rect.y, primary_extent);
            ResolvedSplitPaneLayout {
                primary: Rect::new(rect.x, rect.y, rect.width, primary_extent),
                divider: Some(Rect::new(rect.x, divider_y, rect.width, 1)),
                secondary: Rect::new(
                    rect.x,
                    add_origin(divider_y, 1),
                    rect.width,
                    rect.height.saturating_sub(primary_extent).saturating_sub(1),
                ),
                collapsed: None,
            }
        }
    }
}

fn collapsed_layout(
    rect: Rect,
    axis: SplitPaneAxis,
    collapse: SplitPaneCollapse,
) -> ResolvedSplitPaneLayout {
    let empty_primary = match axis {
        SplitPaneAxis::Horizontal => Rect::new(rect.x, rect.y, 0, rect.height),
        SplitPaneAxis::Vertical => Rect::new(rect.x, rect.y, rect.width, 0),
    };
    let empty_secondary = match axis {
        SplitPaneAxis::Horizontal => {
            Rect::new(add_origin(rect.x, rect.width), rect.y, 0, rect.height)
        }
        SplitPaneAxis::Vertical => {
            Rect::new(rect.x, add_origin(rect.y, rect.height), rect.width, 0)
        }
    };
    match collapse {
        SplitPaneCollapse::Primary => ResolvedSplitPaneLayout {
            primary: empty_primary,
            divider: None,
            secondary: rect,
            collapsed: Some(SplitPaneCollapse::Primary),
        },
        SplitPaneCollapse::Secondary => ResolvedSplitPaneLayout {
            primary: rect,
            divider: None,
            secondary: empty_secondary,
            collapsed: Some(SplitPaneCollapse::Secondary),
        },
    }
}

fn add_origin(origin: i32, extent: u32) -> i32 {
    i64::from(origin)
        .saturating_add(i64::from(extent))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "layout/split-pane.txt",
            "split-pane-layout",
            &[
                "rect",
                "axis",
                "ratio",
                "primary-min",
                "secondary-min",
                "collapse",
                "expected-primary",
                "expected-divider",
                "expected-secondary",
                "expected-collapse",
            ],
        ) else {
            return;
        };

        for record in records {
            let options = SplitPaneOptions {
                axis: fixture_axis(record.field("axis")),
                ratio: fixture_number(record.field("ratio")) as u16,
                primary_minimum: fixture_number(record.field("primary-min")),
                secondary_minimum: fixture_number(record.field("secondary-min")),
                collapse: fixture_collapse(record.field("collapse")),
                ..SplitPaneOptions::default()
            };
            let actual = resolve_split_pane_layout(fixture_rect(record.field("rect")), options);
            assert_eq!(
                actual.primary,
                fixture_rect(record.field("expected-primary")),
                "case {} primary",
                record.id
            );
            assert_eq!(
                actual.divider,
                fixture_optional_rect(record.field("expected-divider")),
                "case {} divider",
                record.id
            );
            assert_eq!(
                actual.secondary,
                fixture_rect(record.field("expected-secondary")),
                "case {} secondary",
                record.id
            );
            assert_eq!(
                actual.collapsed,
                fixture_optional_collapse(record.field("expected-collapse")),
                "case {} collapse",
                record.id
            );
        }
    }

    #[test]
    fn expanded_layout_clamps_ratio_to_minima() {
        let options = SplitPaneOptions {
            ratio: 0,
            primary_minimum: 3,
            secondary_minimum: 2,
            ..SplitPaneOptions::default()
        };

        let layout = resolve_split_pane_layout(Rect::new(2, 4, 10, 3), options);

        assert_eq!(layout.primary, Rect::new(2, 4, 3, 3));
        assert_eq!(layout.divider, Some(Rect::new(5, 4, 1, 3)));
        assert_eq!(layout.secondary, Rect::new(6, 4, 6, 3));
        assert_eq!(layout.collapsed, None);
    }

    #[test]
    fn insufficient_extent_omits_configured_pane() {
        let options = SplitPaneOptions {
            axis: SplitPaneAxis::Vertical,
            primary_minimum: 2,
            secondary_minimum: 2,
            collapse: SplitPaneCollapse::Primary,
            ..SplitPaneOptions::default()
        };

        let layout = resolve_split_pane_layout(Rect::new(-1, 3, 8, 4), options);

        assert_eq!(layout.primary, Rect::new(-1, 3, 8, 0));
        assert_eq!(layout.divider, None);
        assert_eq!(layout.secondary, Rect::new(-1, 3, 8, 4));
        assert_eq!(layout.collapsed, Some(SplitPaneCollapse::Primary));
    }

    fn fixture_axis(value: &str) -> SplitPaneAxis {
        match value {
            "horizontal" => SplitPaneAxis::Horizontal,
            "vertical" => SplitPaneAxis::Vertical,
            _ => panic!("invalid split pane axis {value}"),
        }
    }

    fn fixture_collapse(value: &str) -> SplitPaneCollapse {
        match value {
            "primary" => SplitPaneCollapse::Primary,
            "secondary" => SplitPaneCollapse::Secondary,
            _ => panic!("invalid split pane collapse {value}"),
        }
    }

    fn fixture_optional_collapse(value: &str) -> Option<SplitPaneCollapse> {
        match value {
            "none" => None,
            _ => Some(fixture_collapse(value)),
        }
    }

    fn fixture_optional_rect(value: &str) -> Option<Rect> {
        (value != "none").then(|| fixture_rect(value))
    }

    fn fixture_rect(value: &str) -> Rect {
        let values: Vec<_> = value.split(':').collect();
        let [x, y, width, height] = values.as_slice() else {
            panic!("invalid split pane rect {value}");
        };
        Rect::new(
            x.parse().expect("rect x"),
            y.parse().expect("rect y"),
            width.parse().expect("rect width"),
            height.parse().expect("rect height"),
        )
    }

    fn fixture_number(value: &str) -> u32 {
        value.parse().expect("split pane number")
    }
}
