use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticTarget, display_os};

const DEFAULT_MAX_DEPTH: usize = 16;
const DEFAULT_MAX_SOURCES: usize = 64;
const DEFAULT_MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_TOKENS: usize = 65_536;
const DEFAULT_MAX_TOKEN_BYTES: usize = 8 * 1024 * 1024;

/// Finite resource limits for one Response File expansion
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseFileLimits {
    max_depth: usize,
    max_sources: usize,
    max_source_bytes: usize,
    max_tokens: usize,
    max_token_bytes: usize,
}

impl ResponseFileLimits {
    /// Returns a copy with the active include-depth limit replaced
    pub const fn with_max_depth(mut self, value: usize) -> Self {
        self.max_depth = value;
        self
    }

    /// Returns a copy with the source-read count limit replaced
    pub const fn with_max_sources(mut self, value: usize) -> Self {
        self.max_sources = value;
        self
    }

    /// Returns a copy with the aggregate source-byte limit replaced
    pub const fn with_max_source_bytes(mut self, value: usize) -> Self {
        self.max_source_bytes = value;
        self
    }

    /// Returns a copy with the examined-token count limit replaced
    pub const fn with_max_tokens(mut self, value: usize) -> Self {
        self.max_tokens = value;
        self
    }

    /// Returns a copy with the aggregate token-byte limit replaced
    pub const fn with_max_token_bytes(mut self, value: usize) -> Self {
        self.max_token_bytes = value;
        self
    }

    /// Returns the active include-depth limit
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }

    /// Returns the source-read count limit
    pub const fn max_sources(self) -> usize {
        self.max_sources
    }

    /// Returns the aggregate source-byte limit
    pub const fn max_source_bytes(self) -> usize {
        self.max_source_bytes
    }

    /// Returns the examined-token count limit
    pub const fn max_tokens(self) -> usize {
        self.max_tokens
    }

    /// Returns the aggregate token-byte limit
    pub const fn max_token_bytes(self) -> usize {
        self.max_token_bytes
    }
}

impl Default for ResponseFileLimits {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_sources: DEFAULT_MAX_SOURCES,
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_tokens: DEFAULT_MAX_TOKENS,
            max_token_bytes: DEFAULT_MAX_TOKEN_BYTES,
        }
    }
}

/// Opt-in behavior for one Response File expansion
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResponseFileOptions {
    limits: ResponseFileLimits,
    standard_input: bool,
}

impl ResponseFileOptions {
    /// Returns a copy using the provided resource limits
    pub const fn with_limits(mut self, limits: ResponseFileLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Returns a copy that enables or disables exact `@-` expansion
    pub const fn with_standard_input(mut self, enabled: bool) -> Self {
        self.standard_input = enabled;
        self
    }

    /// Returns the configured resource limits
    pub const fn limits(self) -> ResponseFileLimits {
        self.limits
    }

    /// Reports whether exact `@-` expansion is enabled
    pub const fn standard_input_enabled(self) -> bool {
        self.standard_input
    }
}

/// One bounded read request sent to an injected Response File reader
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseFileReadRequest<'a> {
    path: &'a Path,
    read_limit: usize,
}

impl<'a> ResponseFileReadRequest<'a> {
    fn new(path: &'a Path, read_limit: usize) -> Self {
        Self { path, read_limit }
    }

    /// Returns the lexically resolved platform-native file path
    pub const fn path(&self) -> &'a Path {
        self.path
    }

    /// Returns the maximum bytes needed by the expander
    ///
    /// The value is one greater than the remaining accepted byte count when
    /// representable, allowing a reader to report a limit crossing without
    /// loading the rest of the source
    pub const fn read_limit(&self) -> usize {
        self.read_limit
    }
}

