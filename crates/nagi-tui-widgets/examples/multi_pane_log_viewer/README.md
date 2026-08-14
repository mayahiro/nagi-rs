# Multi-pane log viewer

This example combines Nagi TUI's subscription runtime with a bounded log model
and a multi-pane interface

`SplitPane` allocates the source and event panes, clamps controlled divider
movement to pane minima, and collapses the source pane on narrow terminals.
`Drawer` lazily constructs a modal details panel over the bottom edge. A
simulated subscription uses latest-value delivery and stops while the
application is paused

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example multi_pane_log_viewer
```

Use F6 or Shift-F6 to move between panes, Alt-Left or Alt-Right or a pointer
drag to resize, arrow keys to select, d to toggle details, and p to pause.
Escape closes an open details drawer before the terminal fallback exits;
Control-C exits directly
