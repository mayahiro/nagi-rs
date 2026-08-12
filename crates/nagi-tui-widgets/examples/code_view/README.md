# Code view

This example keeps styled logical lines, selection, wrap policy, and copy
handling in the application. `CodeLayoutCache` avoids re-projecting unchanged
source on each immutable view rebuild and uses terminal width as part of its
application-defined key

Run from `nagi-rs` in a real terminal:

```sh
cargo run -p nagi-tui-widgets --example code_view
```

Use Up and Down to select lines, Shift-Up and Shift-Down to extend selection,
Left and Right to scroll a no-wrap layout, W to toggle wrapping, and Control-C
to emit an application-owned copy request. The example does not write to an OS
or terminal clipboard
