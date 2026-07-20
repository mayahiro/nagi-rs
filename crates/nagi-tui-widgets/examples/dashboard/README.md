# Dashboard

This example builds a Ratatui-style operations dashboard from public Nagi TUI
nodes and widgets

It combines `Panel`, `Sparkline`, `Progress`, `Chart`, `BarChart`, `Table`, and
`Help` in a responsive row and column layout. The table keeps selection in
application state, sizes columns with `Fixed` and `Flex`, acts as one Tab stop,
and keeps a fixed header while its body follows selection

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example dashboard
```

Use Tab to focus the service table, arrow keys to change its selection, and
Escape or Control-C to exit
