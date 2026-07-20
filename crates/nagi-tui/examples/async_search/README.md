# Async search

This example demonstrates supervised asynchronous work with `Effect::latest`
and a controlled `TextInput`. A new query cancels the previous search generation
so stale results do not replace newer ones

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example async_search
```

The input is focused at startup. Type a query, or click the input to restore
focus, and press Escape or Control-C to exit. The search delay and data set are
simulated locally; no network request occurs

Source: [`main.rs`](main.rs)
