# Nagi for Rust

[日本語](README_ja.md)

Nagi for Rust is the native Rust workspace for the Nagi terminal application
family

It provides shared Text and VT foundations, a cell-based TUI framework with 21
standard widgets, and a command-application framework with deterministic test
support

## Requirements

- Rust 1.85 or newer
- Edition 2024
- Linux or macOS on x86-64 or ARM64

## Installation after the v0.2.0 release

After v0.2.0 is published, add only the application framework and test support
that you need

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # CLI applications
```

The tag will select the released repository revision. For applications, commit
`Cargo.lock` so the complete dependency resolution is reproducible

## TUI quick start

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

## CLI quick start

The CLI process helper returns an explicit status and never terminates the
process itself

```rust
use std::ffi::OsStr;
use std::io;
use std::process::ExitCode;

use nagi_cli::{Argument, Command, Context, Diagnostic, DiagnosticCode, Invocation, Outcome};

fn main() -> io::Result<ExitCode> {
    Command::new("greet")
        .about("Print a greeting")
        .argument(Argument::new("name").required())
        .handler(|context: &mut Context, invocation: &Invocation| {
            let name = invocation
                .raw_value("name")
                .and_then(OsStr::to_str)
                .ok_or_else(|| {
                    Diagnostic::new(DiagnosticCode::HandlerError, "name is not valid UTF-8")
                })?;
            writeln!(context.stdout(), "Hello, {name}!")
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
        .run_process()
        .map(Into::into)
}
```

See the [public CLI API guide](https://github.com/mayahiro/nagi/blob/main/docs/CLI_API.md)
and [shared semantics](https://github.com/mayahiro/nagi/blob/main/spec/cli.md)
for option parsing, diagnostics, cancellation, and language parity

## Crates and capabilities

| Crate | Use |
| --- | --- |
| `nagi-tui` | App lifecycle, runtime, Core nodes, layout, events, interaction, Effects, Subscriptions, and terminal loop |
| `nagi-tui-widgets` | The 21 standard widgets |
| `nagi-text` | Unicode 17 graphemes, terminal-width profiles, wrapping, truncation, and positions |
| `nagi-surface` | Geometry, Cells, Surface drawing, composition, diffing, and snapshots |
| `nagi-vt` | Typed terminal input/output, Color, Attributes, and Style |
| `nagi-tui-test` | Virtual input, resize, time, effects, subscriptions, and frame inspection |
| `nagi-cli` | Command Graph, typed Invocation, Context, diagnostics, help, and process integration |
| `nagi-cli-test` | Process-free CLI input injection and output capture |

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
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
```

`nagi-tui-test` can drive messages, terminal input, resize, virtual time,
controlled Effects, and manual Subscriptions without a real terminal. It also
exposes frame and message history, interaction state, supervisor diagnostics,
and canonical Surface snapshots

`nagi-cli-test` injects argv, stdin, environment, current directory, and manual
cancellation, then captures stdout, stderr, and Exit Status without starting a
process or installing a signal handler

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
| CLI basic | `cargo run -p nagi-cli --example basic -- Nagi` |
| CLI subcommands | `cargo run -p nagi-cli --example subcommands -- start -vv` |

## Terminal behavior and limitations

Mouse reporting is disabled by default so terminal text selection remains
available. Enable it in `TerminalOptions` when an application needs pointer
input

Standard input and output must be connected to a terminal. Restoration of raw
mode and screen state is best effort on normal return, error, and panic paths.
Process abort, nested terminal sessions, suspend and resume, and `/dev/tty`
acquisition are not supported

CLI process integration preserves Unix argument values, converts SIGINT into
cooperative cancellation, and supports Linux and macOS on x86-64 and ARM64

The v0.2.0 CLI core does not provide shell completion, configuration-file
loading, interactive prompts, or TUI integration

## License

Nagi for Rust source code is available under the MIT License. Generated
Unicode data is distributed under the
[Unicode License v3](https://github.com/mayahiro/nagi-rs/blob/main/UNICODE-LICENSE)
