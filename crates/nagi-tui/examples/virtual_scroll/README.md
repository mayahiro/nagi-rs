# Virtual scroll

This example declares one million rows while constructing semantic Nodes only
for the resolved visible cell range

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example virtual_scroll
```

The builder receives `VirtualViewport`, creates stable row IDs for its visible
range, and returns a `VirtualFragment` whose origin matches that range. Use
PageUp, PageDown, Home, End, or the mouse wheel to scroll. Press Q or Escape to
exit

Source: [`main.rs`](main.rs)
