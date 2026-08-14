# Diff view

This example constructs typed diff lines in the application and projects them
through `DiffLayoutCache`. Nagi displays unified markers and old and new line
numbers without parsing a patch or accessing a repository

Run from `nagi-rs` in a real terminal:

```sh
cargo run -p nagi-tui-widgets --example diff_view
```

Use Up and Down to select lines, Shift-Up and Shift-Down to extend selection,
Left and Right to scroll a no-wrap layout, W to toggle wrapping, and Control-C
to emit an application-owned unified-text copy request. The example does not
apply a patch or write to an OS or terminal clipboard
