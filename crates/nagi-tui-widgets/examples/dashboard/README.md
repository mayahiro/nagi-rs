# Dashboard

This example builds an operations dashboard from public Nagi TUI nodes and
widgets

It combines `Panel`, `Sparkline`, `Progress`, `Chart`, `BarChart`, `Table`,
`StatusBar`, `ToastRegion`, and `Help` in a responsive layout. The table keeps
selection in application state, sizes columns with `Fixed` and `Flex`, acts as
one Tab stop, and keeps a fixed header while its body follows selection

Changing the service replaces one controlled Toast and schedules its expiry
with an After Effect. The expiry message carries an application generation, so
a stale timer cannot remove a newer replacement. Status slots use arbitrary
Nodes and Core priority-based narrow-width omission

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example dashboard
```

Use Tab to focus the service table, arrow keys to change its selection, and
Escape or Control-C to exit
