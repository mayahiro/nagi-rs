//! Shell completion generators and a handler-free runtime protocol for Nagi CLI
//!
//! Generated scripts tokenize the active shell command line and invoke the
//! application with [`PROTOCOL_TOKEN`]. Applications call [`handle`] before
//! normal Command Graph parsing. A handled request only uses
//! [`nagi_cli::CompletionEngine`] and never executes a command handler

#![deny(missing_docs)]
#![deny(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, Write};

use nagi_cli::{
    CancellationToken, CompletionCandidate, CompletionCandidateKind, CompletionEngine,
    CompletionError, CompletionInput,
};

/// The reserved argv token used by generated completion scripts
///
/// It is outside the portable command-name grammar and is intended to be
/// intercepted before ordinary Command Graph parsing
pub const PROTOCOL_TOKEN: &str = "__nagi_complete";

/// A supported generated shell completion format
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Shell {
    /// GNU Bash programmable completion
    Bash,
    /// Zsh compsys completion
    Zsh,
    /// Fish command completion
    Fish,
    /// PowerShell native argument completion
    PowerShell,
}

impl Shell {
    /// Returns the stable protocol spelling
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
        }
    }

    fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" => Some(Self::PowerShell),
            _ => None,
        }
    }
}

/// Classifies a completion protocol failure
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorKind {
    /// The reserved request has missing or invalid protocol arguments
    InvalidRequest,
    /// Core completion resolution failed
    Completion,
    /// Candidate output could not be written
    Io,
}

/// A completion protocol parsing, resolution, or output failure
#[derive(Debug)]
pub struct ProtocolError {
    kind: ProtocolErrorKind,
    message: String,
    completion: Option<Box<CompletionError>>,
    io: Option<io::Error>,
}

impl ProtocolError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ProtocolErrorKind::InvalidRequest,
            message: message.into(),
            completion: None,
            io: None,
        }
    }

    fn completion(error: CompletionError) -> Self {
        Self {
            kind: ProtocolErrorKind::Completion,
            message: error.to_string(),
            completion: Some(Box::new(error)),
            io: None,
        }
    }

    fn io(error: io::Error) -> Self {
        Self {
            kind: ProtocolErrorKind::Io,
            message: error.to_string(),
            completion: None,
            io: Some(error),
        }
    }

    /// Returns the failure category
    pub const fn kind(&self) -> ProtocolErrorKind {
        self.kind
    }

    /// Returns the completion failure when resolution failed
    pub fn completion_error(&self) -> Option<&CompletionError> {
        self.completion.as_deref()
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "completion protocol failed: {}", self.message)
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.completion
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
            .or_else(|| {
                self.io
                    .as_ref()
                    .map(|error| error as &(dyn std::error::Error + 'static))
            })
    }
}

/// Generates one deterministic shell completion script
pub fn generate(shell: Shell, engine: &CompletionEngine) -> String {
    let command = engine.root_name();
    let function = command.replace('-', "_");
    match shell {
        Shell::Bash => generate_bash(command, &function),
        Shell::Zsh => generate_zsh(command, &function),
        Shell::Fish => generate_fish(command, &function),
        Shell::PowerShell => generate_power_shell(command),
    }
}

/// Handles a generated-script request before normal Command Graph parsing
///
/// The argument iterator excludes the program name. `Ok(false)` means the
/// reserved protocol token was absent and ordinary dispatch should continue
pub fn handle<I, S, W>(
    cancellation: &CancellationToken,
    engine: &CompletionEngine,
    arguments: I,
    output: &mut W,
) -> Result<bool, ProtocolError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
{
    let arguments: Vec<OsString> = arguments.into_iter().map(Into::into).collect();
    if arguments.first().map(OsString::as_os_str) != Some(OsStr::new(PROTOCOL_TOKEN)) {
        return Ok(false);
    }
    if arguments.len() < 3 {
        return Err(ProtocolError::invalid(
            "expected shell and current-token arguments",
        ));
    }
    let shell = Shell::parse(&arguments[1])
        .ok_or_else(|| ProtocolError::invalid("unsupported shell identifier"))?;
    let result = engine
        .complete(
            cancellation,
            CompletionInput::new(arguments[3..].iter().cloned(), arguments[2].clone()),
        )
        .map_err(ProtocolError::completion)?;
    write_protocol(shell, result.candidates(), output).map_err(ProtocolError::io)?;
    Ok(true)
}

