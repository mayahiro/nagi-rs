use std::error::Error;
use std::fmt;
use std::str;
use std::sync::Arc;

/// Stable reason that an identifier could not be constructed
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdentifierErrorKind {
    /// The identifier contained no bytes
    Empty,
    /// The identifier was not valid UTF-8
    InvalidUtf8,
    /// A portable token segment did not begin with an ASCII lowercase letter
    InvalidSegmentStart,
    /// A portable token contained a byte outside its grammar
    InvalidCharacter,
}

impl IdentifierErrorKind {
    /// Returns the stable specification name for this failure kind
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::InvalidUtf8 => "invalid-utf8",
            Self::InvalidSegmentStart => "invalid-segment-start",
            Self::InvalidCharacter => "invalid-character",
        }
    }
}

/// Error returned when a content identifier is invalid
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentifierError {
    kind: IdentifierErrorKind,
}

impl IdentifierError {
    const fn new(kind: IdentifierErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable reason for this failure
    #[must_use]
    pub const fn kind(self) -> IdentifierErrorKind {
        self.kind
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid content identifier: {}",
            self.kind.as_str()
        )
    }
}

impl Error for IdentifierError {}

/// Opaque stable identity for one element occurrence
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ElementId(Arc<str>);

impl ElementId {
    /// Creates a non-empty element ID from valid UTF-8
    pub fn new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(IdentifierError::new(IdentifierErrorKind::Empty));
        }
        Ok(Self(Arc::from(value)))
    }

    /// Creates a non-empty element ID without repairing invalid UTF-8
    pub fn from_bytes(value: &[u8]) -> Result<Self, IdentifierError> {
        let value = str::from_utf8(value)
            .map_err(|_| IdentifierError::new(IdentifierErrorKind::InvalidUtf8))?;
        Self::new(value)
    }

    /// Returns the opaque UTF-8 value
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ElementId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

macro_rules! portable_identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub struct $name(Arc<str>);

        impl $name {
            /// Creates an identifier using the portable content-token grammar
            pub fn new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
                let value = value.as_ref();
                validate_portable_token(value.as_bytes())?;
                Ok(Self(Arc::from(value)))
            }

            /// Creates an identifier without repairing invalid UTF-8
            pub fn from_bytes(value: &[u8]) -> Result<Self, IdentifierError> {
                let value = str::from_utf8(value)
                    .map_err(|_| IdentifierError::new(IdentifierErrorKind::InvalidUtf8))?;
                Self::new(value)
            }

            /// Returns the portable token
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

portable_identifier!(Role, "Open semantic role attached to a content element");
portable_identifier!(
    Class,
    "Opaque presentation-rule class attached to a content element"
);
portable_identifier!(
    AnnotationId,
    "Opaque application-resolved annotation identity"
);

fn validate_portable_token(value: &[u8]) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(IdentifierErrorKind::Empty));
    }

    let mut segment_start = true;
    for &byte in value {
        if segment_start {
            if byte.is_ascii_lowercase() {
                segment_start = false;
                continue;
            }
            return Err(IdentifierError::new(
                IdentifierErrorKind::InvalidSegmentStart,
            ));
        }

        if byte == b'.' {
            segment_start = true;
        } else if !byte.is_ascii_lowercase()
            && !byte.is_ascii_digit()
            && byte != b'-'
            && byte != b'_'
        {
            return Err(IdentifierError::new(IdentifierErrorKind::InvalidCharacter));
        }
    }

    if segment_start {
        return Err(IdentifierError::new(
            IdentifierErrorKind::InvalidSegmentStart,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{IdentifierErrorKind, Role};

    #[test]
    fn token_failure_kinds_are_deterministic() {
        assert_eq!(
            Role::new("").unwrap_err().kind(),
            IdentifierErrorKind::Empty
        );
        assert_eq!(
            Role::new("1role").unwrap_err().kind(),
            IdentifierErrorKind::InvalidSegmentStart
        );
        assert_eq!(
            Role::new("roLe").unwrap_err().kind(),
            IdentifierErrorKind::InvalidCharacter
        );
    }
}
