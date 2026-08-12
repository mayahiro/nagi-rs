#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
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

    pub fn bytes(&self, name: &str) -> Vec<u8> {
        decode(self.field(name))
            .unwrap_or_else(|reason| panic!("case {} has invalid {name}: {reason}", self.id))
    }

    pub fn text(&self, name: &str) -> String {
        String::from_utf8(self.bytes(name))
            .unwrap_or_else(|error| panic!("case {} has non-UTF-8 {name}: {error}", self.id))
    }
}

pub fn load(relative: &str, suite: &str, allowed_fields: &[&str]) -> Vec<Record> {
    let root = env::var_os("NAGI_FIXTURES")
        .map(PathBuf::from)
        .or_else(|| {
            let integrated = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures");
            integrated.is_dir().then_some(integrated)
        })
        .expect("NAGI_FIXTURES is not configured and integrated fixtures are absent");
    let path = root.join(relative);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    parse(&path, &input, suite, allowed_fields)
        .unwrap_or_else(|error| panic!("invalid fixture: {error}"))
}

fn parse(
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
                return Err(format!(
                    "{}:{line_number}: unsupported or mismatched header",
                    path.display()
                ));
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
            || !case_ids.insert(id.to_owned())
        {
            return Err(format!(
                "{}:{line_number}: invalid or duplicate case identifier",
                path.display()
            ));
        }
        let mut fields = BTreeMap::new();
        for part in parts {
            let Some((name, value)) = part.split_once('=') else {
                return Err(format!(
                    "{}:{line_number}: field has no equals sign",
                    path.display()
                ));
            };
            if !allowed.contains(name) || fields.insert(name.to_owned(), value.to_owned()).is_some()
            {
                return Err(format!(
                    "{}:{line_number}: unknown or duplicate field",
                    path.display()
                ));
            }
        }
        if fields.len() != allowed.len() {
            return Err(format!("{}:{line_number}: missing field", path.display()));
        }
        records.push(Record {
            id: id.to_owned(),
            fields,
        });
    }
    if !header_seen {
        return Err(format!("{}:1: missing header", path.display()));
    }
    Ok(records)
}

fn decode(value: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            let mut encoded = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            continue;
        }
        match characters.next().ok_or("incomplete escape")? {
            '\\' => output.push(b'\\'),
            't' => output.push(b'\t'),
            'n' => output.push(b'\n'),
            'r' => output.push(b'\r'),
            'x' => {
                let high = characters.next().ok_or("incomplete byte escape")?;
                let low = characters.next().ok_or("incomplete byte escape")?;
                output.push((hex(high)? << 4) | hex(low)?);
            }
            unknown => return Err(format!("unknown escape {unknown}")),
        }
    }
    Ok(output)
}

fn hex(character: char) -> Result<u8, String> {
    character
        .to_digit(16)
        .filter(|_| !character.is_ascii_lowercase())
        .and_then(|digit| u8::try_from(digit).ok())
        .ok_or_else(|| "non-canonical hexadecimal digit".to_owned())
}
