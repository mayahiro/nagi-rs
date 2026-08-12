# Nagi Rust実装

[English](README.md)

Nagi Rust実装はterminal application向けのnative Content、Text、VT、Surface、TUI、Widget、CLI、test support crateを提供します

## 要件

- Rust 1.85以降
- Edition 2024
- x86-64またはARM64のLinuxとmacOS

## 導入

使用するapplication frameworkと任意componentだけを追加します

```toml
[dependencies]
nagi-content = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # Source-neutral structured content
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" } # CLI application
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
| `nagi-content` | Immutableなsource-neutral text構造、semantic projection、annotation、resource validation |
| `nagi-text` | Unicode 17 grapheme、terminal幅profile、wrap、truncate、位置変換 |
| `nagi-vt` | Typed terminal input／output、Color、Attributes、Style |
| `nagi-surface` | Geometry、Cell、Surface描画、composition、diff、snapshot |
| `nagi-tui` | Terminal Presentation Rules、上限付きContentからNodeへのprojection、App lifecycle、semantic Node、Scoped KeyMap、layout、event、Effect、Subscription、terminal loop |
| `nagi-tui-widgets` | Public TUI APIから構築した27個の標準Widget |
| `nagi-tui-test` | Virtual input、resize、time、Effect、Subscription、frame検査 |
| `nagi-cli` | Localと継承Option、command-local typed Invocation scope、制御可能なUsage Variant付きstructured Help、target付きDiagnostic、handlerを含まないcompletion解決、段階実行Runtime Policy、process統合 |
| `nagi-cli-completion` | Bash、Zsh、Fish、PowerShell generatorと予約済みcompletion protocol |
| `nagi-cli-prompt` | 注入可能なI/Oを持つ任意の行指向Confirm、Select、Input、Secret |
| `nagi-cli-test` | ProcessなしのCLI input注入とoutput取得 |

[Nagi semantic specification](https://github.com/mayahiro/nagi/tree/main/spec)がGo実装と共有する挙動を定義します

[Public CLI API guide](https://github.com/mayahiro/nagi/blob/main/docs/CLI_API_ja.md)では継承Option、command-local scope、completion、Help presentation、structured validator、段階導入を説明します

## Application test

対応するtest support crateだけを追加します

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
```

`nagi-tui-test`は実terminalを使わずにMessage、terminal input、resize、virtual time、Effect、Subscription、Runtime notice、frame、activeなresolved actionを操作できます

`nagi-cli-test`はprocess起動やsignal handler設定を行わず、argvとprocess serviceを注入してoutputとExit Statusを取得します

