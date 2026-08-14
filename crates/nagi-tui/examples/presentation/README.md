# Terminal Presentation and Content Projection

This example resolves exact Content Roles and application state through an
immutable Presentation Sheet, then projects the Content into an ordinary TUI
Node. The later state-qualified rule restores the terminal-default foreground
and turns an earlier Bold declaration off without using VT Style merging

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example presentation
```

Expected output:

```text
display=paragraph
foreground=default
bold=false dim=true
projection=ok
```

Projection applies the same state resolver to each visited Element. It does
not derive a Node ID or action from Content metadata
