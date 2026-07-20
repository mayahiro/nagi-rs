use nagi_text::{grapheme_boundaries, next_grapheme_boundary, previous_grapheme_boundary};

#[cfg(test)]
use crate::fixture_support;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextEdit<'a> {
    Insert(&'a str),
    Paste(&'a str),
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
}

pub(crate) fn apply_text_edit(value: &str, cursor: usize, edit: TextEdit<'_>) -> (String, usize) {
    let cursor = normalize_cursor(value, cursor);
    match edit {
        TextEdit::Insert(text) | TextEdit::Paste(text) => {
            let mut output = String::with_capacity(value.len().saturating_add(text.len()));
            output.push_str(&value[..cursor]);
            output.push_str(text);
            output.push_str(&value[cursor..]);
            let intended = cursor.saturating_add(text.len());
            let next = grapheme_boundaries(&output)
                .into_iter()
                .find(|boundary| *boundary >= intended)
                .unwrap_or(output.len());
            (output, next)
        }
        TextEdit::Left => (
            value.to_owned(),
            previous_grapheme_boundary(value, cursor).unwrap_or(0),
        ),
        TextEdit::Right => (
            value.to_owned(),
            next_grapheme_boundary(value, cursor).unwrap_or(value.len()),
        ),
        TextEdit::Home => (value.to_owned(), 0),
        TextEdit::End => (value.to_owned(), value.len()),
        TextEdit::Backspace => {
            let start = previous_grapheme_boundary(value, cursor).unwrap_or(cursor);
            let mut output = value.to_owned();
            output.replace_range(start..cursor, "");
            (output, start)
        }
        TextEdit::Delete => {
            let end = next_grapheme_boundary(value, cursor).unwrap_or(cursor);
            let mut output = value.to_owned();
            output.replace_range(cursor..end, "");
            (output, cursor)
        }
    }
}

pub(crate) fn normalize_cursor(value: &str, cursor: usize) -> usize {
    grapheme_boundaries(value)
        .into_iter()
        .take_while(|boundary| *boundary <= cursor.min(value.len()))
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_match_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/text-input.txt",
            "text-input-edit",
            &[
                "initial",
                "cursor",
                "operation",
                "text",
                "expected",
                "expected-cursor",
            ],
        ) else {
            return;
        };
        for record in records {
            let initial = record.text("initial");
            let text = record.text("text");
            let edit = match record.field("operation") {
                "insert" => TextEdit::Insert(&text),
                "paste" => TextEdit::Paste(&text),
                "left" => TextEdit::Left,
                "right" => TextEdit::Right,
                "home" => TextEdit::Home,
                "end" => TextEdit::End,
                "backspace" => TextEdit::Backspace,
                "delete" => TextEdit::Delete,
                operation => panic!("invalid operation {operation}"),
            };
            assert_eq!(
                apply_text_edit(&initial, number(record.field("cursor")), edit),
                (
                    record.text("expected"),
                    number(record.field("expected-cursor"))
                ),
                "case {}",
                record.id
            );
        }
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
