use std::fmt::Write;

use nagi_vt::{Color, Style};

use crate::{Cell, Opacity, Surface};

pub(crate) fn snapshot(surface: &Surface) -> String {
    let mut output = String::new();
    write!(
        output,
        "nagi-surface-v1\twidth={}\theight={}\tcursor=",
        surface.width(),
        surface.height()
    )
    .expect("writing to a String cannot fail");
    if let Some(cursor) = surface.cursor() {
        write!(output, "{},{}", cursor.x, cursor.y).expect("writing to a String cannot fail");
    } else {
        output.push_str("none");
    }
    output.push('\n');

    for row in 0..surface.height() {
        write!(output, "row={row}").expect("writing to a String cannot fail");
        for cell in surface.row(row).expect("row is in bounds") {
            output.push('\t');
            write_cell(&mut output, cell);
        }
        output.push('\n');
    }
    output
}

fn write_cell(output: &mut String, cell: &Cell) {
    write_content(output, cell.content());
    write!(
        output,
        "/{}/{}/{}/",
        cell.span().cells(),
        if cell.is_continuation() {
            "cont"
        } else {
            "lead"
        },
        match cell.opacity() {
            Opacity::Opaque => "opaque",
            Opacity::Transparent => "transparent",
        }
    )
    .expect("writing to a String cannot fail");
    let style = cell.style();
    write_color(output, style.foreground);
    output.push('/');
    write_color(output, style.background);
    output.push('/');
    if let Some(color) = style.underline_color {
        write_color(output, color);
    } else {
        output.push_str("none");
    }
    output.push('/');
    write_attributes(output, style);
}

fn write_content(output: &mut String, content: &str) {
    if content.is_empty() {
        output.push('-');
        return;
    }
    for (index, character) in content.chars().enumerate() {
        if index != 0 {
            output.push('+');
        }
        write!(output, "U+{:04X}", u32::from(character)).expect("writing to a String cannot fail");
    }
}

fn write_color(output: &mut String, color: Color) {
    match color {
        Color::Default => output.push_str("default"),
        Color::Indexed(index) => {
            write!(output, "indexed:{index}").expect("writing to a String cannot fail");
        }
        Color::Rgb { red, green, blue } => {
            write!(output, "rgb:{red:02X}{green:02X}{blue:02X}")
                .expect("writing to a String cannot fail");
        }
    }
}

fn write_attributes(output: &mut String, style: Style) {
    let attributes = [
        (style.bold, "bold"),
        (style.dim, "dim"),
        (style.italic, "italic"),
        (style.underline, "underline"),
        (style.blink, "blink"),
        (style.reverse, "reverse"),
        (style.hidden, "hidden"),
        (style.strikethrough, "strikethrough"),
    ];
    let mut wrote_attribute = false;
    for (enabled, name) in attributes {
        if !enabled {
            continue;
        }
        if wrote_attribute {
            output.push('+');
        }
        output.push_str(name);
        wrote_attribute = true;
    }
    if !wrote_attribute {
        output.push('-');
    }
}
