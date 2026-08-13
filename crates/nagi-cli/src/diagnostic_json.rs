use crate::diagnostic::{Diagnostic, DiagnosticTarget, DiagnosticTargetKind};
use crate::policy::DiagnosticRenderer;

/// Stable schema identifier emitted by [`JsonDiagnosticRenderer`]
pub const JSON_DIAGNOSTIC_SCHEMA: &str = "nagi.cli.diagnostic.v1";

/// Renders one structured Diagnostic as a stable newline-delimited JSON object
///
/// The zero-sized renderer has no formatting options. Object member order,
/// string escaping, nullability, and the final newline are part of the public
/// format contract
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JsonDiagnosticRenderer;

impl DiagnosticRenderer for JsonDiagnosticRenderer {
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let mut output = String::with_capacity(estimated_capacity(diagnostic));
        output.push_str("{\"schema\":");
        push_json_string(&mut output, JSON_DIAGNOSTIC_SCHEMA);
        output.push_str(",\"code\":");
        push_json_string(&mut output, diagnostic.code().as_str());
        output.push_str(",\"category\":");
        push_json_string(&mut output, diagnostic.category().as_str());
        output.push_str(",\"message\":");
        push_json_string(&mut output, diagnostic.message());
        output.push_str(",\"command_path\":");
        push_string_array(&mut output, diagnostic.command_path());
        output.push_str(",\"usage\":");
        if let Some(usage) = diagnostic.usage() {
            push_json_string(&mut output, usage);
        } else {
            output.push_str("null");
        }
        output.push_str(",\"targets\":[");
        for (index, target) in diagnostic.targets().iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            push_target(&mut output, target);
        }
        output.push_str("],\"hints\":");
        push_string_array(&mut output, diagnostic.hints());
        output.push_str("}\n");
        output
    }
}

fn push_target(output: &mut String, target: &DiagnosticTarget) {
    output.push_str("{\"kind\":");
    push_json_string(
        output,
        match target.kind() {
            DiagnosticTargetKind::Option => "option",
            DiagnosticTargetKind::Argument => "argument",
        },
    );
    output.push_str(",\"command_id_path\":");
    push_string_array(output, target.command_id_path());
    output.push_str(",\"value_id\":");
    push_json_string(output, target.value_id());
    output.push('}');
}

fn push_string_array(output: &mut String, values: &[String]) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        push_json_string(output, value);
    }
    output.push(']');
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    let mut copied = 0;
    for (index, byte) in value.bytes().enumerate() {
        if byte > 0x1f && !matches!(byte, b'"' | b'\\') {
            continue;
        }
        output.push_str(&value[copied..index]);
        match byte {
            b'"' => output.push_str("\\\""),
            b'\\' => output.push_str("\\\\"),
            b'\x08' => output.push_str("\\b"),
            b'\t' => output.push_str("\\t"),
            b'\n' => output.push_str("\\n"),
            b'\x0c' => output.push_str("\\f"),
            b'\r' => output.push_str("\\r"),
            control => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                output.push_str("\\u00");
                output.push(char::from(HEX[usize::from(control >> 4)]));
                output.push(char::from(HEX[usize::from(control & 0x0f)]));
            }
        }
        copied = index + 1;
    }
    output.push_str(&value[copied..]);
    output.push('"');
}

fn estimated_capacity(diagnostic: &Diagnostic) -> usize {
    const STRUCTURE: usize = 160;
    let mut capacity = STRUCTURE
        .saturating_add(JSON_DIAGNOSTIC_SCHEMA.len())
        .saturating_add(diagnostic.code().as_str().len())
        .saturating_add(diagnostic.category().as_str().len())
        .saturating_add(diagnostic.message().len());
    for value in diagnostic.command_path() {
        capacity = capacity.saturating_add(value.len()).saturating_add(3);
    }
    if let Some(usage) = diagnostic.usage() {
        capacity = capacity.saturating_add(usage.len()).saturating_add(2);
    }
    for target in diagnostic.targets() {
        capacity = capacity
            .saturating_add(target.value_id().len())
            .saturating_add(64);
        for value in target.command_id_path() {
            capacity = capacity.saturating_add(value.len()).saturating_add(3);
        }
    }
    for hint in diagnostic.hints() {
        capacity = capacity.saturating_add(hint.len()).saturating_add(3);
    }
    capacity
}
