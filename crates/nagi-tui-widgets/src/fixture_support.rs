#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Record {
    pub id: String,
    fields: BTreeMap<String, String>,
}

impl Record {
    pub fn field(&self, name: &str) -> &str {
        self.fields
            .get(name)
            .unwrap_or_else(|| panic!("case {} has no {name} field", self.id))
    }

    pub fn decoded(&self, name: &str) -> Vec<u8> {
        decoded_value(self.field(name))
            .unwrap_or_else(|reason| panic!("case {} has invalid {name} field: {reason}", self.id))
    }

    pub fn text(&self, name: &str) -> String {
        String::from_utf8(self.decoded(name))
            .unwrap_or_else(|error| panic!("case {} has non-UTF-8 {name} field: {error}", self.id))
    }
}

pub fn decoded_value(value: &str) -> Result<Vec<u8>, String> {
    decode(value)
}

pub fn text_value(value: &str) -> Result<String, String> {
    String::from_utf8(decode(value)?).map_err(|error| error.to_string())
}

pub fn load(relative: &str, suite: &str, allowed_fields: &[&str]) -> Option<Vec<Record>> {
    let root = fixture_root()?;
    let path = root.join(relative);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    Some(
        parse(&path, &input, suite, allowed_fields)
            .unwrap_or_else(|error| panic!("invalid fixture: {error}")),
    )
}

fn fixture_root() -> Option<PathBuf> {
    if let Some(root) = env::var_os("NAGI_FIXTURES") {
        return Some(PathBuf::from(root));
    }

    let integrated = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures");
    integrated.is_dir().then_some(integrated)
}

pub(super) fn parse(
    path: &Path,
    input: &str,
    suite: &str,
    allowed_fields: &[&str],
) -> Result<Vec<Record>, String> {
    let allowed: BTreeSet<&str> = allowed_fields.iter().copied().collect();
    let mut header_seen = false;
    let mut case_ids = BTreeSet::new();
    let mut records = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if !header_seen {
            if line != format!("nagi-fixture-v1\t{suite}") {
                return Err(at(path, line_number, "unsupported or mismatched header"));
            }
            header_seen = true;
            continue;
        }

        let mut parts = line.split('\t');
        let id = parts.next().unwrap_or_default();
        if id.is_empty()
            || !id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
            })
        {
            return Err(at(path, line_number, "invalid case identifier"));
        }
        if !case_ids.insert(id.to_owned()) {
            return Err(at(path, line_number, "duplicate case identifier"));
        }

        let mut fields = BTreeMap::new();
        for part in parts {
            let Some((name, value)) = part.split_once('=') else {
                return Err(at(path, line_number, "field has no equals sign"));
            };
            if !allowed.contains(name) {
                return Err(at(path, line_number, "unknown field"));
            }
            if fields.insert(name.to_owned(), value.to_owned()).is_some() {
                return Err(at(path, line_number, "duplicate field"));
            }
        }
        if fields.len() != allowed.len() {
            return Err(at(path, line_number, "missing field"));
        }
        records.push(Record {
            id: id.to_owned(),
            fields,
        });
    }

    if !header_seen {
        return Err(at(path, 1, "missing header"));
    }
    Ok(records)
}

fn at(path: &Path, line: usize, reason: &str) -> String {
    format!("{}:{line}: {reason}", path.display())
}

fn decode(value: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            let mut encoded = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            continue;
        }

        match chars.next().ok_or("incomplete escape")? {
            '\\' => output.push(b'\\'),
            't' => output.push(b'\t'),
            'n' => output.push(b'\n'),
            'r' => output.push(b'\r'),
            'x' => {
                let high = chars.next().ok_or("incomplete byte escape")?;
                let low = chars.next().ok_or("incomplete byte escape")?;
                output.push((hex(high)? << 4) | hex(low)?);
            }
            'u' => {
                if chars.next() != Some('{') {
                    return Err("Unicode escape has no opening brace".to_owned());
                }
                let mut digits = String::new();
                loop {
                    let next = chars.next().ok_or("incomplete Unicode escape")?;
                    if next == '}' {
                        break;
                    }
                    if digits.len() == 6 {
                        return Err("overlong Unicode escape".to_owned());
                    }
                    if !next.is_ascii_hexdigit() || next.is_ascii_lowercase() {
                        return Err("non-canonical Unicode escape".to_owned());
                    }
                    digits.push(next);
                }
                if digits.is_empty() {
                    return Err("empty Unicode escape".to_owned());
                }
                let scalar = u32::from_str_radix(&digits, 16)
                    .map_err(|_| "invalid Unicode escape".to_owned())?;
                let character = char::from_u32(scalar)
                    .ok_or_else(|| "invalid Unicode scalar value".to_owned())?;
                let mut encoded = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            unknown => {
                let mut reason = String::from("unknown escape ");
                let _ = reason.write_char(unknown);
                return Err(reason);
            }
        }
    }
    Ok(output)
}

fn hex(character: char) -> Result<u8, String> {
    if !character.is_ascii_hexdigit() || character.is_ascii_lowercase() {
        return Err("non-canonical hexadecimal digit".to_owned());
    }
    character
        .to_digit(16)
        .and_then(|digit| u8::try_from(digit).ok())
        .ok_or_else(|| "invalid hexadecimal digit".to_owned())
}
