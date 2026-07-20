# Counter

This minimal application keeps state in an `App`, updates it from Messages,
builds a semantic Node tree, and exits through an Effect

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example counter
```

Press Enter to increment the counter and Escape to render the final state and
restore the terminal
