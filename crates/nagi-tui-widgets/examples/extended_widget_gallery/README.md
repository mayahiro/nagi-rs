# Extended widget gallery

This example demonstrates `TextArea`, `Composer`, `SelectableText`,
`Disclosure`, `Dialog`, `Table`, `Tree`, `Tabs`, `Checkbox`, `Radio`, `Select`,
`CommandPalette`, and `Scrollbar` in one application

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example extended_widget_gallery
```

The first tab is focused at startup. Use Tab and Shift-Tab to move focus, arrow
keys to navigate the focused widget, and Enter or Space to activate it. Widgets
also accept mouse clicks. Escape exits. Control-C exits unless the focused
SelectableText consumes it to copy a non-empty selection

The example keeps all values, selections, Composer history, Disclosure and
Tree expansion flags, dialog visibility, and query text in application state.
Composer uses Enter to submit, Shift-Enter to insert a line, and Up or Down at
a visual boundary to recall history. SelectableText keeps selection state in
the application and turns selection or document copy actions into application
messages. The application returns a Clipboard Effect for that message, while
the terminal options explicitly opt in to write-only OSC 52 output. The generic
Dialog shows three application-defined actions. It is intended as an API survey
rather than a production screen layout
