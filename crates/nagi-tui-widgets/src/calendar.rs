use std::sync::Arc;

use nagi_tui::{
    Event, EventResult, HorizontalAlignment, KeyAction, KeyCode, Length, Node, NodeId, Style,
    VerticalAlignment,
};

use crate::event::is_activation_event;

/// A proleptic Gregorian date between years 1 and 9999
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CalendarDate {
    /// Gregorian year
    pub year: i32,
    /// One-based month
    pub month: u8,
    /// One-based day of month
    pub day: u8,
}

impl CalendarDate {
    /// Creates a date clamped into the supported Gregorian range
    #[must_use]
    pub fn new(year: i32, month: i32, day: i32) -> Self {
        normalize_date(Self {
            year,
            month: u8::try_from(month.clamp(0, i32::from(u8::MAX))).unwrap_or(u8::MAX),
            day: u8::try_from(day.clamp(0, i32::from(u8::MAX))).unwrap_or(u8::MAX),
        })
    }
}

/// First displayed weekday column
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CalendarWeekStart {
    /// Renders Monday in the first column
    #[default]
    Monday,
    /// Renders Sunday in the first column
    Sunday,
}

/// Visual styles used by a [`Calendar`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarStyle {
    /// Style used by the displayed year and month
    pub header: Style,
    /// Style used by weekday headings
    pub weekday: Style,
    /// Style used by days in the displayed month
    pub normal: Style,
    /// Style merged over Saturdays and Sundays
    pub weekend: Style,
    /// Style used by dates outside the displayed month
    pub adjacent: Style,
    /// Style merged over the application-selected date
    pub selected: Style,
    /// Style merged over the date that owns focus
    pub focused: Style,
    /// Style used when date changes are unavailable
    pub disabled: Style,
}

