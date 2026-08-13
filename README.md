# Nagi for Rust

[日本語](README_ja.md)

Nagi for Rust provides native Content, Text, VT, Surface, TUI, Widget, CLI, and
test support crates for terminal applications

## Requirements

- Rust 1.85 or newer
- Edition 2024
- Linux or macOS on x86-64 or ARM64

## Installation

Add only the application framework and optional components that you use

```toml
[dependencies]
nagi-content = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # Source-neutral structured content
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # CLI applications
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
| `nagi-content` | Immutable source-neutral text structure, semantic projection, annotations, and resource validation |
| `nagi-text` | Unicode 17 graphemes, terminal-width profiles, wrapping, truncation, and positions |
| `nagi-vt` | Typed terminal input/output, Color, Attributes, and Style |
| `nagi-surface` | Geometry, Cells, Surface drawing, composition, diffing, and snapshots |
| `nagi-tui` | Terminal Presentation Rules, bounded Content-to-Node projection, App lifecycle, semantic nodes, scoped key maps, layout, events, Effects, Subscriptions, and terminal loop |
| `nagi-tui-widgets` | 31 standard widgets built from the public TUI API |
| `nagi-tui-test` | Virtual input, resize, time, effects, subscriptions, and frame inspection |
| `nagi-cli` | Local and inherited options, command-local typed Invocation scopes, structured Help with controllable Usage Variants, targeted Diagnostics, handler-free completion resolution, staged Runtime Policy, and process integration |
| `nagi-cli-completion` | Bash, Zsh, Fish, and PowerShell generators plus the reserved completion protocol |
| `nagi-cli-prompt` | Optional line-oriented Confirm, Select, Input, and Secret prompts with injected I/O |
| `nagi-cli-status` | Optional synchronous TTY status, spinner, progress, and plain-log fallback with injected I/O |
| `nagi-cli-test` | Process-free CLI input injection and output capture |

The [Nagi semantic specifications](https://github.com/mayahiro/nagi/tree/main/spec)
define behavior shared with the Go implementations. The
[public CLI API guide](https://github.com/mayahiro/nagi/blob/main/docs/CLI_API.md)
explains inherited options, command-local scopes, completion, Help
presentation, structured validators, and staged adoption

## Testing applications

Add only the matching test support crate

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
```

`nagi-tui-test` drives messages, terminal input, resize, virtual time, Effects,
Subscriptions, pending terminal tasks and clipboard requests, Runtime notices,
frame inspection, and active resolved action queries without a real terminal

`nagi-cli-test` injects argv and process services, then captures output and Exit
Status without starting a process or installing a signal handler

