use nagi_surface::Surface;
use nagi_vt::{Color, SgrColor, SgrStyle, Style, TerminalOp};

pub(crate) fn operations(previous: Option<&Surface>, current: &Surface) -> Vec<TerminalOp> {
    let runs = match previous {
        Some(previous) => current.changed_runs(previous),
        None => (0..current.height())
            .filter(|_| current.width() != 0)
            .map(|row| nagi_surface::ChangedRun {
                row,
                start: 0,
                end: current.width(),
            })
            .collect(),
    };

    if runs.is_empty() && previous.is_some_and(|previous| previous.cursor() == current.cursor()) {
        return Vec::new();
    }

    let mut output = Vec::with_capacity(8);
    output.push(TerminalOp::BeginSynchronizedUpdate);
    output.push(TerminalOp::HideCursor);

    for run in runs {
        output.push(TerminalOp::MoveTo {
            x: run.start,
            y: run.row,
        });
        let Some(row) = current.row(run.row) else {
            continue;
        };
        let mut style = None;
        let mut text = String::new();
        for cell in &row[run.start as usize..run.end as usize] {
            if cell.is_continuation() {
                continue;
            }
            if style != Some(cell.style()) {
                flush_text(&mut output, &mut text);
                style = Some(cell.style());
                if cell.style() == Style::default() {
                    output.push(TerminalOp::ResetStyle);
                } else {
                    output.push(TerminalOp::SetStyle(sgr_style(cell.style())));
                }
            }
            text.push_str(cell.content());
        }
        flush_text(&mut output, &mut text);
    }

    output.push(TerminalOp::ResetStyle);
    match current.cursor() {
        Some(cursor) => {
            output.push(TerminalOp::MoveTo {
                x: cursor.x,
                y: cursor.y,
            });
            output.push(TerminalOp::ShowCursor);
        }
        None => output.push(TerminalOp::HideCursor),
    }
    output.push(TerminalOp::EndSynchronizedUpdate);
    output
}

fn flush_text(output: &mut Vec<TerminalOp>, text: &mut String) {
    if !text.is_empty() {
        output.push(TerminalOp::WriteText(std::mem::take(text)));
    }
}

fn sgr_style(style: Style) -> SgrStyle {
    SgrStyle {
        foreground: sgr_color(style.foreground),
        background: sgr_color(style.background),
        underline_color: style.underline_color.map(sgr_color),
        bold: style.bold,
        dim: style.dim,
        italic: style.italic,
        underline: style.underline,
        blink: style.blink,
        reverse: style.reverse,
        hidden: style.hidden,
        strikethrough: style.strikethrough,
    }
}

fn sgr_color(color: Color) -> SgrColor {
    match color {
        Color::Default => SgrColor::Default,
        Color::Indexed(index) => SgrColor::Indexed(index),
        Color::Rgb { red, green, blue } => SgrColor::Rgb { red, green, blue },
    }
}

#[cfg(test)]
mod tests {
    use nagi_surface::{Cursor, Surface};
    use nagi_text::WidthProfile;

    use super::*;

    #[test]
    fn first_frame_writes_complete_rows_and_cursor() {
        let mut current = Surface::new(3, 1).unwrap();
        current.write(
            0,
            0,
            "A日",
            Style {
                bold: true,
                ..Style::default()
            },
            WidthProfile::MODERN,
        );
        current.set_cursor(Some(Cursor::new(2, 0)));

        let actual = operations(None, &current);

        assert!(actual.contains(&TerminalOp::WriteText("A日".to_owned())));
        assert!(actual.contains(&TerminalOp::MoveTo { x: 2, y: 0 }));
        assert_eq!(actual.last(), Some(&TerminalOp::EndSynchronizedUpdate));
    }

    #[test]
    fn unchanged_second_frame_emits_no_operations() {
        let current = Surface::new(2, 1).unwrap();

        let actual = operations(Some(&current), &current);

        assert!(actual.is_empty());
    }
}
