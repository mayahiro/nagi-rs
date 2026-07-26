# Staged adoption

This example parses before dispatch, inspects the selected stable command-ID
path, bridges one command to an existing handler, and delegates Help and
version actions through `run_parsed_with_policy`

It also uses Runtime Policy helpers to render parser Diagnostics and map their
semantic category to an existing CLI's exit status without adopting the full
process runtime

Run it from the Rust repository root:

```sh
cargo run -p nagi-cli --example staged -- inspect page
```

It prints `legacy inspect: page`. Use `--help`, `--version`, or
`inspect blocked` to exercise the other staged paths
