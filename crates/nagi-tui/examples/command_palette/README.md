# Command palette

This example demonstrates the application runtime and low-level Core
nodes by implementing a searchable command list without the standard widget
library. It shows global text input mapping, grapheme-safe backspace, selection
state, and declarative rendering

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example command_palette
```

Type to filter commands, use Up and Down to move selection, and press Enter or
Escape to exit. The example intentionally closes instead of executing a command

Source: [`main.rs`](main.rs)
