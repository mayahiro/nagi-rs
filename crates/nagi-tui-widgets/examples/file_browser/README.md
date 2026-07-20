# File browser

This example demonstrates a controlled file browser built with `FilePicker`,
`Checkbox`, `Panel`, `RichText`, and `Help`

The directory tree is intentionally in memory. `FilePicker` only renders and
navigates application-supplied metadata; it never reads a path or accesses the
filesystem. A real application can perform its own authorized I/O and provide
the resulting entries through the same API

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example file_browser
```

Use Tab to move focus, arrow keys to select, Enter to open, and Left or
Backspace to return to the parent. Escape or Control-C exits
