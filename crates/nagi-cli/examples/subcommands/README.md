# Nested subcommands

This example requires a `start` subcommand and uses a repeatable `-v` count
option

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example subcommands -- start -vv
```

It prints `starting with verbosity 2`. Pass `--help` before or after the
subcommand to inspect the generated help for that command path
