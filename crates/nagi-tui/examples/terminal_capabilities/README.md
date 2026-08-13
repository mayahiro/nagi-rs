# Terminal capabilities

This example opts into conservative terminal capability detection, displays the
result supplied through `ViewContext`, and reports whether Enter and Shift+Enter
arrive as distinct logical input

Run it from the Rust repository root in a real terminal:

```sh
cargo run -p nagi-tui --example terminal_capabilities
```

Press Enter or Shift+Enter to inspect the decoded keyboard protocol, and press
Escape to exit. A terminal without the queried Kitty keyboard protocol may
report unsupported and may not distinguish Shift+Enter from Enter

Capability metadata is observational. This example does not grant clipboard
or hyperlink output permission, and the positive support of those features
remains unknown when no portable active query exists
