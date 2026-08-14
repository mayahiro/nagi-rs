# Value Source Adapter example

This example maps already loaded, application-owned project configuration into
raw Value Option fallbacks. Nagi owns the fixed source precedence and typed
parsing; the application owns configuration loading, schema, and key mapping

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example value_sources
```

The deterministic in-memory adapter supplies `profile` with replace semantics
and `tag` with merge semantics:

```text
profile=workspace:external(project-config)
tags=configured:external(project-config),baseline:default
```

Command-line and environment values are resolved before the adapter and skip
its callback for that Value Option. An unresolved adapter result permits the
configured default. The resolver is synchronous and should project data that
the application has already loaded rather than start file or network I/O
