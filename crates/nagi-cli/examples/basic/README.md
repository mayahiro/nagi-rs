# Basic command

This example defines one required positional value, parses it as a
platform-native value, writes through the injected Context, and returns an
explicit Exit Status

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example basic -- Nagi
```

It prints `Hello, Nagi!`. Pass `--help` to inspect the named Help-only Usage
Variant, structured example, note, and link rendered with the command help