/// Reads Response File bytes for an injected or real filesystem
pub trait ResponseFileReader {
    /// Reads at most the requested bytes or returns a structured failure
    fn read(&mut self, request: &ResponseFileReadRequest<'_>) -> Result<Vec<u8>, Diagnostic>;
}

impl<F> ResponseFileReader for F
where
    F: FnMut(&ResponseFileReadRequest<'_>) -> Result<Vec<u8>, Diagnostic>,
{
    fn read(&mut self, request: &ResponseFileReadRequest<'_>) -> Result<Vec<u8>, Diagnostic> {
        self(request)
    }
}

/// A stateless Response File reader backed by the process filesystem
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FilesystemResponseFileReader;

impl ResponseFileReader for FilesystemResponseFileReader {
    fn read(&mut self, request: &ResponseFileReadRequest<'_>) -> Result<Vec<u8>, Diagnostic> {
        let file = File::open(request.path()).map_err(|_| response_file_io())?;
        read_bounded(file, request.read_limit()).map_err(|_| response_file_io())
    }
}

/// Expands opt-in `@file` arguments through injected file and standard input
/// readers
///
/// Expansion is independent of a Command Graph. Returned arguments retain
/// platform-native bytes and can be passed to [`crate::Command::parse`] or
/// [`crate::Command::run`]
pub fn expand_response_files<I, S>(
    arguments: I,
    base_directory: impl AsRef<Path>,
    options: &ResponseFileOptions,
    reader: &mut dyn ResponseFileReader,
    standard_input: &mut dyn Read,
) -> Result<Vec<OsString>, Diagnostic>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    Expansion::new(base_directory.as_ref(), *options, reader, standard_input)
        .expand(arguments.into_iter().map(Into::into).collect())
}

struct Expansion<'a> {
    options: ResponseFileOptions,
    reader: &'a mut dyn ResponseFileReader,
    standard_input: &'a mut dyn Read,
    initial_base: Arc<PathBuf>,
    active_files: BTreeSet<PathBuf>,
    standard_input_consumed: bool,
    source_count: usize,
    source_bytes: usize,
    token_count: usize,
    token_bytes: usize,
}

enum Work {
    Token {
        value: OsString,
        base: Arc<PathBuf>,
        source_depth: usize,
        source_reference: Option<Arc<str>>,
        budget_counted: bool,
    },
    EndFile(PathBuf),
}

impl<'a> Expansion<'a> {
    fn new(
        base_directory: &Path,
        options: ResponseFileOptions,
        reader: &'a mut dyn ResponseFileReader,
        standard_input: &'a mut dyn Read,
    ) -> Self {
        Self {
            options,
            reader,
            standard_input,
            initial_base: Arc::new(lexical_clean(base_directory)),
            active_files: BTreeSet::new(),
            standard_input_consumed: false,
            source_count: 0,
            source_bytes: 0,
            token_count: 0,
            token_bytes: 0,
        }
    }

    fn expand(&mut self, arguments: Vec<OsString>) -> Result<Vec<OsString>, Diagnostic> {
        let mut work = Vec::with_capacity(arguments.len());
        for value in arguments.into_iter().rev() {
            work.push(Work::Token {
                value,
                base: Arc::clone(&self.initial_base),
                source_depth: 0,
                source_reference: None,
                budget_counted: false,
            });
        }
        let mut output = Vec::new();
        while let Some(item) = work.pop() {
            match item {
                Work::EndFile(path) => {
                    self.active_files.remove(&path);
                }
                Work::Token {
                    value,
                    base,
                    source_depth,
                    source_reference,
                    budget_counted,
                } => {
                    if !budget_counted {
                        self.record_token(value.as_bytes().len(), source_reference.as_deref())?;
                    }
                    let bytes = value.as_bytes();
                    if bytes.starts_with(b"@@") {
                        output.push(OsString::from_vec(bytes[1..].to_vec()));
                    } else if bytes == b"@" || !bytes.starts_with(b"@") {
                        output.push(value);
                    } else {
                        self.include(
                            OsStr::from_bytes(&bytes[1..]),
                            &base,
                            source_depth,
                            &mut work,
                        )?;
                    }
                }
            }
        }
        Ok(output)
    }

