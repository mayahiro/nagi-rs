pub fn node_declared_groups(
    groups: Vec<nagi_tui::ResolvedActions>,
) -> Vec<nagi_tui::ResolvedActions> {
    groups
        .into_iter()
        .filter(|group| {
            !matches!(
                group.actions().first().map(|action| action.id().as_str()),
                Some(nagi_tui::FOCUS_NEXT_ACTION_ID | nagi_tui::SCROLL_PAGE_UP_ACTION_ID)
            )
        })
        .collect()
}
