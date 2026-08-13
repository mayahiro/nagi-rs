# JSON Diagnostic example

This example projects one structured Diagnostic through a Runtime Policy using
the stable compact JSON schema

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example json_diagnostic
```

The example writes to standard output so its record can be inspected or piped
to another process. A command executed through the same Runtime Policy writes
Diagnostics to standard error and preserves the category-to-exit-status policy

Every rendered record has one final newline and includes the schema, code,
category, message, command path, nullable usage, ordered targets, and ordered
hints