    fn include(
        &mut self,
        reference: &OsStr,
        base: &Arc<PathBuf>,
        source_depth: usize,
        work: &mut Vec<Work>,
    ) -> Result<(), Diagnostic> {
        let reference_display: Arc<str> = Arc::from(display_os(reference));
        let depth = source_depth.saturating_add(1);
        if depth > self.options.limits.max_depth {
            return Err(response_file_error(
                DiagnosticCode::ResponseFileLimit,
                "response file include depth exceeds limit",
                &reference_display,
            ));
        }

        if reference.as_bytes() == b"-" {
            if !self.options.standard_input {
                return Err(response_file_error(
                    DiagnosticCode::ResponseFileStdin,
                    "response file standard input is disabled",
                    &reference_display,
                ));
            }
            if self.standard_input_consumed {
                return Err(response_file_error(
                    DiagnosticCode::ResponseFileStdin,
                    "response file standard input was already consumed",
                    &reference_display,
                ));
            }
            self.standard_input_consumed = true;
            self.reserve_source(&reference_display)?;
            let bytes = self.read_standard_input(&reference_display)?;
            let tokens = self.tokenize_source(&bytes, &reference_display)?;
            self.push_tokens(
                tokens,
                Arc::clone(&self.initial_base),
                depth,
                reference_display,
                work,
            );
            return Ok(());
        }

        let path = resolve_path(base, Path::new(reference));
        if self.active_files.contains(&path) {
            return Err(response_file_error(
                DiagnosticCode::ResponseFileCycle,
                "response file include cycle detected",
                &reference_display,
            ));
        }
        self.reserve_source(&reference_display)?;
        let bytes = self.read_file(&path, &reference_display)?;
        let tokens = self.tokenize_source(&bytes, &reference_display)?;
        let child_base = Arc::new(
            path.parent()
                .map_or_else(|| (*base.as_ref()).clone(), Path::to_path_buf),
        );
        self.active_files.insert(path.clone());
        work.push(Work::EndFile(path));
        self.push_tokens(tokens, child_base, depth, reference_display, work);
        Ok(())
    }

    fn reserve_source(&mut self, reference: &str) -> Result<(), Diagnostic> {
        if self.source_count >= self.options.limits.max_sources {
            return Err(response_file_error(
                DiagnosticCode::ResponseFileLimit,
                "response file source count exceeds limit",
                reference,
            ));
        }
        self.source_count += 1;
        Ok(())
    }

    fn read_file(&mut self, path: &Path, reference: &str) -> Result<Vec<u8>, Diagnostic> {
        let read_limit = self.next_read_limit();
        let request = ResponseFileReadRequest::new(path, read_limit);
        match self.reader.read(&request) {
            Ok(bytes) => self.accept_source_bytes(bytes, reference),
            Err(diagnostic) if diagnostic.targets().is_empty() => {
                Err(diagnostic.with_target(DiagnosticTarget::response_file(reference)))
            }
            Err(diagnostic) => Err(diagnostic),
        }
    }

    fn read_standard_input(&mut self, reference: &str) -> Result<Vec<u8>, Diagnostic> {
        let read_limit = self.next_read_limit();
        match read_bounded(&mut self.standard_input, read_limit) {
            Ok(bytes) => self.accept_source_bytes(bytes, reference),
            Err(_) => Err(response_file_error(
                DiagnosticCode::ResponseFileIo,
                "could not read response file",
                reference,
            )),
        }
    }

    fn next_read_limit(&self) -> usize {
        self.options
            .limits
            .max_source_bytes
            .saturating_sub(self.source_bytes)
            .saturating_add(1)
    }

    fn accept_source_bytes(
        &mut self,
        bytes: Vec<u8>,
        reference: &str,
    ) -> Result<Vec<u8>, Diagnostic> {
        let remaining = self
            .options
            .limits
            .max_source_bytes
            .saturating_sub(self.source_bytes);
        if bytes.len() > remaining {
            return Err(response_file_error(
                DiagnosticCode::ResponseFileLimit,
                "response file source bytes exceed limit",
                reference,
            ));
        }
        self.source_bytes += bytes.len();
        Ok(bytes)
    }

    fn tokenize_source(
        &mut self,
        bytes: &[u8],
        reference: &str,
    ) -> Result<Vec<OsString>, Diagnostic> {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        let text = std::str::from_utf8(bytes).map_err(|_| {
            response_file_error(
                DiagnosticCode::ResponseFileEncoding,
                "response file is not valid UTF-8",
                reference,
            )
        })?;
        tokenize(
            text,
            reference,
            &mut self.token_count,
            &mut self.token_bytes,
            self.options.limits,
        )
    }

