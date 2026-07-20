//! Shared CLI conformance fixtures

mod support;

use std::ffi::{OsStr, OsString};
use std::io::{Cursor, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome,
    ParseResult, ValueSource, integer_parser, possible_values_parser, value_parser,
};

#[test]
fn parsing_matches_shared_fixtures() {
    let command = fixture_command();
    for record in support::load(
        "cli/parsing.txt",
        "cli-parsing",
        &["argv", "env", "expected"],
    ) {
        let result = command
            .parse_with_environment(
                arguments(&record.bytes("argv")),
                environment(&record.bytes("env")),
            )
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        assert_eq!(
            snapshot_parse(result),
            record.field("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn errors_match_shared_fixtures() {
    let command = fixture_command();
    for record in support::load("cli/errors.txt", "cli-errors", &["argv", "env", "expected"]) {
        let error = command
            .parse_with_environment(
                arguments(&record.bytes("argv")),
                environment(&record.bytes("env")),
            )
            .expect_err("fixture case must fail");
        assert_eq!(
            error.code().as_str(),
            record.field("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn help_matches_shared_fixtures() {
    let command = fixture_command();
    for record in support::load("cli/help.txt", "cli-help", &["path", "expected"]) {
        let mut path = vec!["nagi".to_owned()];
        if !record.field("path").is_empty() {
            path.extend(record.field("path").split('/').map(str::to_owned));
        }
        assert_eq!(
            command.render_help(&path).unwrap(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn runtime_matches_shared_fixtures() {
    for record in support::load(
        "cli/runtime.txt",
        "cli-runtime",
        &[
            "argv",
            "env",
            "stdin",
            "cwd",
            "cancelled",
            "status",
            "stdout",
            "stderr",
        ],
    ) {
        let stdout = SharedWriter::default();
        let stderr = SharedWriter::default();
        let (token, handle) = nagi_cli::cancellation_pair();
        if record.field("cancelled") == "true" {
            handle.cancel();
        }
        let mut context = Context::with_cancellation(
            Cursor::new(record.bytes("stdin")),
            stdout.clone(),
            stderr.clone(),
            environment(&record.bytes("env")),
            record.field("cwd"),
            token,
        );
        let outcome = runtime_command()
            .run(&mut context, arguments(&record.bytes("argv")))
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        assert_eq!(
            outcome.status().code().to_string(),
            record.field("status"),
            "case {} status",
            record.id
        );
        assert_eq!(
            stdout.bytes(),
            record.bytes("stdout"),
            "case {} stdout",
            record.id
        );
        assert_eq!(
            stderr.bytes(),
            record.bytes("stderr"),
            "case {} stderr",
            record.id
        );
    }
}

#[test]
fn definition_validation_and_typed_values_are_public() {
    let invalid = Command::new("root")
        .option(OptionSpec::flag("first").long("same"))
        .option(OptionSpec::flag("second").long("same"));
    assert_eq!(
        invalid.validate().unwrap_err().code(),
        DiagnosticCode::InvalidSpecification
    );

    let invocation = fixture_command()
        .parse(["serve", "--mode", "http", "--port", "42", "host"])
        .unwrap();
    let ParseResult::Invocation(invocation) = invocation else {
        panic!("expected invocation");
    };
    assert_eq!(invocation.value::<i64>("port"), Some(&42));
}

fn fixture_command() -> Command {
    Command::new("nagi")
        .about("Nagi fixture command")
        .version("1.2.3")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .help("Increase verbosity"),
        )
        .option(
            OptionSpec::value("output")
                .long("output")
                .short('o')
                .parser(value_parser("PATH", |value: &OsStr| Ok(value.to_owned())))
                .help("Output path"),
        )
        .option(
            OptionSpec::value("color")
                .long("color")
                .parser(possible_values_parser(["auto", "always", "never"]))
                .default_value("auto")
                .help("Color mode"),
        )
        .option(
            OptionSpec::value("config")
                .long("config")
                .short('c')
                .environment("NAGI_CONFIG")
                .help("Config path"),
        )
        .option(
            OptionSpec::value("tag")
                .long("tag")
                .short('t')
                .repeated()
                .help("Tag value"),
        )
        .option(
            OptionSpec::flag("force")
                .long("force")
                .short('f')
                .conflicts("dry-run")
                .help("Force operation"),
        )
        .option(
            OptionSpec::flag("dry-run")
                .long("dry-run")
                .short('n')
                .help("Dry run"),
        )
        .option(
            OptionSpec::value("token")
                .long("token")
                .requires("config")
                .help("Token value"),
        )
        .argument(Argument::new("input").help("Input value"))
        .argument(Argument::new("extra").repeated().help("Extra values"))
        .subcommand(
            Command::new("serve")
                .alias("s")
                .about("Serve files")
                .option(
                    OptionSpec::value("port")
                        .long("port")
                        .short('p')
                        .parser(integer_parser())
                        .default_value("8080")
                        .help("Port"),
                )
                .option(
                    OptionSpec::value("mode")
                        .long("mode")
                        .short('m')
                        .parser(possible_values_parser(["http", "https"]))
                        .required()
                        .help("Mode"),
                )
                .option(
                    OptionSpec::value("header")
                        .long("header")
                        .short('H')
                        .repeated()
                        .help("Header value"),
                )
                .argument(Argument::new("host").required().help("Host name")),
        )
}

fn runtime_command() -> Command {
    Command::new("nagi")
        .about("Nagi fixture command")
        .version("1.2.3")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .help("Increase verbosity"),
        )
        .option(
            OptionSpec::value("output")
                .long("output")
                .short('o')
                .parser(value_parser("PATH", |value: &OsStr| Ok(value.to_owned())))
                .help("Output path"),
        )
        .argument(Argument::new("input").help("Input value"))
        .argument(Argument::new("extra").repeated().help("Extra values"))
        .handler(|context: &mut Context, invocation: &Invocation| {
            if invocation.raw_value("input") == Some(OsStr::new("fail")) {
                return Err(Diagnostic::new(
                    DiagnosticCode::HandlerError,
                    "requested failure",
                ));
            }
            let command = invocation.command_path().join("/");
            let cwd = context.current_directory().display().to_string();
            let environment = context
                .environment(OsStr::new("NAGI_TEST"))
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_owned();
            let mut input = Vec::new();
            context
                .stdin()
                .read_to_end(&mut input)
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            write!(
                context.stdout(),
                "command={command}\ncwd={cwd}\nenv={environment}\nstdin={}\n",
                String::from_utf8_lossy(&input)
            )
            .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
}

fn arguments(bytes: &[u8]) -> Vec<OsString> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .split(|byte| *byte == b'|')
        .map(|value| OsString::from_vec(value.to_vec()))
        .collect()
}

fn environment(bytes: &[u8]) -> Vec<(OsString, OsString)> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .split(|byte| *byte == b'|')
        .map(|entry| {
            let separator = entry
                .iter()
                .position(|byte| *byte == b'=')
                .expect("fixture environment entry has equals sign");
            (
                OsString::from_vec(entry[..separator].to_vec()),
                OsString::from_vec(entry[separator + 1..].to_vec()),
            )
        })
        .collect()
}

fn snapshot_parse(result: ParseResult) -> String {
    match result {
        ParseResult::Help { command_path } => {
            format!("action;kind=help;command={}", command_path.join("/"))
        }
        ParseResult::Version { version } => format!("action;kind=version;value={version}"),
        ParseResult::Invocation(invocation) => snapshot_invocation(&invocation),
    }
}

fn snapshot_invocation(invocation: &Invocation) -> String {
    let mut values = Vec::new();
    for id in invocation.value_ids() {
        if invocation.flag(id).is_some() {
            values.push(format!("{id}=flag:true"));
        } else if let Some(count) = invocation.count(id) {
            values.push(format!("{id}=count:{count}"));
        } else if let Some(parsed) = invocation.parsed_values(id) {
            if !invocation.is_repeated(id) {
                values.push(format!(
                    "{id}=value:{}:{}",
                    source(parsed[0].source()),
                    hex(parsed[0].raw().as_bytes())
                ));
            } else {
                let joined = parsed
                    .iter()
                    .map(|value| {
                        format!("{}:{}", source(value.source()), hex(value.raw().as_bytes()))
                    })
                    .collect::<Vec<_>>()
                    .join("+");
                values.push(format!("{id}=values:{joined}"));
            }
        }
    }
    format!(
        "ok;command={};values={}",
        invocation.command_path().join("/"),
        values.join(",")
    )
}

fn source(source: ValueSource) -> &'static str {
    match source {
        ValueSource::CommandLine => "cli",
        ValueSource::Environment => "env",
        ValueSource::Default => "default",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl SharedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
