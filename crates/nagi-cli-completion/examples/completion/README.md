# Shell completion example

This example builds one immutable `CompletionEngine`, handles the reserved
completion request before ordinary Command Graph dispatch, and generates
Bash, Zsh, Fish, or PowerShell integration text

Generate a Bash script from the Rust workspace root

```sh
cargo run -p nagi-cli-completion --example completion -- generate bash
```

Run the ordinary application path

```sh
cargo run -p nagi-cli-completion --example completion -- --profile preview
```

An installed script invokes the same executable with `__nagi_complete`. The
example checks that token through `nagi_cli_completion::handle` before calling
`Command::run_process`, so no application handler runs during completion