impl Default for CalendarStyle {
    fn default() -> Self {
        Self {
            header: Style {
                bold: true,
                ..Style::default()
            },
            weekday: Style {
                dim: true,
                ..Style::default()
            },
            normal: Style::default(),
            weekend: Style {
                dim: true,
                ..Style::default()
            },
            adjacent: Style {
                dim: true,
                ..Style::default()
            },
            selected: Style {
                reverse: true,
                ..Style::default()
            },
            focused: Style {
                underline: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A controlled month grid over proleptic Gregorian dates
pub struct Calendar<Message> {
    id: NodeId,
    year: i32,
    month: u8,
    selected: CalendarDate,
    week_start: CalendarWeekStart,
    show_adjacent: bool,
    enabled: bool,
    style: CalendarStyle,
    on_select: Arc<dyn Fn(CalendarDate) -> Message>,
}

impl<Message: 'static> Calendar<Message> {
    /// Creates an enabled controlled month grid
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        year: i32,
        month: i32,
        selected: CalendarDate,
        on_select: impl Fn(CalendarDate) -> Message + 'static,
    ) -> Self {
        let displayed = CalendarDate::new(year, month, 1);
        Self {
            id: id.into(),
            year: displayed.year,
            month: displayed.month,
            selected: normalize_date(selected),
            week_start: CalendarWeekStart::Monday,
            show_adjacent: false,
            enabled: true,
            style: CalendarStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Replaces the first displayed weekday column
    #[must_use]
    pub const fn week_start(mut self, start: CalendarWeekStart) -> Self {
        self.week_start = start;
        self
    }

    /// Sets whether dates outside the displayed month are shown
    #[must_use]
    pub const fn show_adjacent(mut self, show: bool) -> Self {
        self.show_adjacent = show;
        self
    }

    /// Sets whether the calendar can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the calendar styles
    #[must_use]
    pub const fn style(mut self, style: CalendarStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this calendar
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let first = CalendarDate::new(self.year, i32::from(self.month), 1);
        let active = active_date(first, self.selected);
        let header = Node::align(
            Node::styled_text(
                format!("{:04}-{:02}", first.year, first.month),
                self.style.header,
            ),
            HorizontalAlignment::Center,
            VerticalAlignment::Start,
        )
        .with_length(Length::Fixed(1));
        let weekday_nodes = weekday_labels(self.week_start).into_iter().map(|label| {
            Node::styled_text(format!("{label} "), self.style.weekday).with_length(Length::Fixed(3))
        });
        let mut rows = vec![
            header,
            Node::row(weekday_nodes).with_length(Length::Fixed(1)),
        ];
        let offset = month_offset(first, self.week_start);
        for week in 0..6 {
            let mut days = Vec::with_capacity(7);
            for weekday in 0..7 {
                let position = week * 7 + weekday;
                let date = add_days(first, position - offset);
                let in_month = date.year == first.year && date.month == first.month;
                if !in_month && !self.show_adjacent {
                    days.push(Node::text("   ").with_length(Length::Fixed(3)));
                    continue;
                }
                let is_selected = date == active;
                let mut style = if in_month {
                    self.style.normal
                } else {
                    self.style.adjacent
                };
                if matches!(weekday_of(date), 0 | 6) {
                    style = style.merged(self.style.weekend);
                }
                if is_selected {
                    style = style.merged(self.style.selected);
                }
                if !self.enabled {
                    style = self.style.disabled;
                }
                let day_id = date_id(&self.id, date);
                let day_node = Node::styled_text(format!("{:2} ", date.day), style)
                    .with_length(Length::Fixed(3));
                if !self.enabled {
                    days.push(day_node.with_id(day_id));
                    continue;
                }
                if is_selected {
                    let focus_id = self.id.clone();
                    let event_id = self.id.clone();
                    let on_select = Arc::clone(&self.on_select);
                    days.push(
                        Node::column([day_node.with_id(day_id)])
                            .with_length(Length::Fixed(3))
                            .focusable(self.id.clone())
                            .with_focused_style(self.style.focused)
                            .on_event(event_id, move |event| {
                                selected_event_result(event, first, active, &focus_id, &on_select)
                            }),
                    );
                    continue;
                }
                let focus_id = self.id.clone();
                let on_select = Arc::clone(&self.on_select);
                days.push(
                    day_node
                        .with_id(day_id.clone())
                        .on_event(day_id, move |event| {
                            if !is_activation_event(event) {
                                return EventResult::ignored();
                            }
                            EventResult::consumed()
                                .focus(focus_id.clone())
                                .emit(on_select(date))
                        }),
                );
            }
            rows.push(Node::row(days).with_length(Length::Fixed(1)));
        }
        let root = Node::column(rows);
        if self.enabled {
            root
        } else {
            root.with_id(self.id)
        }
    }
}

fn selected_event_result<Message>(
    event: &Event,
    displayed: CalendarDate,
    selected: CalendarDate,
    focus_id: &NodeId,
    on_select: &Arc<dyn Fn(CalendarDate) -> Message>,
) -> EventResult<Message> {
    if is_activation_event(event) {
        return EventResult::consumed().focus(focus_id.clone());
    }
    let Some(next) = date_for_event(displayed, selected, event) else {
        return EventResult::ignored();
    };
    let result = EventResult::consumed().focus(focus_id.clone());
    if next == selected {
        result
    } else {
        result.emit(on_select(next))
    }
}

fn normalize_date(date: CalendarDate) -> CalendarDate {
    let year = date.year.clamp(1, 9999);
    let month = date.month.clamp(1, 12);
    let day = date.day.clamp(1, days_in_month(year, month));
    CalendarDate { year, month, day }
}

fn active_date(displayed: CalendarDate, selected: CalendarDate) -> CalendarDate {
    let displayed = normalize_date(displayed);
    let selected = normalize_date(selected);
    if selected.year == displayed.year && selected.month == displayed.month {
        selected
    } else {
        CalendarDate {
            year: displayed.year,
            month: displayed.month,
            day: 1,
        }
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn add_days(mut date: CalendarDate, mut days: i32) -> CalendarDate {
    date = normalize_date(date);
    while days > 0 {
        let last = days_in_month(date.year, date.month);
        if date.year == 9999 && date.month == 12 && date.day == last {
            return date;
        }
        if date.day < last {
            date.day += 1;
        } else {
            date = add_months(
                CalendarDate {
                    year: date.year,
                    month: date.month,
                    day: 1,
                },
                1,
            );
        }
        days -= 1;
    }
    while days < 0 {
        if date.year == 1 && date.month == 1 && date.day == 1 {
            return date;
        }
        if date.day > 1 {
            date.day -= 1;
        } else {
            let mut previous = add_months(
                CalendarDate {
                    year: date.year,
                    month: date.month,
                    day: 1,
                },
                -1,
            );
            previous.day = days_in_month(previous.year, previous.month);
            date = previous;
        }
        days += 1;
    }
    date
}

fn add_months(date: CalendarDate, months: i32) -> CalendarDate {
    let date = normalize_date(date);
    let index = ((date.year - 1) * 12 + i32::from(date.month) - 1 + months).clamp(0, 9999 * 12 - 1);
    let year = index / 12 + 1;
    let month = u8::try_from(index % 12 + 1).unwrap_or(12);
    CalendarDate {
        year,
        month,
        day: date.day.min(days_in_month(year, month)),
    }
}

fn weekday_of(date: CalendarDate) -> i32 {
    let date = normalize_date(date);
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut year = date.year;
    if date.month < 3 {
        year -= 1;
    }
    (year + year / 4 - year / 100
        + year / 400
        + OFFSETS[usize::from(date.month - 1)]
        + i32::from(date.day))
        % 7
}

fn month_offset(first: CalendarDate, start: CalendarWeekStart) -> i32 {
    let weekday = weekday_of(first);
    if start == CalendarWeekStart::Sunday {
        weekday
    } else {
        (weekday + 6) % 7
    }
}

fn weekday_labels(start: CalendarWeekStart) -> [&'static str; 7] {
    if start == CalendarWeekStart::Sunday {
        ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
    } else {
        ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
    }
}

fn date_for_event(
    displayed: CalendarDate,
    selected: CalendarDate,
    event: &Event,
) -> Option<CalendarDate> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.action == KeyAction::Release
        || key.modifiers.alt
        || key.modifiers.control
        || key.modifiers.meta
    {
        return None;
    }
    let displayed = CalendarDate::new(displayed.year, i32::from(displayed.month), 1);
    let selected = active_date(displayed, selected);
    match key.code {
        KeyCode::Left => Some(add_days(selected, -1)),
        KeyCode::Right => Some(add_days(selected, 1)),
        KeyCode::Up => Some(add_days(selected, -7)),
        KeyCode::Down => Some(add_days(selected, 7)),
        KeyCode::PageUp => Some(add_months(selected, -1)),
        KeyCode::PageDown => Some(add_months(selected, 1)),
        KeyCode::Home => Some(displayed),
        KeyCode::End => Some(CalendarDate {
            year: displayed.year,
            month: displayed.month,
            day: days_in_month(displayed.year, displayed.month),
        }),
        _ => None,
    }
}

fn date_id(root: &NodeId, date: CalendarDate) -> NodeId {
    NodeId::new(format!(
        "{}/date/{:04}-{:02}-{:02}",
        root.as_str(),
        date.year,
        date.month,
        date.day
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        CalendarDate, CalendarWeekStart, add_days, add_months, days_in_month, month_offset,
        weekday_of,
    };

    #[test]
    fn gregorian_math_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/calendar.txt",
            "widget-calendar",
            &[
                "date",
                "delta-days",
                "expected-days",
                "delta-months",
                "expected-months",
                "weekday",
                "month-days",
                "monday-offset",
                "sunday-offset",
            ],
        ) else {
            return;
        };
        for record in records {
            let date = parse_date(record.field("date"));
            assert_eq!(
                format_date(add_days(date, number(record.field("delta-days")))),
                record.field("expected-days"),
                "case {} days",
                record.id
            );
            assert_eq!(
                format_date(add_months(date, number(record.field("delta-months")))),
                record.field("expected-months"),
                "case {} months",
                record.id
            );
            assert_eq!(
                weekday_of(date),
                number(record.field("weekday")),
                "case {} weekday",
                record.id
            );
            assert_eq!(
                i32::from(days_in_month(date.year, date.month)),
                number(record.field("month-days")),
                "case {} month days",
                record.id
            );
            let first = CalendarDate::new(date.year, i32::from(date.month), 1);
            assert_eq!(
                month_offset(first, CalendarWeekStart::Monday),
                number(record.field("monday-offset")),
                "case {} Monday offset",
                record.id
            );
            assert_eq!(
                month_offset(first, CalendarWeekStart::Sunday),
                number(record.field("sunday-offset")),
                "case {} Sunday offset",
                record.id
            );
        }
    }

    fn parse_date(value: &str) -> CalendarDate {
        let values: Vec<_> = value.split('-').map(number).collect();
        assert_eq!(values.len(), 3, "invalid date {value}");
        CalendarDate::new(values[0], values[1], values[2])
    }

    fn format_date(date: CalendarDate) -> String {
        format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
    }

    fn number(value: &str) -> i32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid i32 {value}: {error}"))
    }
}
