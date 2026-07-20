use std::fmt;
use std::sync::Arc;

/// A stable semantic identity for an interactive or stateful node
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(Arc<str>);

impl NodeId {
    /// Creates an identifier from an application-defined stable key
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::from(value.into()))
    }

    /// Returns the application-defined key
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(formatter)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}
