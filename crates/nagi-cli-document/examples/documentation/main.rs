//! Renders every visible command Help Document without retaining all pages

use std::env;
use std::process::ExitCode;

use nagi_cli::{Command, HelpDocument, OptionSpec};
use nagi_cli_document::{ManRenderer, MarkdownRenderer};

fn main() -> ExitCode {
    let format = env::args().nth(1).unwrap_or_else(|| "markdown".to_owned());
    let command = Command::new("qed")
        .about("Manage coding sessions")
        .option(
            OptionSpec::value("config")
                .long("config")
                .help("Configuration path")
                .inherited(),
        )
        .subcommand(
            Command::new("run")
                .about("Run one task")
                .option(OptionSpec::flag("verbose").long("verbose")),
        );

    let mut first = true;
    let result = command.visit_help_documents(|document| {
        if !first {
            println!("---");
        }
        first = false;
        print!("{}", render(&format, document));
        true
    });
    if let Err(diagnostic) = result {
        eprintln!("{diagnostic}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn render(format: &str, document: &HelpDocument) -> String {
    match format {
        "man" => ManRenderer.render(document),
        _ => MarkdownRenderer.render(document),
    }
}
