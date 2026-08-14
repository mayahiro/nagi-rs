# Terminal suspend and resume

This example returns a terminal-suspending Effect to run an
application-selected interactive shell. Nagi temporarily restores the original
terminal mode and screen, then resumes the full-screen application after the
task returns

Run it from the Rust repository root in a real terminal:

```sh
cargo run -p nagi-tui --example terminal_suspend
```

Press `S` to run `$SHELL`, exit that shell to return to the application, and
press Escape to exit. The application owns shell selection and process result
mapping; Nagi owns only the terminal lifecycle around the task
