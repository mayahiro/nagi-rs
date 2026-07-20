# Nagi Rust実装

[English](README.md)

Nagi Rust実装はterminal application向けのnative Text、VT、Surface、TUI、Widget、CLI、test support crateを提供します

## 要件

- Rust 1.85以降
- Edition 2024
- x86-64またはARM64のLinuxとmacOS

## 導入

使用するapplication frameworkと任意componentだけを追加します

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # CLI application
```

依存関係全体の解決結果を維持するため、applicationの`Cargo.lock`をcommitしてください

## Quick start

最小のstateful TUI applicationを実行します

```sh
cargo run -p nagi-tui --example counter
```

最小のcommand applicationを実行します

```sh
cargo run -p nagi-cli --example basic -- Nagi
```

完全なsourceと挙動は下記exampleに記載します

## Crate

| Crate | 責務 |
| --- | --- |
| `nagi-text` | Unicode 17 grapheme、terminal幅profile、wrap、truncate、位置変換 |
| `nagi-vt` | Typed terminal input／output、Color、Attributes、Style |
| `nagi-surface` | Geometry、Cell、Surface描画、composition、diff、snapshot |
| `nagi-tui` | App lifecycle、semantic Node、layout、event、Effect、Subscription、terminal loop |
| `nagi-tui-widgets` | Public TUI APIから構築した21個の標準Widget |
| `nagi-tui-test` | Virtual input、resize、time、Effect、Subscription、frame検査 |
| `nagi-cli` | Command Graph、typed Invocation、Context、Diagnostic、Help、process統合 |
| `nagi-cli-test` | ProcessなしのCLI input注入とoutput取得 |

[Nagi semantic specification](https://github.com/mayahiro/nagi/tree/main/spec)がGo実装と共有する挙動を定義します

## Application test

対応するtest support crateだけを追加します

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
```

`nagi-tui-test`は実terminalを使わずにMessage、terminal input、resize、virtual time、Effect、Subscription、frameを操作できます

`nagi-cli-test`はprocess起動やsignal handler設定を行わず、argvとprocess serviceを注入してoutputとExit Statusを取得します

## Example

Rust repository rootから実行します

| Example | Command |
| --- | --- |
| [Counter](crates/nagi-tui/examples/counter/README.md) | `cargo run -p nagi-tui --example counter` |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Virtual scroll](crates/nagi-tui/examples/virtual_scroll/README.md) | `cargo run -p nagi-tui --example virtual_scroll` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filter付きList](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |
| [CLI basic](crates/nagi-cli/examples/basic/README.md) | `cargo run -p nagi-cli --example basic -- Nagi` |
| [CLI subcommands](crates/nagi-cli/examples/subcommands/README.md) | `cargo run -p nagi-cli --example subcommands -- start -vv` |

## 制約

TUIのterminal inputとoutputはterminalへ接続されている必要があります。Mouse reportは既定で無効です。Raw modeとscreen stateは正常return、error、panic経路でbest effortとして復元します。Process abort、nested terminal session、suspendとresume、`/dev/tty`取得には対応していません

`ScrollViewport`はeagerなchild treeをclipしてscrollします。大規模dataでは`Node::virtual_scroll_viewport`を使用し、content全体のCell extentを宣言して現在表示する範囲または上限付きoverscanの`VirtualFragment`だけを構築できます

CLI process統合はLinuxとmacOSへ対応し、Unix argument valueを保持してSIGINTを協調的cancellationへ変換します。Shell completion、設定file読み込み、interactive prompt、TUI統合は提供しません

## License

Source codeはMIT Licenseで提供します。生成済みUnicode dataは[Unicode License v3](UNICODE-LICENSE)で配布します