fn write_protocol<W: Write>(
    shell: Shell,
    candidates: &[CompletionCandidate],
    output: &mut W,
) -> io::Result<()> {
    for candidate in candidates {
        let description = candidate
            .description()
            .unwrap_or_else(|| candidate.display_label());
        if shell == Shell::Fish {
            writeln!(output, "{}\t{description}", candidate.value())?;
            continue;
        }
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            candidate.value(),
            candidate.display_label(),
            description,
            candidate_kind(candidate.kind()),
            if candidate.append_space() {
                "space"
            } else {
                "none"
            }
        )?;
    }
    Ok(())
}

const fn candidate_kind(kind: CompletionCandidateKind) -> &'static str {
    match kind {
        CompletionCandidateKind::Command => "command",
        CompletionCandidateKind::Option => "option",
        CompletionCandidateKind::Value => "value",
    }
}

fn generate_bash(command: &str, function: &str) -> String {
    format!(
        r#"# Nagi completion for {command}
_nagi_completion_dequote_{function}() {{
    local LC_ALL=C
    local input="$1"
    local output=""
    local quote=""
    local character next
    local index=0
    local length=${{#input}}

    while (( index < length )); do
        character="${{input:index:1}}"
        if [[ -z "$quote" ]]; then
            case "$character" in
                "'") quote="single" ;;
                '"') quote="double" ;;
                \\)
                    if (( index + 1 < length )); then
                        ((index++))
                        output+="${{input:index:1}}"
                    else
                        output+="$character"
                    fi
                    ;;
                *) output+="$character" ;;
            esac
        elif [[ "$quote" == "single" ]]; then
            if [[ "$character" == "'" ]]; then
                quote=""
            else
                output+="$character"
            fi
        elif [[ "$character" == '"' ]]; then
            quote=""
        elif [[ "$character" == \\ ]] && (( index + 1 < length )); then
            next="${{input:index+1:1}}"
            if [[ "$next" == '$' || "$next" == $'\x60' || "$next" == '"' || "$next" == \\ ]]; then
                output+="$next"
                ((index++))
            elif [[ "$next" == $'\n' ]]; then
                ((index++))
            else
                output+="$character"
            fi
        else
            output+="$character"
        fi
        ((index++))
    done
    REPLY="$output"
    NAGI_COMPLETION_QUOTE_REPLY="$quote"
}}

