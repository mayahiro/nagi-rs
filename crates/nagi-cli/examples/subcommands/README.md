# Nested subcommands

This example requires a `start` subcommand, expands its direct Usage Variant in
parent Help, reuses the local `profile` value ID in root and child scopes, and
returns a structured validator Diagnostic for a reserved profile

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example subcommands -- start -vv
```

It prints
`starting profile service from root default with verbosity 2`. Values before
and after child selection belong to different scopes:

```sh
cargo run -p nagi-cli --example subcommands -- \
  --profile platform start --profile canary -vv
```

Pass `--help` before or after the subcommand, or run
`cargo run -p nagi-cli --example subcommands -- help start`, to inspect the
generated Help. Pass `start --profile blocked` to inspect the application
Diagnostic code, target, hint, Usage category, and default rendering
