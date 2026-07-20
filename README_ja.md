# Nagi TUI Rust実装

[English](README.md)

Nagi TUI Rust実装は、セルベースのターミナルUIフレームワークNagi TUIのRustネイティブ実装です

宣言的application runtime、Unicode対応semantic Node、21個の標準Widget、supervised async work、Subscription、決定的test harnessを提供します

## 要件

- Rust 1.85以降
- Edition 2024
- x86-64またはARM64のLinuxとmacOS

## 最初のrelease後のInstallation

Nagi TUI Rust v0.1.0の公開後にRuntimeと必要なWidget libraryを追加します

```toml
[dependencies]
nagi-tui = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" }
nagi-tui-widgets = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" } # Optional
```

Tagはrelease済みrepository revisionを選択します。Applicationでは依存関係全体の解決結果を再現できるように`Cargo.lock`をcommitしてください

## Quick start

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

## Crateと機能

| Crate | 用途 |
| --- | --- |
| `nagi-tui` | App lifecycle、runtime、Core Node、layout、event、interaction、Effect、Subscription、terminal loop |
| `nagi-tui-widgets` | 21個の標準Widget |
| `nagi-text` | Unicode 17 grapheme、terminal幅profile、wrap、truncate、位置変換 |
| `nagi-surface` | Geometry、Cell、Surface描画、composition、diff、snapshot |
| `nagi-vt` | Typed terminal input／output、Color、Attributes、Style |
| `nagi-tui-test` | Virtual input、resize、time、Effect、Subscription、frame検査 |

Core compositionはText、RichText、Paragraph、安全なANSI SGR text、Surface、TextInput、Spacer、Gap、Row、Column、Stack、Padding、Border、Panel、Align、Clip、ScrollViewport、Modalを提供します

Widget libraryはList、Button、Modal、Progress、Spinner、Scrollbar、TextArea、Table、Tree、Tabs、Checkbox、Radio、Select、Command Palette、Sparkline、BarChart、Chart、Help、Paginator、FilePicker、Calendarを提供します

## Application test

Test harnessをdevelopment dependencyへ追加します

```toml
[dev-dependencies]
nagi-tui-test = { git = "https://github.com/mayahiro/nagi-rs", tag = "v0.1.0" }
```

`nagi-tui-test`は実terminalを使わずにMessage、terminal input、resize、virtual time、controlled Effect、manual Subscriptionを操作できます。FrameとMessage history、Interaction State、supervisor diagnostic、canonical Surface snapshotも検査できます

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

## Terminalの挙動と制約

Terminalのtext selectionを維持するため、mouse reportは既定で無効です。Pointer inputが必要なapplicationでは`TerminalOptions`で有効にしてください

Standard inputとoutputはterminalへ接続されている必要があります。Raw modeとscreen stateは正常return、error、panic経路でbest effortとして復元します。Process abort、nested terminal session、suspendとresume、`/dev/tty`取得には対応していません

## ライセンス

Nagi TUI Rust実装のsource codeはMIT Licenseで提供します。生成済みUnicode dataは[Unicode License v3](https://github.com/mayahiro/nagi-rs/blob/main/UNICODE-LICENSE)で配布します
