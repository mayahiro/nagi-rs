//! Completion isolation, cancellation, and immutable projection behavior

use std::ffi::{OsStr, OsString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, CompletionCandidate, CompletionEngine, CompletionErrorKind, CompletionInput,
    CompletionProviderError, DiagnosticCode, Invocation, OptionSpec, Outcome, cancellation_pair,
    possible_values_parser, value_parser,
};

#[test]
fn completion_does_not_run_parser_validator_handler_or_unrelated_provider() {
    let parser_calls = Arc::new(AtomicUsize::new(0));
    let validator_calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::new(AtomicUsize::new(0));
    let active_provider_calls = Arc::new(AtomicUsize::new(0));
    let unrelated_provider_calls = Arc::new(AtomicUsize::new(0));

    let command = Command::new("root")
        .option(
            OptionSpec::value("mode")
                .long("mode")
                .parser(value_parser("MODE", {
                    let parser_calls = Arc::clone(&parser_calls);
                    move |value: &OsStr| {
                        parser_calls.fetch_add(1, Ordering::Relaxed);
                        Ok(value.to_owned())
                    }
                }))
                .completion_provider({
                    let active_provider_calls = Arc::clone(&active_provider_calls);
                    move |_cancellation: &nagi_cli::CancellationToken,
                          request: &nagi_cli::CompletionRequest| {
                        active_provider_calls.fetch_add(1, Ordering::Relaxed);
                        assert_eq!(request.target().value_id(), Some("mode"));
                        Ok(vec![CompletionCandidate::new("auto")])
                    }
                }),
        )
        .argument(Argument::new("other").completion_provider({
            let unrelated_provider_calls = Arc::clone(&unrelated_provider_calls);
            move |_cancellation: &nagi_cli::CancellationToken,
                  _request: &nagi_cli::CompletionRequest| {
                unrelated_provider_calls.fetch_add(1, Ordering::Relaxed);
                Ok(Vec::new())
            }
        }))
        .validator({
            let validator_calls = Arc::clone(&validator_calls);
            move |_invocation: &Invocation| {
                validator_calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .handler({
            let handler_calls = Arc::clone(&handler_calls);
            move |_context: &mut nagi_cli::Context, _invocation: &Invocation| {
                handler_calls.fetch_add(1, Ordering::Relaxed);
                Ok(Outcome::success())
            }
        });

    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["--mode"], "a"),
        )
        .expect("completion must succeed");
    assert_eq!(
        result
            .candidates()
            .iter()
            .map(CompletionCandidate::value)
            .collect::<Vec<_>>(),
        ["auto"]
    );
    assert_eq!(parser_calls.load(Ordering::Relaxed), 0);
    assert_eq!(validator_calls.load(Ordering::Relaxed), 0);
    assert_eq!(handler_calls.load(Ordering::Relaxed), 0);
    assert_eq!(active_provider_calls.load(Ordering::Relaxed), 1);
    assert_eq!(unrelated_provider_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn cancellation_before_and_during_provider_is_observable() {
    let (token, handle) = cancellation_pair();
    handle.cancel();
    let command = Command::new("root").option(
        OptionSpec::value("value")
            .long("value")
            .completion_provider(
                |_cancellation: &nagi_cli::CancellationToken,
                 _request: &nagi_cli::CompletionRequest| {
                    panic!("provider ran after pre-cancellation")
                },
            ),
    );
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let error = engine
        .complete(&token, CompletionInput::new(["--value"], ""))
        .expect_err("completion must be cancelled");
    assert_eq!(error.kind(), CompletionErrorKind::Cancelled);

    let (during_token, during_handle) = cancellation_pair();
    let command = Command::new("root").option(
        OptionSpec::value("value")
            .long("value")
            .completion_provider(
                move |_cancellation: &nagi_cli::CancellationToken,
                      _request: &nagi_cli::CompletionRequest| {
                    during_handle.cancel();
                    Ok(vec![CompletionCandidate::new("value")])
                },
            ),
    );
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let error = engine
        .complete(&during_token, CompletionInput::new(["--value"], ""))
        .expect_err("completion must observe provider cancellation");
    assert_eq!(error.kind(), CompletionErrorKind::Cancelled);
}

#[test]
fn provider_and_candidate_errors_are_distinct() {
    let provider_command = Command::new("root").option(
        OptionSpec::value("value")
            .long("value")
            .completion_provider(
                |_cancellation: &nagi_cli::CancellationToken,
                 _request: &nagi_cli::CompletionRequest| {
                    Err(CompletionProviderError::new("provider unavailable"))
                },
            ),
    );
    let engine = CompletionEngine::new(&provider_command).expect("command must be valid");
    let error = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["--value"], ""),
        )
        .expect_err("provider must fail");
    assert_eq!(error.kind(), CompletionErrorKind::Provider);
    assert_eq!(error.message(), "provider unavailable");

    for candidate in [
        CompletionCandidate::new(""),
        CompletionCandidate::new("bad\nvalue"),
        CompletionCandidate::new("\u{0085}"),
    ] {
        let candidate = Arc::new(Mutex::new(Some(candidate)));
        let command = Command::new("root").option(
            OptionSpec::value("value")
                .long("value")
                .completion_provider({
                    let candidate = Arc::clone(&candidate);
                    move |_cancellation: &nagi_cli::CancellationToken,
                          _request: &nagi_cli::CompletionRequest| {
                        Ok(vec![
                            candidate
                                .lock()
                                .expect("candidate mutex must not be poisoned")
                                .take()
                                .expect("provider is called once"),
                        ])
                    }
                }),
        );
        let engine = CompletionEngine::new(&command).expect("command must be valid");
        let error = engine
            .complete(
                &nagi_cli::CancellationToken::new(),
                CompletionInput::new(["--value"], ""),
            )
            .expect_err("candidate must fail validation");
        assert_eq!(error.kind(), CompletionErrorKind::InvalidCandidate);
    }
}

#[test]
fn engine_owns_its_graph_snapshot() {
    let mut command = Command::new("root").subcommand(Command::new("first"));
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    command = command.subcommand(Command::new("second"));
    assert!(command.validate().is_ok());
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::default(),
        )
        .expect("completion must succeed");
    assert_eq!(
        result
            .candidates()
            .iter()
            .map(CompletionCandidate::value)
            .collect::<Vec<_>>(),
        ["first", "help", "--help", "-h"]
    );
}

