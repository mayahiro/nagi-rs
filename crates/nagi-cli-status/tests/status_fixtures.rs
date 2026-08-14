//! Shared Status Reporter conformance fixtures

mod support;

use std::io::{self, Write};

use nagi_cli_status::{Options, Reporter, Snapshot, StatusIo};
use nagi_text::WidthProfile;

#[test]
fn status_reporter_matches_shared_fixtures() {
    for record in support::load(
        "cli/status.txt",
        "cli-status",
        &[
            "terminal",
            "width",
            "profile",
            "progress-width",
            "kind",
            "tick",
            "current",
            "total",
            "message",
            "repeat",
            "log",
            "end",
            "emitted",
            "output",
        ],
    ) {
        let width = match record.field("width") {
            "-" => None,
            value => Some(number::<usize>(value)),
        };
        let profile = match record.field("profile") {
            "modern" => WidthProfile::MODERN,
            "cjk" => WidthProfile::CJK,
            value => panic!("unknown width profile {value}"),
        };
        let options = Options::default()
            .with_progress_width(number(record.field("progress-width")))
            .with_width_profile(profile);
        let mut status_io = MemoryIo {
            terminal: record.field("terminal") == "yes",
            width,
            output: Vec::new(),
        };
        let mut emitted = Vec::new();
        let message = record.text("message");
        let snapshot = snapshot(&record, &message);
        {
            let mut reporter = Reporter::with_options(&mut status_io, options)
                .expect("fixture options must be valid");
            emitted.push(
                reporter
                    .update(snapshot)
                    .expect("fixture update must succeed"),
            );
            if record.field("repeat") == "yes" {
                emitted.push(
                    reporter
                        .update(snapshot)
                        .expect("fixture repeat must succeed"),
                );
            }
            if record.field("log") != "-" {
                reporter
                    .log(&record.text("log"))
                    .expect("fixture log must succeed");
            }
            match record.field("end") {
                "none" => {}
                "clear" => emitted.push(reporter.clear().expect("fixture clear must succeed")),
                "finish" => emitted.push(
                    reporter
                        .finish(snapshot)
                        .expect("fixture finish must succeed"),
                ),
                value => panic!("unknown fixture end {value}"),
            }
        }
        assert_eq!(
            emitted_snapshot(&emitted),
            record.field("emitted"),
            "case {}",
            record.id
        );
        assert_eq!(
            status_io.output,
            record.bytes("output"),
            "case {}",
            record.id
        );
    }
}

fn snapshot<'message>(record: &support::Record, message: &'message str) -> Snapshot<'message> {
    match record.field("kind") {
        "status" => Snapshot::status(message),
        "spinner" => Snapshot::spinner(number(record.field("tick")), message),
        "progress" => Snapshot::progress(
            number(record.field("current")),
            number(record.field("total")),
            message,
        ),
        value => panic!("unknown snapshot kind {value}"),
    }
}

fn emitted_snapshot(values: &[bool]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn number<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}

struct MemoryIo {
    terminal: bool,
    width: Option<usize>,
    output: Vec<u8>,
}

impl Write for MemoryIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl StatusIo for MemoryIo {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn terminal_width(&self) -> Option<usize> {
        self.width
    }
}
