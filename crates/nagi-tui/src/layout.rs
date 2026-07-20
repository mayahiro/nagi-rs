use crate::{Rect, Size};

#[cfg(test)]
use crate::fixture_support;

/// A main-axis sizing rule for a child node
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Length {
    /// Use the child's measured size
    #[default]
    Auto,
    /// Request an exact cell count
    Fixed(u32),
    /// Share cells left after non-flexible requests
    Flex(u32),
    /// Request a percentage of the parent, rounded down and capped at 100
    Percent(u32),
    /// Request a preferred size constrained by minimum and maximum values
    MinMax {
        /// Preferred lower bound while space remains
        min: u32,
        /// Preferred size before flexible growth
        preferred: u32,
        /// Preferred upper bound
        max: u32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Track {
    pub(crate) length: Length,
    pub(crate) desired: u32,
}

#[cfg(test)]
pub(crate) fn allocate(available: u32, tracks: &[Track]) -> Vec<u32> {
    let mut allocations = vec![0; tracks.len()];
    let mut minimums = vec![0; tracks.len()];
    allocate_into(available, tracks, &mut allocations, &mut minimums);
    allocations
}

pub(crate) fn allocate_into(
    available: u32,
    tracks: &[Track],
    allocations: &mut [u32],
    minimums: &mut [u32],
) {
    let mut total = 0_u64;

    for (index, track) in tracks.iter().enumerate() {
        let (base, minimum) = base_and_minimum(available, *track);
        allocations[index] = base;
        minimums[index] = minimum.min(base);
        total = total.saturating_add(u64::from(base));
    }

    if total > u64::from(available) {
        shrink_from_end(allocations, minimums, total - u64::from(available));
    }

    let used: u64 = allocations.iter().map(|value| u64::from(*value)).sum();
    let remaining = u64::from(available).saturating_sub(used);
    distribute_flex(allocations, tracks, remaining);
}

fn base_and_minimum(available: u32, track: Track) -> (u32, u32) {
    match track.length {
        Length::Auto => (track.desired, 0),
        Length::Fixed(value) => (value, value),
        Length::Flex(_) => (0, 0),
        Length::Percent(percent) => {
            let value = u64::from(available) * u64::from(percent.min(100)) / 100;
            (value as u32, 0)
        }
        Length::MinMax {
            min,
            preferred,
            max,
        } => {
            let lower = min.min(max);
            let value = preferred.clamp(lower, max.max(lower));
            (value, lower)
        }
    }
}

fn shrink_from_end(allocations: &mut [u32], minimums: &[u32], mut excess: u64) {
    for index in (0..allocations.len()).rev() {
        let reducible = allocations[index].saturating_sub(minimums[index]);
        let reduction = u64::from(reducible).min(excess) as u32;
        allocations[index] -= reduction;
        excess -= u64::from(reduction);
        if excess == 0 {
            return;
        }
    }

    for allocation in allocations.iter_mut().rev() {
        let reduction = u64::from(*allocation).min(excess) as u32;
        *allocation -= reduction;
        excess -= u64::from(reduction);
        if excess == 0 {
            return;
        }
    }
}

fn distribute_flex(allocations: &mut [u32], tracks: &[Track], remaining: u64) {
    if remaining == 0 {
        return;
    }
    let total_weight: u64 = tracks
        .iter()
        .filter_map(|track| match track.length {
            Length::Flex(weight) => Some(u64::from(weight)),
            _ => None,
        })
        .sum();
    if total_weight == 0 {
        return;
    }

    let mut assigned = 0_u64;
    for (allocation, track) in allocations.iter_mut().zip(tracks) {
        let Length::Flex(weight) = track.length else {
            continue;
        };
        let share = u128::from(remaining) * u128::from(weight) / u128::from(total_weight);
        let share = u64::try_from(share).unwrap_or(u64::MAX);
        *allocation = allocation.saturating_add(share.min(u64::from(u32::MAX)) as u32);
        assigned = assigned.saturating_add(share);
    }

    let mut remainder = remaining.saturating_sub(assigned);
    while remainder != 0 {
        let mut progressed = false;
        for (allocation, track) in allocations.iter_mut().zip(tracks) {
            if !matches!(track.length, Length::Flex(weight) if weight != 0) {
                continue;
            }
            if *allocation != u32::MAX {
                *allocation += 1;
                remainder -= 1;
                progressed = true;
            }
            if remainder == 0 {
                return;
            }
        }
        if !progressed {
            return;
        }
    }
}

pub(crate) fn horizontal_rect(parent: Rect, offset: u32, width: u32) -> Rect {
    Rect::new(
        saturating_coordinate(parent.x, offset),
        parent.y,
        width,
        parent.height,
    )
}

pub(crate) fn vertical_rect(parent: Rect, offset: u32, height: u32) -> Rect {
    Rect::new(
        parent.x,
        saturating_coordinate(parent.y, offset),
        parent.width,
        height,
    )
}

pub(crate) fn inset(rect: Rect, left: u32, top: u32, right: u32, bottom: u32) -> Rect {
    let horizontal = left.saturating_add(right);
    let vertical = top.saturating_add(bottom);
    Rect::new(
        saturating_coordinate(rect.x, left.min(rect.width)),
        saturating_coordinate(rect.y, top.min(rect.height)),
        rect.width.saturating_sub(horizontal),
        rect.height.saturating_sub(vertical),
    )
}

pub(crate) fn add_size(size: Size, horizontal: u32, vertical: u32) -> Size {
    Size::new(
        size.width.saturating_add(horizontal),
        size.height.saturating_add(vertical),
    )
}

fn saturating_coordinate(origin: i32, offset: u32) -> i32 {
    (i64::from(origin) + i64::from(offset)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_allocations_match_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "layout/linear.txt",
            "layout-linear",
            &["available", "lengths", "desired", "expected"],
        ) else {
            return;
        };

        for record in records {
            let lengths = list(record.field("lengths"), length);
            let desired = list(record.field("desired"), number);
            let expected = list(record.field("expected"), number);
            assert_eq!(lengths.len(), desired.len(), "case {}", record.id);
            let tracks: Vec<_> = lengths
                .into_iter()
                .zip(desired)
                .map(|(length, desired)| Track { length, desired })
                .collect();
            assert_eq!(
                allocate(number(record.field("available")), &tracks),
                expected,
                "case {}",
                record.id
            );
        }
    }

    fn list<T>(value: &str, parse: impl Fn(&str) -> T) -> Vec<T> {
        if value == "-" {
            Vec::new()
        } else {
            value.split(',').map(parse).collect()
        }
    }

    fn number(value: &str) -> u32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }

    fn length(value: &str) -> Length {
        let parts: Vec<_> = value.split(':').collect();
        match parts.as_slice() {
            ["auto"] => Length::Auto,
            ["fixed", value] => Length::Fixed(number(value)),
            ["flex", value] => Length::Flex(number(value)),
            ["percent", value] => Length::Percent(number(value)),
            ["minmax", min, preferred, max] => Length::MinMax {
                min: number(min),
                preferred: number(preferred),
                max: number(max),
            },
            _ => panic!("invalid length {value}"),
        }
    }
}