共有の[event-driven application architecture](https://github.com/mayahiro/nagi/blob/main/docs/EVENT_DRIVEN_APPLICATIONS_ja.md)では、第2のUI loopを作らずprocess outputとtimerをNagiへ渡す方法を説明します

`RuntimeConfig::width_profile`と`TerminalOptions::width_profile`はCoreのmeasure、render、hit geometry、cursor配置で使うcell幅policyを1個選択します。幅計算を行うWidget builderには`ViewContext::width_profile`を渡します。予期しないasync lifecycle transitionは上限付きRuntime notice queueまたはterminal notice-handler entry pointから観測できます

## Example

Rust repository rootから実行します

| Example | Command |
| --- | --- |
| [Source-neutral Content](crates/nagi-content/examples/content/README.md) | `cargo run -p nagi-content --example content` |
| [Presentation RulesとContent projection](crates/nagi-tui/examples/presentation/README.md) | `cargo run -p nagi-tui --example presentation` |
| [Counter](crates/nagi-tui/examples/counter/README.md) | `cargo run -p nagi-tui --example counter` |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Event-driven log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Virtual scroll](crates/nagi-tui/examples/virtual_scroll/README.md) | `cargo run -p nagi-tui --example virtual_scroll` |
| [Variable-height feed](crates/nagi-tui-widgets/examples/virtual_feed/README.md) | `cargo run -p nagi-tui-widgets --example virtual_feed` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filter付きList](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |
| [CLI basic](crates/nagi-cli/examples/basic/README.md) | `cargo run -p nagi-cli --example basic -- Nagi` |
| [CLI subcommands](crates/nagi-cli/examples/subcommands/README.md) | `cargo run -p nagi-cli --example subcommands -- start -vv` |
| [CLI段階導入](crates/nagi-cli/examples/staged/README.md) | `cargo run -p nagi-cli --example staged -- inspect page` |
| [CLI shell completion](crates/nagi-cli-completion/examples/completion/README.md) | `cargo run -p nagi-cli-completion --example completion -- generate bash` |
| [CLI軽量prompt](crates/nagi-cli-prompt/examples/prompt/README.md) | `cargo run -p nagi-cli-prompt --example prompt` |

## 制約

TUIのterminal inputとoutputはterminalへ接続されている必要があります。Mouse reportは既定で無効です。Raw modeとscreen stateは正常return、error、panic経路でbest effortとして復元します。Process abort、nested terminal session、suspendとresume、`/dev/tty`取得には対応していません

`ScrollViewport`はeagerなchild treeをclipしてscrollします。大規模dataでは`Node::virtual_scroll_viewport`を使用し、content全体のCell extentを宣言して現在表示する範囲または上限付きoverscanの`VirtualFragment`だけを構築できます。`Node::reveal_descendant`はfocusを移動せずstable descendant IDを表示範囲内に保ち、virtual targetは現在のfragment内に存在する必要があります

`Node::virtual_flow`は可変item heightとstable anchorをappend、prepend、削除、streaming更新、幅変更にまたがって保持し、visible fragmentとCell単位の上限付きoverscanだけを構築します。Intrinsic高は0のためlayout `Length`を割り当てます。`VirtualFeed`はdomain stateを所有せず、末尾追従とApplication制御のempty、loading、unread slotを追加します

`TextArea`はdefaultでno-wrap挙動を維持します。`soft_wrap`はvisual-line navigationを追加し、`boundary_navigation`はvisual boundaryのUpとDownをpass-throughへ切り替えられ、`viewport`はTab stopを増やさずapplication suppliedのzero-width typed cursor anchorへ追従します。Cursorはcaret graphemeを描かず後続textを移動しません

`Composer`は`TextArea`へcontrolled submitとhistory recall、自動row境界、任意のvalidation content、挿入制限を加えます。Applicationはmessageの意味、history persistence、sensitive value policyを引き続き所有します

`SelectableText`はimmutableなstyled contentへgrapheme境界に揃えたcontrolled keyboard selectionを加えます。Copy actionはApplication Messageを発行し、clipboard I/O、pointer selection、redaction policyはApplicationの責務として維持します

`Disclosure`はexpanded stateをApplicationに維持し、expanded時だけbodyを構築します。Core Modal scopeはdefaultでentry時に最初のdescendantへfocusし、close時に以前のfocusへ戻り、両方のtargetを設定できます。Modalがunhandled raw Eventとterminal fallback mappingも止める必要がある場合は`Node::block_unhandled_events`でopt-inのhard input boundaryを追加します

`Dialog`はapplication-defined action list、lazy controlled details、明示的なdefaultとcancel target、focus policy、Cell幅によるaction wrappingを構成します。`ConfirmDialog`はdefaultを明示する二action convenienceです

CLI process統合はLinuxとmacOSへ対応し、Unix argument valueを保持してSIGINTを協調的cancellationへ変換します

Shell固有生成と行指向Promptは任意crateです

Completion installation、dynamic candidate I/O、credential管理、approval policyはApplicationが所有します

設定file読み込みとCLIからTUIへの統合は提供しません

Portable graphは任意のinvocation grammarを表現しません

Help-only Usage Variantはparser semanticsを変更せずにvalidatorで支えるformを記述できます

## License

Source codeはMIT Licenseで提供します。生成済みUnicode dataは[Unicode License v3](UNICODE-LICENSE)で配布します
