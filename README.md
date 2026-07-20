# Nagi for Rust

[日本語](README_ja.md)

Nagi for Rust provides native Text, VT, Surface, TUI, Widget, CLI, and test
support crates for terminal applications

## Requirements

- Rust 1.85 or newer
- Edition 2024
- Linux or macOS on x86-64 or ARM64

## Installation

Add only the application framework and optional components that you use

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # CLI applications
```

Commit an application's `Cargo.lock` to preserve its complete dependency
resolution

## Quick start

Run the minimal stateful TUI application:

```sh
cargo run -p nagi-tui --example counter
```

Run the minimal command application:

```sh
cargo run -p nagi-cli --example basic -- Nagi
```

The complete source and behavior are documented with the examples below

## Crates

| Crate | Responsibility |
| --- | --- |
| `nagi-text` | Unicode 17 graphemes, terminal-width profiles, wrapping, truncation, and positions |
| `nagi-vt` | Typed terminal input/output, Color, Attributes, and Style |
| `nagi-surface` | Geometry, Cells, Surface drawing, composition, diffing, and snapshots |
| `nagi-tui` | App lifecycle, semantic nodes, layout, events, Effects, Subscriptions, and terminal loop |
| `nagi-tui-widgets` | 21 standard widgets built from the public TUI API |
| `nagi-tui-test` | Virtual input, resize, time, effects, subscriptions, and frame inspection |
| `nagi-cli` | Command Graph, typed Invocation, Context, diagnostics, help, and process integration |
| `nagi-cli-test` | Process-free CLI input injection and output capture |

The [Nagi semantic specifications](https://github.com/mayahiro/nagi/tree/main/spec)
define behavior shared with the Go implementations

## Testing applications

Add only the matching test support crate

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
```

`nagi-tui-test` drives messages, terminal input, resize, virtual time, Effects,
Subscriptions, and frame inspection without a real terminal

`nagi-cli-test` injects argv and process services, then captures output and Exit
Status without starting a process or installing a signal handler

The shared [event-driven application architecture](https://github.com/mayahiro/nagi/blob/main/docs/EVENT_DRIVEN_APPLICATIONS.md)
explains how process output and timers enter Nagi without a second UI loop

## Examples

Run commands from the Rust repository root

| Example | Command |
| --- | --- |
| [Counter](crates/nagi-tui/examples/counter/README.md) | `cargo run -p nagi-tui --example counter` |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Event-driven log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Virtual scroll](crates/nagi-tui/examples/virtual_scroll/README.md) | `cargo run -p nagi-tui --example virtual_scroll` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filtered list](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |
| [CLI basic](crates/nagi-cli/examples/basic/README.md) | `cargo run -p nagi-cli --example basic -- Nagi` |
| [CLI subcommands](crates/nagi-cli/examples/subcommands/README.md) | `cargo run -p nagi-cli --example subcommands -- start -vv` |

## Limitations

TUI terminal input and output must be connected to a terminal. Mouse reporting
is disabled by default. Raw mode and screen restoration are best effort on
normal return, error, and panic paths. Process abort, nested terminal sessions,
suspend and resume, and `/dev/tty` acquisition are not supported

`ScrollViewport` clips and scrolls an eager child tree. Large data sets can use
`Node::virtual_scroll_viewport`, which declares the complete cell extent and
constructs only the current visible or bounded-overscan `VirtualFragment`

CLI process integration supports Linux and macOS, preserves Unix argument
values, and converts SIGINT into cooperative cancellation. Shell completion,
configuration-file loading, interactive prompts, and TUI integration are not
provided

## License

Source code is available under the MIT License. Generated Unicode data is
distributed under the [Unicode License v3](UNICODE-LICENSE)
