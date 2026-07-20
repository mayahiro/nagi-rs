use std::borrow::Cow;
use std::str;

/// Replaces each run of invalid UTF-8 byte sequences with one U+FFFD
///
/// Valid input is returned without allocation
#[must_use]
pub fn normalize_utf8(input: &[u8]) -> Cow<'_, str> {
    if let Ok(valid) = str::from_utf8(input) {
        return Cow::Borrowed(valid);
    }

    let mut output = String::with_capacity(input.len());
    let mut offset = 0_usize;
    while offset < input.len() {
        match str::from_utf8(&input[offset..]) {
            Ok(valid) => {
                output.push_str(valid);
                break;
            }
            Err(error) => {
                let valid_end = offset + error.valid_up_to();
                if valid_end > offset {
                    if let Ok(valid) = str::from_utf8(&input[offset..valid_end]) {
                        output.push_str(valid);
                    }
                    offset = valid_end;
                    continue;
                }

                output.push('\u{FFFD}');
                loop {
                    let error_length = str::from_utf8(&input[offset..]).map_or_else(
                        |invalid| invalid.error_len().unwrap_or(input.len() - offset),
                        |_| 0,
                    );
                    if error_length == 0 {
                        break;
                    }
                    offset = offset.saturating_add(error_length).min(input.len());
                    if offset == input.len() {
                        break;
                    }
                    match str::from_utf8(&input[offset..]) {
                        Ok(_) => break,
                        Err(next) if next.valid_up_to() != 0 => break,
                        Err(_) => {}
                    }
                }
            }
        }
    }
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::normalize_utf8;

    #[test]
    fn invalid_runs_get_one_replacement() {
        assert_eq!(normalize_utf8(b"\xFF\xFEA"), "\u{FFFD}A");
        assert_eq!(normalize_utf8(b"\xF0(\x8C\xBC"), "\u{FFFD}(\u{FFFD}");
    }
}
