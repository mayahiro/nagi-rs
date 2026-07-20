use nagi_tui::{Event, KeyAction, KeyCode};

#[derive(Clone, Copy)]
pub(crate) enum Navigation {
    Normalize,
    Up,
    Down,
    Home,
    End,
}

pub(crate) fn navigate(count: usize, selected: usize, action: Navigation) -> Option<usize> {
    let selected = normalize_selection(count, selected)?;
    Some(match action {
        Navigation::Normalize => selected,
        Navigation::Up => selected.saturating_sub(1),
        Navigation::Down => selected.saturating_add(1).min(count - 1),
        Navigation::Home => 0,
        Navigation::End => count - 1,
    })
}

pub(crate) fn navigate_event(event: &Event, count: usize, selected: usize) -> Option<usize> {
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
    let action = match key.code {
        KeyCode::Up => Navigation::Up,
        KeyCode::Down => Navigation::Down,
        KeyCode::Home => Navigation::Home,
        KeyCode::End => Navigation::End,
        _ => return None,
    };
    navigate(count, selected, action)
}

fn normalize_selection(count: usize, selected: usize) -> Option<usize> {
    (count != 0).then(|| selected.min(count - 1))
}
