# Basic command

This example defines one required positional value, parses it as a
platform-native value, writes through the injected Context, and returns an
explicit Exit Status

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example basic -- Nagi
```

It prints `Hello, Nagi!`. Pass `--help` to inspect the generated help output
