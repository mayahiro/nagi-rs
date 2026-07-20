use std::fmt;

/// A stable semantic identity for an interactive or stateful node
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(String);

impl NodeId {
    /// Creates an identifier from an application-defined stable key
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the application-defined key
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