#[test]
fn provider_is_value_only_configuration() {
    let command =
        Command::new("root").option(OptionSpec::flag("flag").long("flag").completion_provider(
            |_cancellation: &nagi_cli::CancellationToken,
             _request: &nagi_cli::CompletionRequest| Ok(Vec::new()),
        ));
    let error = command
        .validate()
        .expect_err("Flag provider must be invalid");
    assert_eq!(error.code(), DiagnosticCode::InvalidSpecification);
}

#[test]
fn help_path_is_reserved_only_when_the_root_has_subcommands() {
    let command = Command::new("root")
        .argument(Argument::new("value").parser(possible_values_parser(["help", "other"])));
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(Vec::<OsString>::new(), "h"),
        )
        .expect("completion must succeed");
    assert_eq!(result.candidates().len(), 1);
    assert_eq!(result.candidates()[0].value(), "help");
    assert_eq!(
        result.candidates()[0].kind(),
        nagi_cli::CompletionCandidateKind::Value
    );

    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["help"], ""),
        )
        .expect("literal help argument must be consumed");
    assert_eq!(result.request().partial_occurrences().len(), 1);
    assert_eq!(
        result.request().partial_occurrences()[0].raw(),
        Some(OsStr::new("help"))
    );
}

#[test]
fn empty_candidate_metadata_is_absent() {
    let candidate = CompletionCandidate::new("value")
        .with_display_label("")
        .with_description("");
    assert_eq!(candidate.display_label(), "value");
    assert_eq!(candidate.description(), None);
}
