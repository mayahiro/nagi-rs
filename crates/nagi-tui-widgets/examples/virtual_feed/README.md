# Variable-height feed

This example uses `VirtualFeed` with 60 stable items whose heights vary from
one to three terminal Cells. The runtime constructs only the visible items and
Cell-bounded overscan. The application owns the unread flag and clears the
overlay when the scroll callback reports that the feed reached its end

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example virtual_feed
```

Use PageUp, PageDown, Home, End, or the mouse wheel to scroll. Escape exits

