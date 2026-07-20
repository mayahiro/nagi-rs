# Standard widget gallery

This example demonstrates the initial standard widgets: `List`, `Button`,
`Modal`, `Progress`, and `Spinner`. It also shows application-owned state and a
clock-driven spinner subscription

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example widget_gallery
```

The first list item is focused at startup. Use Tab and Shift-Tab to move focus,
arrow keys to navigate the list, and Enter or Space to activate buttons. The
list and buttons also accept mouse clicks. Press Q, Escape, or Control-C to exit

Source: [`main.rs`](main.rs)