    fn push_tokens(
        &self,
        tokens: Vec<OsString>,
        base: Arc<PathBuf>,
        source_depth: usize,
        source_reference: Arc<str>,
        work: &mut Vec<Work>,
    ) {
        for value in tokens.into_iter().rev() {
            work.push(Work::Token {
                value,
                base: Arc::clone(&base),
                source_depth,
                source_reference: Some(Arc::clone(&source_reference)),
                budget_counted: true,
            });
        }
    }

    fn record_token(
        &mut self,
        byte_count: usize,
        reference: Option<&str>,
    ) -> Result<(), Diagnostic> {
        record_token_budget(
            byte_count,
            reference,
            &mut self.token_count,
            &mut self.token_bytes,
            self.options.limits,
        )
    }
}

fn tokenize(
    text: &str,
    reference: &str,
    token_count: &mut usize,
    token_bytes: &mut usize,
    limits: ResponseFileLimits,
) -> Result<Vec<OsString>, Diagnostic> {
    let mut cursor = TextCursor::new(text);
    let mut tokens = Vec::new();
    let mut token = Vec::new();
    let mut started = false;
    let mut quote: Option<(Quote, usize, usize)> = None;

    while let Some((character, line, column)) = cursor.next() {
        if let Some((kind, _, _)) = quote {
            match (kind, character) {
                (Quote::Single, '\'') | (Quote::Double, '"') => quote = None,
                (Quote::Double, '\\') => {
                    let Some((escaped, escape_line, escape_column)) = cursor.next() else {
                        return Err(syntax_error(
                            reference,
                            "response file has a trailing escape",
                            line,
                            column,
                        ));
                    };
                    push_character(
                        &mut token,
                        escaped,
                        reference,
                        escape_line,
                        escape_column,
                        *token_bytes,
                        limits,
                    )?;
                }
                _ => push_character(
                    &mut token,
                    character,
                    reference,
                    line,
                    column,
                    *token_bytes,
                    limits,
                )?,
            }
            continue;
        }

        if ascii_separator(character) {
            if started {
                push_token(
                    &mut tokens,
                    std::mem::take(&mut token),
                    reference,
                    token_count,
                    token_bytes,
                    limits,
                )?;
                started = false;
            }
            continue;
        }
        if character == '#' && !started {
            while let Some((comment, comment_line, comment_column)) = cursor.next() {
                if comment == '\0' {
                    return Err(syntax_error(
                        reference,
                        "response file contains U+0000",
                        comment_line,
                        comment_column,
                    ));
                }
                if comment == '\n' {
                    break;
                }
            }
            continue;
        }
        match character {
            '\'' => {
                ensure_token_slot(started, reference, *token_count, limits)?;
                started = true;
                quote = Some((Quote::Single, line, column));
            }
            '"' => {
                ensure_token_slot(started, reference, *token_count, limits)?;
                started = true;
                quote = Some((Quote::Double, line, column));
            }
            '\\' => {
                ensure_token_slot(started, reference, *token_count, limits)?;
                started = true;
                let Some((escaped, escape_line, escape_column)) = cursor.next() else {
                    return Err(syntax_error(
                        reference,
                        "response file has a trailing escape",
                        line,
                        column,
                    ));
                };
                push_character(
                    &mut token,
                    escaped,
                    reference,
                    escape_line,
                    escape_column,
                    *token_bytes,
                    limits,
                )?;
            }
            _ => {
                ensure_token_slot(started, reference, *token_count, limits)?;
                started = true;
                push_character(
                    &mut token,
                    character,
                    reference,
                    line,
                    column,
                    *token_bytes,
                    limits,
                )?;
            }
        }
    }

    if let Some((kind, line, column)) = quote {
        let message = match kind {
            Quote::Single => "response file has an unterminated single quote",
            Quote::Double => "response file has an unterminated double quote",
        };
        return Err(syntax_error(reference, message, line, column));
    }
    if started {
        push_token(
            &mut tokens,
            token,
            reference,
            token_count,
            token_bytes,
            limits,
        )?;
    }
    Ok(tokens)
}

fn push_token(
    tokens: &mut Vec<OsString>,
    token: Vec<u8>,
    reference: &str,
    token_count: &mut usize,
    token_bytes: &mut usize,
    limits: ResponseFileLimits,
) -> Result<(), Diagnostic> {
    record_token_budget(
        token.len(),
        Some(reference),
        token_count,
        token_bytes,
        limits,
    )?;
    tokens.push(OsString::from_vec(token));
    Ok(())
}

