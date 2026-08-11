# Terminal Presentation Rules

This example resolves exact Content Roles and application state through an
immutable Presentation Sheet. The later state-qualified rule restores the
terminal-default foreground and turns an earlier Bold declaration off without
using VT Style merging

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example presentation
```

Expected output:

```text
display=paragraph
foreground=default
bold=false dim=true
```

The example resolves values only. Content-to-Node projection remains a
separate contract
