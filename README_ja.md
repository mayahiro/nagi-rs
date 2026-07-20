# Nagi Rust実装

[English](README.md)

Nagi Rust実装はNagi terminal application familyのnative Rust workspaceです

共有TextとVT基盤、21個の標準Widgetを持つCell-based TUI framework、決定的test supportを持つcommand application frameworkを提供します

## 要件

- Rust 1.85以降
- Edition 2024
- x86-64またはARM64のLinuxとmacOS

## v0.2.0 release後のInstallation

v0.2.0の公開後、必要なapplication frameworkとtest supportだけを追加します

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # Optional
nagi-cli = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" } # CLI application
```

Tagはrelease済みrepository revisionを選択します。Applicationでは依存関係全体の解決結果を再現できるように`Cargo.lock`をcommitしてください

## TUI quick start

次のcounterはEnterでapplication stateを更新し、Escapeで終了します

```rust
use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Node, TerminalOptions, ViewContext,
    run_terminal,
};

enum Message {
    Increment,
    Quit,
}

#[derive(Default)]
struct Counter {
    count: u64,
    exiting: bool,
}

impl App for Counter {
    type Message = Message;

    fn update(&mut self, message: Message) -> Effect<Message> {
        match message {
            Message::Increment => self.count += 1,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Message> {
        let status = if self.exiting { "Stopping" } else { "Running" };
        Node::panel(
            Node::column([
                Node::text(format!("Count: {}", self.count)),
                Node::text(format!("Status: {status}")),
                Node::text("Press Enter to increment, Escape to exit"),
            ]),
            "Counter",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        Counter::default(),
        TerminalOptions::default(),
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Enter => {
                EventAction::Message(Message::Increment)
            }
            Event::Key(key) if key.code == KeyCode::Escape => {
                EventAction::Message(Message::Quit)
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
```

`App`はstateを所有し、`update`でMessageを1個ずつ処理し、`subscriptions`で長期sourceを宣言し、`view`でapplication stateと`ViewContext`からsemantic `Node` treeを再構築します。`Effect::exit`はterminal復元前に最後のdirty viewを描画します。省略した`init`と`subscriptions`はworkなしとして動作します

## CLI quick start

CLI process helperは明示的なstatusを返し、process自体を終了しません

```rust
use std::ffi::OsStr;
use std::io;
use std::process::ExitCode;

use nagi_cli::{Argument, Command, Context, Diagnostic, DiagnosticCode, Invocation, Outcome};

fn main() -> io::Result<ExitCode> {
    Command::new("greet")
        .about("Print a greeting")
        .argument(Argument::new("name").required())
        .handler(|context: &mut Context, invocation: &Invocation| {
            let name = invocation
                .raw_value("name")
                .and_then(OsStr::to_str)
                .ok_or_else(|| {
                    Diagnostic::new(DiagnosticCode::HandlerError, "name is not valid UTF-8")
                })?;
            writeln!(context.stdout(), "Hello, {name}!")
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
        .run_process()
        .map(Into::into)
}
```

Option parsing、Diagnostic、cancellation、言語間parityの完全な契約は[public CLI API guide](https://github.com/mayahiro/nagi/blob/main/docs/CLI_API_ja.md)と[共有semantics](https://github.com/mayahiro/nagi/blob/main/spec/cli.md)を参照してください

## Crateと機能

| Crate | 用途 |
| --- | --- |
| `nagi-tui` | App lifecycle、runtime、Core Node、layout、event、interaction、Effect、Subscription、terminal loop |
| `nagi-tui-widgets` | 21個の標準Widget |
| `nagi-text` | Unicode 17 grapheme、terminal幅profile、wrap、truncate、位置変換 |
| `nagi-surface` | Geometry、Cell、Surface描画、composition、diff、snapshot |
| `nagi-vt` | Typed terminal input／output、Color、Attributes、Style |
| `nagi-tui-test` | Virtual input、resize、time、Effect、Subscription、frame検査 |
| `nagi-cli` | Command Graph、typed Invocation、Context、Diagnostic、Help、process統合 |
| `nagi-cli-test` | ProcessなしのCLI input注入とoutput取得 |

Core compositionはText、RichText、Paragraph、安全なANSI SGR text、Surface、TextInput、Spacer、Gap、Row、Column、Stack、Padding、Border、Panel、Align、Clip、ScrollViewport、Modalを提供します

Widget libraryはList、Button、Modal、Progress、Spinner、Scrollbar、TextArea、Table、Tree、Tabs、Checkbox、Radio、Select、Command Palette、Sparkline、BarChart、Chart、Help、Paginator、FilePicker、Calendarを提供します

## Application test

Test harnessをdevelopment dependencyへ追加します

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
nagi-cli-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.2.0" }
```

`nagi-tui-test`は実terminalを使わずにMessage、terminal input、resize、virtual time、controlled Effect、manual Subscriptionを操作できます。FrameとMessage history、Interaction State、supervisor diagnostic、canonical Surface snapshotも検査できます

`nagi-cli-test`はprocess起動やsignal handler設定を行わず、argv、stdin、environment、current directory、manual cancellationを注入してstdout、stderr、Exit Statusを取得します

## Example

Repository rootから実terminalで実行します

| Example | Command |
| --- | --- |
| [Command palette](crates/nagi-tui/examples/command_palette/README.md) | `cargo run -p nagi-tui --example command_palette` |
| [Async search](crates/nagi-tui/examples/async_search/README.md) | `cargo run -p nagi-tui --example async_search` |
| [Log viewer](crates/nagi-tui/examples/log_viewer/README.md) | `cargo run -p nagi-tui --example log_viewer` |
| [Widget gallery](crates/nagi-tui-widgets/examples/widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example widget_gallery` |
| [Extended widget gallery](crates/nagi-tui-widgets/examples/extended_widget_gallery/README.md) | `cargo run -p nagi-tui-widgets --example extended_widget_gallery` |
| [Dashboard](crates/nagi-tui-widgets/examples/dashboard/README.md) | `cargo run -p nagi-tui-widgets --example dashboard` |
| [Filter付きList](crates/nagi-tui-widgets/examples/filtered_list/README.md) | `cargo run -p nagi-tui-widgets --example filtered_list` |
| [File browser](crates/nagi-tui-widgets/examples/file_browser/README.md) | `cargo run -p nagi-tui-widgets --example file_browser` |
| [Multi-pane log viewer](crates/nagi-tui-widgets/examples/multi_pane_log_viewer/README.md) | `cargo run -p nagi-tui-widgets --example multi_pane_log_viewer` |
| [Form validation](crates/nagi-tui-widgets/examples/form_validation/README.md) | `cargo run -p nagi-tui-widgets --example form_validation` |
| CLI basic | `cargo run -p nagi-cli --example basic -- Nagi` |
| CLI subcommands | `cargo run -p nagi-cli --example subcommands -- start -vv` |

## Terminalの挙動と制約

Terminalのtext selectionを維持するため、mouse reportは既定で無効です。Pointer inputが必要なapplicationでは`TerminalOptions`で有効にしてください

Standard inputとoutputはterminalへ接続されている必要があります。Raw modeとscreen stateは正常return、error、panic経路でbest effortとして復元します。Process abort、nested terminal session、suspendとresume、`/dev/tty`取得には対応していません

CLI process統合はUnix argument valueを保持し、SIGINTを協調的cancellationへ変換し、x86-64とARM64のLinuxおよびmacOSへ対応します

v0.2.0 CLI coreはshell completion、設定file読み込み、interactive prompt、TUI統合を提供しません

## ライセンス

Nagi Rust実装のsource codeはMIT Licenseで提供します。生成済みUnicode dataは[Unicode License v3](https://github.com/mayahiro/nagi-rs/blob/main/UNICODE-LICENSE)で配布します
