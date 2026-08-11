# Extended widget gallery

This example demonstrates `TextArea`, `Composer`, `Disclosure`, `Dialog`,
`Table`, `Tree`, `Tabs`, `Checkbox`, `Radio`, `Select`, `CommandPalette`, and
`Scrollbar` in one application

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example extended_widget_gallery
```

The first tab is focused at startup. Use Tab and Shift-Tab to move focus, arrow
keys to navigate the focused widget, and Enter or Space to activate it. Widgets
also accept mouse clicks. Escape or Control-C exits

The example keeps all values, selections, Composer history, Disclosure and
Tree expansion flags, dialog visibility, and query text in application state.
Composer uses Enter to submit, Shift-Enter to insert a line, and Up or Down at
a visual boundary to recall history. The generic Dialog shows three
application-defined actions. It is intended as an API survey rather than a
production screen layout