The shared [event-driven application architecture](https://github.com/mayahiro/nagi/blob/main/docs/EVENT_DRIVEN_APPLICATIONS.md)
explains how process output and timers enter Nagi without a second UI loop

`RuntimeConfig::width_profile` and `TerminalOptions::width_profile` select one
cell width policy for Core measurement, rendering, hit geometry, and cursor
placement. Pass `ViewContext::width_profile` to width-sensitive widget builders.
Unexpected asynchronous lifecycle transitions are available through the
bounded Runtime notice queue or the terminal notice-handler entry point

`TerminalOptions::capability_detection` explicitly enables conservative
environment hints and an active Kitty keyboard query. The immutable result is
available as `ViewContext::terminal_capabilities`. Detection is disabled by
default, bounds configured color output without promoting it, and never
grants OSC 52 or another output policy. VT `Capabilities::color_level` selects
Monochrome, ANSI 16, Indexed 256, or True Color output

`Effect::suspend_terminal` runs an application-owned blocking task after the
standard runner restores the ordinary terminal and leaves its configured
viewport. Returning from the task resumes a full-screen viewport or reserves a
fresh inline region, resets pending decoder state, and forces a full redraw

`TerminalViewport::inline(height)` runs the same Runtime in a bounded region of
the main screen and leaves its final frame in terminal history. The standard
runner owns cursor discovery, resize placement, coordinate translation, and
restoration

## Examples

Run commands from the Rust repository root

| Example | Command |
| --- | --- |
| [Source-neutral content](crates/nagi-content/examples/content/README.md) | `cargo run -p nagi-content --example content` |
| [Presentation Rules and Content projection](crates/nagi-tui/examples/presentation/README.md) | `cargo run -p nagi-tui --example presentation` |
| [Counter](crates/nagi-tui/examples/counter/README.md) | `cargo run -p nagi-tui --example counter` |
| [Terminal capabilities](crates/nagi-tui/examples/terminal_capabilities/README.md) | `cargo run -p nagi-tui --example terminal_capabilities` |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Suggestion popup](crates/nagi-tui-widgets/examples/suggestion_popup/README.md) | `cargo run -p nagi-tui-widgets --example suggestion_popup` |
| [JSON inspector](crates/nagi-tui-widgets/examples/json_inspector/README.md) | `cargo run -p nagi-tui-widgets --example json_inspector` |
| [Code view](crates/nagi-tui-widgets/examples/code_view/README.md) | `cargo run -p nagi-tui-widgets --example code_view` |
| [Diff view](crates/nagi-tui-widgets/examples/diff_view/README.md) | `cargo run -p nagi-tui-widgets --example diff_view` |
| [Event-driven log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Terminal suspend and resume](crates/nagi-tui/examples/terminal_suspend/README.md) | `cargo run -p nagi-tui --example terminal_suspend` |
| [Inline terminal viewport](crates/nagi-tui/examples/inline_terminal/README.md) | `cargo run -p nagi-tui --example inline_terminal` |
| [Virtual scroll](crates/nagi-tui/examples/virtual_scroll/README.md) | `cargo run -p nagi-tui --example virtual_scroll` |
| [Variable-height feed](crates/nagi-tui-widgets/examples/virtual_feed/README.md) | `cargo run -p nagi-tui-widgets --example virtual_feed` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filtered list](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |
| [CLI basic](crates/nagi-cli/examples/basic/README.md) | `cargo run -p nagi-cli --example basic -- Nagi` |
| [CLI subcommands](crates/nagi-cli/examples/subcommands/README.md) | `cargo run -p nagi-cli --example subcommands -- start -vv` |
| [CLI staged adoption](crates/nagi-cli/examples/staged/README.md) | `cargo run -p nagi-cli --example staged -- inspect page` |
| [CLI shell completion](crates/nagi-cli-completion/examples/completion/README.md) | `cargo run -p nagi-cli-completion --example completion -- generate bash` |
| [CLI lightweight prompts](crates/nagi-cli-prompt/examples/prompt/README.md) | `cargo run -p nagi-cli-prompt --example prompt` |
| [CLI TTY-aware status](crates/nagi-cli-status/examples/status/README.md) | `cargo run -p nagi-cli-status --example status` |

## Limitations

TUI terminal input and output must be connected to a terminal. Mouse reporting
is disabled by default. Raw mode and screen restoration are best effort on
normal return, error, and panic paths. Application-requested temporary terminal
suspension is supported. Process abort, nested terminal sessions, job-control
suspension of the Nagi process, and `/dev/tty` acquisition are not supported

`ScrollViewport` clips and scrolls an eager child tree. Large data sets can use
`Node::virtual_scroll_viewport`, which declares the complete cell extent and
constructs only the current visible or bounded-overscan `VirtualFragment`.
`Node::reveal_descendant` keeps a stable descendant ID visible without moving
focus; virtual targets must be present in the current fragment

`Node::virtual_flow` retains variable item heights and stable anchors across
append, prepend, removal, streaming updates, and width changes while
constructing only the visible fragment and Cell-bounded overscan. It has zero
intrinsic height, so assign a layout `Length`. `VirtualFeed` adds end following
and application-controlled empty, loading, and unread slots without owning
domain state

`TextArea` keeps no-wrap behavior by default. `soft_wrap` adds visual-line
navigation, `boundary_navigation` can pass Up and Down through at visual
boundaries, and `viewport` follows an application-identified zero-width typed
cursor anchor without an extra Tab stop. The cursor does not draw a caret
grapheme or shift following text

`Composer` adds controlled submit and history recall, automatic row bounds,
optional validation content, and insertion limits over `TextArea`. Applications
retain ownership of message meaning, history persistence, and sensitive-value
policy

`SuggestionPopup` composes a generic `AnchoredOverlay` with controlled stable
candidate IDs, a bounded selected row window, replaceable loading and empty
content, semantic actions, and focus-preserving pointer activation. The
application owns query parsing, ranking, asynchronous Effects, cancellation,
and acceptance meaning

`SelectableText` adds controlled grapheme-aligned keyboard and left-button
drag selection over immutable styled content. Stable-ID pointer capture
survives controlled view rebuilds, and a drag can request one-Cell edge
scrolling from its nearest viewport. Copy actions emit application messages.
Applications may return `Effect::set_clipboard`, and
`TerminalClipboard::Osc52` provides an explicit write-only terminal backend.
Redaction policy, terminal support detection, and OS-specific clipboard
commands remain outside the widget

`JsonInspector` projects an immutable typed `JsonDocument` into a controlled
tree with bounded row construction and grapheme-safe scalar previews. Copy
requests retain the complete compact value. Parsing, schema validation,
redaction, clipboard policy, and domain meaning remain application-owned

`CodeView` projects immutable application-styled logical lines through a
memoized terminal-width layout. Tabs, wrapping, line numbers, line selection,
horizontal scrolling, bounded Node construction, and complete-line copy stay
independent from syntax parsing, files, diff meaning, and clipboard I/O

`DiffView` accepts immutable typed metadata, hunk, context, addition, and
deletion lines. It reuses the bounded code projection while adding old and new
line numbers, unified markers, semantic styles, and on-demand unified copy.
Diff parsing, repository access, patch application, approval policy, and
clipboard I/O remain application-owned

Core `Node::split_pane` allocates two horizontal or vertical panes with a
one-Cell divider, per-pane minima, a basis-point ratio, and a deterministic
collapse target. Omitted panes are absent from rendering, semantic routing,
focus, and lazy virtual preparation. `SplitPane` adds controlled F6 focus
movement, axis-aware keyboard resizing, and divider dragging without assigning
pane meaning

`Drawer` constructs its body only while its controlled open state is true,
places it at a viewport edge, and reuses Core Modal focus and routing by
default. Applications own open-state persistence, outside-click behavior, and
the meaning of drawer content

`StatusBar` composes arbitrary one-row slots through Core `ResponsiveRow`,
which retains higher-priority start, center, or end items before semantic
indexing. `ToastRegion` overlays only the newest configured number of lazy
`Toast` bodies. Applications own notification records and may pair an After
Effect with stable identity or a generation for stale-safe expiry

`Disclosure` keeps expanded state in the application and constructs its body
only while expanded. Core Modal scopes focus their first descendant on entry
and return to previous focus on close by default; both targets are configurable.
`Node::block_unhandled_events` adds an opt-in hard input boundary when a modal
must also stop unhandled raw Events and terminal fallback mapping

`Dialog` composes application-defined action lists, lazy controlled details,
explicit default and cancel targets, focus policies, and Cell-width action
wrapping. `ConfirmDialog` is the explicit-default two-action convenience

CLI process integration supports Linux and macOS, preserves Unix argument
values, and converts SIGINT into cooperative cancellation. Shell-specific
generation, line-oriented Prompt, and synchronous Status Reporter are optional
crates. Applications own completion installation, dynamic candidate I/O,
credential handling, approval policy, status timing, and progress meaning.
Configuration-file loading and CLI-to-TUI integration are not
provided. The portable graph does not model arbitrary invocation grammars.
Help-only Usage Variants can document validator-backed forms without changing
parser semantics

## License

Source code is available under the MIT License. Generated Unicode data is
distributed under the [Unicode License v3](UNICODE-LICENSE)
