# Log viewer

This example demonstrates a high-frequency `Subscription`, latest-value
delivery, bounded application storage, frame coalescing, safe ANSI SGR text,
responsive `ViewContext` usage, and a Core `ScrollViewport` with
`stick_to_end`

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example log_viewer
```

The viewport is focused at startup and follows new log lines while it remains
at the end. PageUp or the mouse wheel pauses following without pausing input;
End resumes following. Press Space or P to pause input. Q, Escape, or Control-C
updates the application to its final `STOPPED` frame and returns `Effect::exit`
to restore the terminal. The help text compacts on narrow terminals. Generated
log lines include SGR colors and the buffer is capped at 1,000 lines

Source: [`main.rs`](main.rs)
