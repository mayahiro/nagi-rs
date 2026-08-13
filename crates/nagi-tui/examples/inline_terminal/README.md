# Inline terminal viewport

This example runs the normal Nagi Runtime inside four rows of the terminal's
main screen. It does not enter the alternate screen, and its final frame remains
in terminal history after restoration

Run it from the Rust repository root in a real terminal:

```sh
cargo run -p nagi-tui --example inline_terminal
```

Press Enter to increment the counter and Escape to exit. The viewport spans the
terminal width, clamps to a shorter terminal, follows resize, and translates
mouse coordinates into its local region
