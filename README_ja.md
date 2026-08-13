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
| `nagi-tui-widgets` | Public TUI APIから構築した31個の標準Widget |
| `nagi-tui-test` | Virtual input、resize、time、Effect、Subscription、frame検査 |
| `nagi-cli` | Localと継承Option、汎用Hidden、Deprecated、Sensitive metadata、command-local typed Invocation scope、制御可能なUsage Variant付きstructured Help、stable JSON renderingを持つtarget付きDiagnostic、handlerを含まないcompletion解決、段階実行Runtime Policy、process統合 |
| `nagi-cli-completion` | Bash、Zsh、Fish、PowerShell generatorと予約済みcompletion protocol |
| `nagi-cli-document` | 任意の決定的なCommonMarkとsection 1 man Help renderer |
| `nagi-cli-prompt` | 注入可能なI/Oを持つ任意の行指向Confirm、Select、Input、Secret |
| `nagi-cli-status` | 注入可能なI/Oを持つ任意の同期TTY status、spinner、progress、plain-log fallback |
| `nagi-cli-test` | ProcessなしのCLI input注入とoutput取得 |

[Nagi semantic specification](https://github.com/mayahiro/nagi/tree/main/spec)がGo実装と共有する挙動を定義します

[Public CLI API guide](https://github.com/mayahiro/nagi/blob/main/docs/CLI_API_ja.md)では継承Option、command-local scope、completion、Help presentation、lifecycleとSensitive Value metadata、structured validator、段階導入を説明します

## Application test

対応するtest support crateだけを追加します

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.7" }
```

`nagi-tui-test`は実terminalを使わずにMessage、terminal input、resize、virtual time、Effect、Subscription、pending terminal taskとclipboard request、Runtime notice、frame、activeなresolved actionを操作できます

`nagi-cli-test`はprocess起動やsignal handler設定を行わず、argvとprocess serviceを注入してoutputとExit Statusを取得します

共有の[event-driven application architecture](https://github.com/mayahiro/nagi/blob/main/docs/EVENT_DRIVEN_APPLICATIONS_ja.md)では、第2のUI loopを作らずprocess outputとtimerをNagiへ渡す方法を説明します

`RuntimeConfig::width_profile`と`TerminalOptions::width_profile`はCoreのmeasure、render、hit geometry、cursor配置で使うcell幅policyを1個選択します。幅計算を行うWidget builderには`ViewContext::width_profile`を渡します。予期しないasync lifecycle transitionは上限付きRuntime notice queueまたはterminal notice-handler entry pointから観測できます

`TerminalOptions::capability_detection`は保守的なenvironment hintとactiveなKitty keyboard queryを明示的に有効化します。Immutableな結果は`ViewContext::terminal_capabilities`から参照できます。検出は既定で無効であり、設定済みcolor outputを昇格させず上限として制約し、OSC 52やその他のoutput policyを許可しません。VTの`Capabilities::color_level`はMonochrome、ANSI 16、Indexed 256、True Color outputを選択します

`Effect::suspend_terminal`は標準runnerが通常terminalを復元して設定済みviewportを離れた後にApplication所有のblocking taskを実行します。Task return後はfull-screen viewportを再開するか新しいinline領域を確保し、pending decoder stateをresetしてfull redrawを強制します

`TerminalViewport::inline(height)`は同じRuntimeをmain screenの上限付き領域で実行し、最終frameをterminal historyへ残します。標準runnerがcursor取得、resize配置、座標変換、復元を所有します

## Example

Rust repository rootから実行します

| Example | Command |
| --- | --- |
| [Source-neutral Content](crates/nagi-content/examples/content/README.md) | `cargo run -p nagi-content --example content` |
| [Presentation RulesとContent projection](crates/nagi-tui/examples/presentation/README.md) | `cargo run -p nagi-tui --example presentation` |
| [Counter](crates/nagi-tui/examples/counter/README.md) | `cargo run -p nagi-tui --example counter` |
| [Terminal capability](crates/nagi-tui/examples/terminal_capabilities/README.md) | `cargo run -p nagi-tui --example terminal_capabilities` |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Suggestion popup](crates/nagi-tui-widgets/examples/suggestion_popup/README.md) | `cargo run -p nagi-tui-widgets --example suggestion_popup` |
| [JSON inspector](crates/nagi-tui-widgets/examples/json_inspector/README.md) | `cargo run -p nagi-tui-widgets --example json_inspector` |
| [Code view](crates/nagi-tui-widgets/examples/code_view/README.md) | `cargo run -p nagi-tui-widgets --example code_view` |
| [Diff view](crates/nagi-tui-widgets/examples/diff_view/README.md) | `cargo run -p nagi-tui-widgets --example diff_view` |
| [Event-driven log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Terminal suspendとresume](crates/nagi-tui/examples/terminal_suspend/README.md) | `cargo run -p nagi-tui --example terminal_suspend` |
| [Inline terminal viewport](crates/nagi-tui/examples/inline_terminal/README.md) | `cargo run -p nagi-tui --example inline_terminal` |
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
| [CLI JSON Diagnostic](crates/nagi-cli/examples/json_diagnostic/README.md) | `cargo run -p nagi-cli --example json_diagnostic` |
| [CLI Command lifecycle](crates/nagi-cli/examples/lifecycle/README.md) | `cargo run -p nagi-cli --example lifecycle -- --legacy old` |
| [CLI Sensitive Value](crates/nagi-cli/examples/sensitive_values/README.md) | `cargo run -p nagi-cli --example sensitive_values -- --token demo-token` |
| [CLI shell completion](crates/nagi-cli-completion/examples/completion/README.md) | `cargo run -p nagi-cli-completion --example completion -- generate bash` |
| [CLI Help派生document](crates/nagi-cli-document/examples/documentation/README.md) | `cargo run -p nagi-cli-document --example documentation -- markdown` |
| [CLI軽量prompt](crates/nagi-cli-prompt/examples/prompt/README.md) | `cargo run -p nagi-cli-prompt --example prompt` |
| [CLI TTY-aware status](crates/nagi-cli-status/examples/status/README.md) | `cargo run -p nagi-cli-status --example status` |

## 制約

TUIのterminal inputとoutputはterminalへ接続されている必要があります。Mouse reportは既定で無効です。Raw modeとscreen stateは正常return、error、panic経路でbest effortとして復元します。Applicationが要求する一時的なterminal suspendには対応します。Process abort、nested terminal session、Nagi processのjob-control suspend、`/dev/tty`取得には対応していません

`ScrollViewport`はeagerなchild treeをclipしてscrollします。大規模dataでは`Node::virtual_scroll_viewport`を使用し、content全体のCell extentを宣言して現在表示する範囲または上限付きoverscanの`VirtualFragment`だけを構築できます。`Node::reveal_descendant`はfocusを移動せずstable descendant IDを表示範囲内に保ち、virtual targetは現在のfragment内に存在する必要があります

`Node::virtual_flow`は可変item heightとstable anchorをappend、prepend、削除、streaming更新、幅変更にまたがって保持し、visible fragmentとCell単位の上限付きoverscanだけを構築します。Intrinsic高は0のためlayout `Length`を割り当てます。`VirtualFeed`はdomain stateを所有せず、末尾追従とApplication制御のempty、loading、unread slotを追加します

`TextArea`はdefaultでno-wrap挙動を維持します。`soft_wrap`はvisual-line navigationを追加し、`boundary_navigation`はvisual boundaryのUpとDownをpass-throughへ切り替えられ、`viewport`はTab stopを増やさずapplication suppliedのzero-width typed cursor anchorへ追従します。Cursorはcaret graphemeを描かず後続textを移動しません

`Composer`は`TextArea`へcontrolled submitとhistory recall、自動row境界、任意のvalidation content、挿入制限を加えます。Applicationはmessageの意味、history persistence、sensitive value policyを引き続き所有します

`SuggestionPopup`はgeneric `AnchoredOverlay`へcontrolled stable candidate ID、selected rowを含むbounded window、差し替え可能なloadingとempty content、semantic action、focusを維持するpointer activationを組み合わせます。Query解析、ranking、async Effect、cancellation、acceptの意味はApplicationが所有します

`SelectableText`はimmutableなstyled contentへgrapheme境界に揃えたcontrolled keyboardと左button drag selectionを加えます

Stable IDによるpointer captureはcontrolled view再構築後も継続し、dragは最も近いviewportへ1 Cell単位のedge scrollを要求できます

Copy actionはApplication Messageを発行します。Applicationは`Effect::set_clipboard`を返すことができ、`TerminalClipboard::Osc52`はwrite-only terminal backendを明示的に有効化します。Redaction policy、terminal support検出、OS固有clipboard commandはWidgetの外側に維持します

`JsonInspector`はimmutableなtyped `JsonDocument`をbounded row構築とgrapheme境界を保つscalar previewを持つcontrolled treeへ投影します。Copy requestは完全なcompact valueを保持し、parser、schema validation、redaction、clipboard policy、domain上の意味はApplicationが所有します

`CodeView`はApplicationがstyleを付けたimmutableなlogical lineをmemo化したterminal幅layoutで投影します。Tab、wrap、line number、行選択、横scroll、上限付きNode構築、完全な行単位copyをsyntax parser、file、diffの意味、clipboard I/Oから独立させます

`DiffView`はimmutableなtyped metadata、hunk、context、addition、deletion lineを受け取ります。上限付きCode projectionを再利用しながらold／new line number、unified marker、semantic style、要求時のunified copyを追加します。Diff parse、repository access、patch apply、approval policy、clipboard I/OはApplicationが所有します

Core `Node::split_pane`はhorizontalまたはverticalな二paneを1 Cellのdivider、paneごとのminimum、basis-point ratio、決定的なcollapse targetで割り当てます。省略されたpaneはrender、semantic routing、focus、lazy virtual preparationの対象外です。`SplitPane`はpaneの意味を定義せず、controlledなF6 focus移動、axisに対応するkeyboard resize、divider dragを追加します

`Drawer`はcontrolledなopen stateがtrueの間だけbodyを構築してviewport edgeへ配置し、defaultではCore Modalのfocusとroutingを再利用します。Open stateの永続化、outside-click挙動、drawer contentの意味はApplicationが所有します

`Disclosure`はexpanded stateをApplicationに維持し、expanded時だけbodyを構築します。Core Modal scopeはdefaultでentry時に最初のdescendantへfocusし、close時に以前のfocusへ戻り、両方のtargetを設定できます。Modalがunhandled raw Eventとterminal fallback mappingも止める必要がある場合は`Node::block_unhandled_events`でopt-inのhard input boundaryを追加します

`Dialog`はapplication-defined action list、lazy controlled details、明示的なdefaultとcancel target、focus policy、Cell幅によるaction wrappingを構成します。`ConfirmDialog`はdefaultを明示する二action convenienceです

CLI process統合はLinuxとmacOSへ対応し、Unix argument valueを保持してSIGINTを協調的cancellationへ変換します

Shell固有生成、行指向Prompt、同期Status Reporterは任意crateです

Completion installation、dynamic candidate I/O、credential管理、approval policy、status更新時点、progressの意味はApplicationが所有します

Sensitive Value metadataはframework projectionをredactしますが、memoryをzeroizeせず、OSのprocess argumentまたはshell historyから値を隠しません

設定file読み込みとCLIからTUIへの統合は提供しません

Portable graphは任意のinvocation grammarを表現しません

Help-only Usage Variantはparser semanticsを変更せずにvalidatorで支えるformを記述できます

## License

Source codeはMIT Licenseで提供します。生成済みUnicode dataは[Unicode License v3](UNICODE-LICENSE)で配布します
