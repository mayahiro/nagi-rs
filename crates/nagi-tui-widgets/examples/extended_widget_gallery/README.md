# Extended widget gallery

This example demonstrates `TextArea`, `Table`, `Tree`, `Tabs`, `Checkbox`,
`Radio`, `Select`, `CommandPalette`, and `Scrollbar` in one application

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example extended_widget_gallery
```

The first tab is focused at startup. Use Tab and Shift-Tab to move focus, arrow
keys to navigate the focused widget, and Enter or Space to activate it. Widgets
also accept mouse clicks. Escape or Control-C exits

The example keeps all values, selections, expansion flags, and query text in
application state. It is intended as an API survey rather than a production
screen layout