fn record_token_budget(
    byte_count: usize,
    reference: Option<&str>,
    token_count: &mut usize,
    token_bytes: &mut usize,
    limits: ResponseFileLimits,
) -> Result<(), Diagnostic> {
    if *token_count >= limits.max_tokens {
        return Err(limit_error(
            "response file token count exceeds limit",
            reference,
        ));
    }
    if byte_count > limits.max_token_bytes.saturating_sub(*token_bytes) {
        return Err(limit_error(
            "response file token bytes exceed limit",
            reference,
        ));
    }
    *token_count += 1;
    *token_bytes += byte_count;
    Ok(())
}

fn push_character(
    token: &mut Vec<u8>,
    character: char,
    reference: &str,
    line: usize,
    column: usize,
    completed_token_bytes: usize,
    limits: ResponseFileLimits,
) -> Result<(), Diagnostic> {
    if character == '\0' {
        return Err(syntax_error(
            reference,
            "response file contains U+0000",
            line,
            column,
        ));
    }
    let mut encoded = [0; 4];
    let encoded = character.encode_utf8(&mut encoded).as_bytes();
    let remaining = limits
        .max_token_bytes
        .saturating_sub(completed_token_bytes)
        .saturating_sub(token.len());
    if encoded.len() > remaining {
        return Err(response_file_error(
            DiagnosticCode::ResponseFileLimit,
            "response file token bytes exceed limit",
            reference,
        ));
    }
    token.extend_from_slice(encoded);
    Ok(())
}

fn ensure_token_slot(
    started: bool,
    reference: &str,
    token_count: usize,
    limits: ResponseFileLimits,
) -> Result<(), Diagnostic> {
    if !started && token_count >= limits.max_tokens {
        return Err(response_file_error(
            DiagnosticCode::ResponseFileLimit,
            "response file token count exceeds limit",
            reference,
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Quote {
    Single,
    Double,
}

#[derive(Clone, Copy)]
struct TextCursor<'a> {
    text: &'a str,
    index: usize,
    line: usize,
    column: usize,
}

impl<'a> TextCursor<'a> {
    const fn new(text: &'a str) -> Self {
        Self {
            text,
            index: 0,
            line: 1,
            column: 1,
        }
    }

    fn next(&mut self) -> Option<(char, usize, usize)> {
        let character = self.text[self.index..].chars().next()?;
        let line = self.line;
        let column = self.column;
        self.index += character.len_utf8();
        if character == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some((character, line, column))
    }
}

const fn ascii_separator(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\r' | '\n' | '\u{000b}' | '\u{000c}'
    )
}

fn syntax_error(reference: &str, message: &str, line: usize, column: usize) -> Diagnostic {
    response_file_error(
        DiagnosticCode::ResponseFileSyntax,
        format!("{message} at line {line}, column {column}"),
        reference,
    )
}

fn limit_error(message: &str, reference: Option<&str>) -> Diagnostic {
    let diagnostic = Diagnostic::new(DiagnosticCode::ResponseFileLimit, message);
    match reference {
        Some(reference) => diagnostic.with_target(DiagnosticTarget::response_file(reference)),
        None => diagnostic,
    }
}

fn response_file_error(
    code: DiagnosticCode,
    message: impl Into<String>,
    reference: &str,
) -> Diagnostic {
    Diagnostic::new(code, message).with_target(DiagnosticTarget::response_file(reference))
}

fn response_file_io() -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ResponseFileIo,
        "could not read response file",
    )
}

fn read_bounded(reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn resolve_path(base: &Path, reference: &Path) -> PathBuf {
    if reference.is_absolute() {
        lexical_clean(reference)
    } else {
        lexical_clean(&base.join(reference))
    }
}

fn lexical_clean(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let absolute = path.is_absolute();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = result
                    .file_name()
                    .is_some_and(|value| value != OsStr::new(".."));
                if can_pop {
                    result.pop();
                } else if !absolute {
                    result.push("..");
                }
            }
            Component::Normal(value) => result.push(value),
        }
    }
    if result.as_os_str().is_empty() {
        result.push(if absolute { "/" } else { "." });
    }
    result
}