_nagi_completion_escape_{function}() {{
    local input="$1"
    local quote="$2"
    local output=""
    local character
    local index=0

    if [[ -z "$quote" ]]; then
        printf -v output '%q' "$input"
    else
        while (( index < ${{#input}} )); do
            character="${{input:index:1}}"
            if [[ "$quote" == "double" && ( "$character" == '$' || "$character" == $'\x60' || "$character" == '"' || "$character" == \\ ) ]]; then
                output+="\\$character"
            elif [[ "$quote" == "single" && "$character" == "'" ]]; then
                output+="'\\''"
            else
                output+="$character"
            fi
            ((index++))
        done
    fi

    REPLY="$output"
}}

_nagi_completion_current_{function}() {{
    local LC_ALL=C
    local full="${{COMP_WORDS[COMP_CWORD]}}"
    local length=${{#full}}
    local point=$COMP_POINT
    local start=$(( point - length ))
    local raw="$full"

    (( start < 0 )) && start=0
    while (( start <= point )); do
        if (( point <= start + length )) && [[ "${{COMP_LINE:start:length}}" == "$full" ]]; then
            raw="${{COMP_LINE:start:point-start}}"
            break
        fi
        ((start++))
    done
    _nagi_completion_dequote_{function} "$raw"
}}

_nagi_completion_{function}() {{
    local current
    local -a completed=()
    local value label description kind append escaped
    local index
    local REPLY
    local NAGI_COMPLETION_QUOTE_REPLY
    local current_quote

    _nagi_completion_current_{function}
    current="$REPLY"
    current_quote="$NAGI_COMPLETION_QUOTE_REPLY"
    for (( index = 1; index < COMP_CWORD; index++ )); do
        _nagi_completion_dequote_{function} "${{COMP_WORDS[index]}}"
        completed+=("$REPLY")
    done

    COMPREPLY=()
    while IFS=$'\t' read -r value label description kind append; do
        [[ -z "$value" ]] && continue
        _nagi_completion_escape_{function} "$value" "$current_quote"
        escaped="$REPLY"
        COMPREPLY+=("$escaped")
    done < <(command {command} {PROTOCOL_TOKEN} bash "$current" "${{completed[@]}}" 2>/dev/null)
}}
complete -o nospace -F _nagi_completion_{function} {command}
"#
    )
}

fn generate_zsh(command: &str, function: &str) -> String {
    format!(
        r#"#compdef {command}
# Nagi completion for {command}
_nagi_completion_{function}() {{
    local current="$PREFIX"
    local -a completed=()
    local value label description kind append

    if (( CURRENT > 2 )); then
        completed=("${{(@)words[2,CURRENT-1]}}")
    fi

    while IFS=$'\t' read -r value label description kind append; do
        [[ -z "$value" ]] && continue
        if [[ "$append" == "space" ]]; then
            compadd -S ' ' -- "$value"
        else
            compadd -S '' -- "$value"
        fi
    done < <(command {command} {PROTOCOL_TOKEN} zsh "$current" "${{completed[@]}}" 2>/dev/null)
}}
compdef _nagi_completion_{function} {command}
"#
    )
}

fn generate_fish(command: &str, function: &str) -> String {
    format!(
        r#"# Nagi completion for {command}
function __nagi_completion_{function}
    set -l tokens (commandline -xpc)
    set -l current (commandline -ct)
    if test (count $tokens) -gt 0
        set -e tokens[1]
    end
    command {command} {PROTOCOL_TOKEN} fish "$current" $tokens 2>/dev/null
end
complete -c {command} -f -a '(__nagi_completion_{function})'
"#
    )
}

fn generate_power_shell(command: &str) -> String {
    format!(
        r#"# Nagi completion for {command}
Register-ArgumentCompleter -Native -CommandName '{command}' -ScriptBlock {{
    param($wordToComplete, $commandAst, $cursorPosition)

    $completed = @()
    $elements = @($commandAst.CommandElements)
    for ($index = 1; $index -lt $elements.Count; $index++) {{
        $element = $elements[$index]
        if ($element.Extent.StartOffset -ge $cursorPosition) {{ break }}
        if ($element.Extent.EndOffset -gt $cursorPosition) {{ break }}
        if ($wordToComplete -ne '' -and $element.Extent.EndOffset -eq $cursorPosition) {{ break }}
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {{
            $completed += [string]$element.Value
        }} else {{
            $completed += $element.Extent.Text
        }}
    }}

    $lines = & '{command}' '{PROTOCOL_TOKEN}' 'powershell' $wordToComplete @completed 2>$null
    foreach ($line in $lines) {{
        $fields = $line -split "`t", 5
        if ($fields.Count -ne 5 -or $fields[0] -eq '') {{ continue }}
        $completionText = $fields[0]
        if ($completionText -notmatch '^[a-zA-Z0-9_./:=+@%,-]+$') {{
            $completionText = "'" + $completionText.Replace("'", "''") + "'"
        }}
        if ($fields[4] -eq 'space') {{ $completionText += ' ' }}
        $listItem = if ($fields[1] -ne '') {{ $fields[1] }} else {{ $fields[0] }}
        $toolTip = if ($fields[2] -ne '') {{ $fields[2] }} else {{ $listItem }}
        $resultType = switch ($fields[3]) {{
            'command' {{ 'Command' }}
            'option' {{ 'ParameterName' }}
            default {{ 'ParameterValue' }}
        }}
        [System.Management.Automation.CompletionResult]::new(
            $completionText, $listItem, $resultType, $toolTip
        )
    }}
}}
"#
    )
}
