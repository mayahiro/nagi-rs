# Nagi TUI for Rust

[日本語](README_ja.md)

Nagi TUI for Rust is the native Rust implementation of the Nagi TUI cell-based
terminal user interface framework

It provides a declarative application runtime, Unicode-aware semantic nodes,
21 standard widgets, supervised asynchronous work, subscriptions, and a
deterministic test harness

## Requirements

- Rust 1.85 or newer
- Edition 2024
- Linux or macOS on x86-64 or ARM64

## Installation after the first release

After Nagi TUI Rust v0.1.0 is published, add the runtime and, when needed, the
widget library

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" } # Optional
```

The tag will select the released repository revision. For applications, commit
`Cargo.lock` so the complete dependency resolution is reproducible

## Quick start

This counter updates application state on Enter and exits on Escape

```rust
use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Node, TerminalOptions, ViewContext,
    run_terminal,
};

enum Message {
    Increment,
    Quit,
}

#[derive(Default)]
struct Counter {
    count: u64,
    exiting: bool,
}

impl App for Counter {
    type Message = Message;

    fn update(&mut self, message: Message) -> Effect<Message> {
        match message {
            Message::Increment => self.count += 1,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Message> {
        let status = if self.exiting { "Stopping" } else { "Running" };
        Node::panel(
            Node::column([
                Node::text(format!("Count: {}", self.count)),
                Node::text(format!("Status: {status}")),
                Node::text("Press Enter to increment, Escape to exit"),
            ]),
            "Counter",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        Counter::default(),
        TerminalOptions::default(),
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Enter => {
                EventAction::Message(Message::Increment)
            }
            Event::Key(key) if key.code == KeyCode::Escape => {
                EventAction::Message(Message::Quit)
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
```

An `App` owns its state, processes one Message at a time in `update`, declares
long-lived sources through `subscriptions`, and rebuilds a semantic `Node` tree
from application state and `ViewContext`. `Effect::exit` renders the final
dirty view before terminal restoration. `init` and `subscriptions` default to
no work when omitted

## Crates and capabilities

| Crate | Use |
| --- | --- |
| `nagi-tui` | App lifecycle, runtime, Core nodes, layout, events, interaction, Effects, Subscriptions, and terminal loop |
| `nagi-tui-widgets` | The 21 standard widgets |
| `nagi-text` | Unicode 17 graphemes, terminal-width profiles, wrapping, truncation, and positions |
| `nagi-surface` | Geometry, Cells, Surface drawing, composition, diffing, and snapshots |
| `nagi-vt` | Typed terminal input/output, Color, Attributes, and Style |
| `nagi-tui-test` | Virtual input, resize, time, effects, subscriptions, and frame inspection |

Core composition includes Text, RichText, Paragraph, safe ANSI SGR text,
Surface, TextInput, Spacer, Gap, Row, Column, Stack, Padding, Border, Panel,
Align, Clip, ScrollViewport, and Modal nodes

The widget library includes List, Button, Modal, Progress, Spinner, Scrollbar,
TextArea, Table, Tree, Tabs, Checkbox, Radio, Select, Command Palette,
Sparkline, BarChart, Chart, Help, Paginator, FilePicker, and Calendar

## Testing applications

Add the test harness as a development dependency

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" }
```

`nagi-tui-test` can drive messages, terminal input, resize, virtual time,
controlled Effects, and manual Subscriptions without a real terminal. It also
exposes frame and message history, interaction state, supervisor diagnostics,
and canonical Surface snapshots

## Examples

Run examples from the repository root in a real terminal

| Example | Command |
| --- | --- |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filtered list](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |

## Terminal behavior and limitations

Mouse reporting is disabled by default so terminal text selection remains
available. Enable it in `TerminalOptions` when an application needs pointer
input

Standard input and output must be connected to a terminal. Restoration of raw
mode and screen state is best effort on normal return, error, and panic paths.
Process abort, nested terminal sessions, suspend and resume, and `/dev/tty`
acquisition are not supported

## License

Nagi TUI for Rust source code is available under the MIT License. Generated
Unicode data is distributed under the
[Unicode License v3](https://github.com/mayahiro/nagi-rs/blob/main/UNICODE-LICENSE)
