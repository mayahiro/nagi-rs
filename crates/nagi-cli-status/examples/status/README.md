# TTY-aware status example

This example drives a transient spinner and determinate progress from ordinary
application state

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli-status --example status
```

On a terminal, updates replace one line on standard error and reserve the last
terminal Cell to avoid autowrap. When standard error is redirected, the same
Snapshots become newline-delimited plain logs without terminal controls or
spinner-frame noise

`Reporter` does not start a timer or task. The application owns update timing,
cancellation, progress meaning, and serialization with other standard-error
output. Use `Reporter::log` to write a permanent line while preserving an
active transient status
