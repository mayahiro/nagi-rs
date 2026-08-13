# Command lifecycle example

This example attaches generic Hidden and Deprecated metadata to commands and
options. It keeps explicit hidden syntax parseable, excludes it from generated
Help, and opts into plain deprecation notices through the Runtime Policy

Run Help from the Rust repository root:

```sh
cargo run -p nagi-cli --example lifecycle -- --help
```

The Help output includes replacement hints for `old` and `--legacy`, but omits
the `internal` command and option. Hidden is projection control, not a security
or redaction boundary

Run deprecated syntax:

```sh
cargo run -p nagi-cli --example lifecycle -- --legacy old
```

The configured policy writes one structured-use notice per deprecated target
to standard error before the handler runs. The default Runtime Policy remains
silent, and applications can inspect `Invocation::deprecation_notices`
directly
