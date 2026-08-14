use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Default maximum JSON value occurrences accepted by one document
pub const DEFAULT_JSON_DOCUMENT_MAX_NODES: u64 = 100_000;
/// Default maximum one-based JSON value depth accepted by one document
pub const DEFAULT_JSON_DOCUMENT_MAX_DEPTH: u32 = 128;
/// Hard maximum JSON depth supported by the inspector Node backend
pub const MAX_JSON_DOCUMENT_DEPTH: u32 = 256;
/// Default maximum decoded UTF-8 bytes across strings and object keys
pub const DEFAULT_JSON_DOCUMENT_MAX_STRING_BYTES: u64 = 16 * 1024 * 1024;
/// Default maximum deterministic compact serialization size
pub const DEFAULT_JSON_DOCUMENT_MAX_SERIALIZED_BYTES: u64 = 32 * 1024 * 1024;

/// Closed JSON value category
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JsonKind {
    /// JSON null
    Null,
    /// JSON Boolean
    Boolean,
    /// JSON number token
    Number,
    /// JSON string
    String,
    /// Ordered JSON array
    Array,
    /// Ordered JSON object
    Object,
}

impl JsonKind {
    /// Returns the stable language-independent kind identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        }
    }
}

impl fmt::Display for JsonKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One validated JSON number token that preserves its spelling
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JsonNumber(Arc<str>);

impl JsonNumber {
    /// Validates and owns one JSON number token
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidJsonNumber> {
        let value = value.into();
        if let Err(offset) = validate_json_number(&value) {
            return Err(InvalidJsonNumber {
                offset,
                byte_length: value.len(),
            });
        }
        Ok(Self(Arc::from(value)))
    }

    /// Returns the validated token without numeric conversion
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned for a token outside the JSON number grammar
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidJsonNumber {
    offset: usize,
    byte_length: usize,
}

impl InvalidJsonNumber {
    /// Returns the first invalid or missing byte offset
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the rejected token byte length without retaining its contents
    #[must_use]
    pub const fn byte_length(&self) -> usize {
        self.byte_length
    }
}

impl fmt::Display for InvalidJsonNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid JSON number at byte {} of {}",
            self.offset, self.byte_length
        )
    }
}

impl Error for InvalidJsonNumber {}

/// One ordered JSON object member
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonMember {
    key: Arc<str>,
    value: JsonValue,
}

impl JsonMember {
    /// Creates one key-value member
    #[must_use]
    pub fn new(key: impl Into<String>, value: JsonValue) -> Self {
        Self {
            key: Arc::from(key.into()),
            value,
        }
    }

    /// Returns the member key
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the immutable member value
    #[must_use]
    pub const fn value(&self) -> &JsonValue {
        &self.value
    }
}

/// Error returned when one JSON object repeats a key
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateJsonKey {
    key: Arc<str>,
}

impl DuplicateJsonKey {
    /// Returns the duplicated key
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl fmt::Display for DuplicateJsonKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate JSON object key {}", self.key)
    }
}

impl Error for DuplicateJsonKey {}

/// Immutable typed JSON value
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonValue {
    inner: Arc<JsonValueData>,
}

#[derive(Debug, Eq, PartialEq)]
enum JsonValueData {
    Null,
    Boolean(bool),
    Number(JsonNumber),
    String(Arc<str>),
    Array(Arc<[JsonValue]>),
    Object(Arc<[JsonMember]>),
}

impl JsonValue {
    /// Creates JSON null
    #[must_use]
    pub fn null() -> Self {
        Self {
            inner: Arc::new(JsonValueData::Null),
        }
    }

    /// Creates a JSON Boolean
    #[must_use]
    pub fn boolean(value: bool) -> Self {
        Self {
            inner: Arc::new(JsonValueData::Boolean(value)),
        }
    }

    /// Creates a JSON number from a validated token
    #[must_use]
    pub fn number(value: JsonNumber) -> Self {
        Self {
            inner: Arc::new(JsonValueData::Number(value)),
        }
    }

    /// Creates a JSON string
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(JsonValueData::String(Arc::from(value.into()))),
        }
    }

    /// Creates an immutable ordered JSON array
    #[must_use]
    pub fn array(values: impl IntoIterator<Item = JsonValue>) -> Self {
        Self {
            inner: Arc::new(JsonValueData::Array(Arc::from(
                values.into_iter().collect::<Vec<_>>(),
            ))),
        }
    }

    /// Creates an immutable ordered JSON object and rejects duplicate keys
    pub fn object(members: impl IntoIterator<Item = JsonMember>) -> Result<Self, DuplicateJsonKey> {
        let members: Vec<JsonMember> = members.into_iter().collect();
        let mut seen = HashSet::with_capacity(members.len());
        for member in &members {
            if !seen.insert(member.key()) {
                return Err(DuplicateJsonKey {
                    key: Arc::clone(&member.key),
                });
            }
        }
        Ok(Self {
            inner: Arc::new(JsonValueData::Object(Arc::from(members))),
        })
    }

    /// Returns the closed value category
    #[must_use]
    pub fn kind(&self) -> JsonKind {
        match self.inner.as_ref() {
            JsonValueData::Null => JsonKind::Null,
            JsonValueData::Boolean(_) => JsonKind::Boolean,
            JsonValueData::Number(_) => JsonKind::Number,
            JsonValueData::String(_) => JsonKind::String,
            JsonValueData::Array(_) => JsonKind::Array,
            JsonValueData::Object(_) => JsonKind::Object,
        }
    }

    /// Returns the Boolean value when this is a Boolean
    #[must_use]
    pub fn as_boolean(&self) -> Option<bool> {
        match self.inner.as_ref() {
            JsonValueData::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the validated number token when this is a Number
    #[must_use]
    pub fn as_number(&self) -> Option<&JsonNumber> {
        match self.inner.as_ref() {
            JsonValueData::Number(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the decoded string when this is a String
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self.inner.as_ref() {
            JsonValueData::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the ordered values when this is an Array
    #[must_use]
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self.inner.as_ref() {
            JsonValueData::Array(values) => Some(values),
            _ => None,
        }
    }

    /// Returns the ordered members when this is an Object
    #[must_use]
    pub fn as_object(&self) -> Option<&[JsonMember]> {
        match self.inner.as_ref() {
            JsonValueData::Object(members) => Some(members),
            _ => None,
        }
    }
}

impl Default for JsonValue {
    fn default() -> Self {
        Self::null()
    }
}

/// Validated RFC 6901 JSON Pointer used as one document identity
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JsonPointer(Arc<str>);

impl JsonPointer {
    /// Returns the root pointer
    #[must_use]
    pub fn root() -> Self {
        Self::default()
    }

    /// Validates and owns one JSON Pointer
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidJsonPointer> {
        let value = value.into();
        validate_json_pointer(&value)?;
        Ok(Self(Arc::from(value)))
    }

    /// Returns the encoded pointer
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned for an invalid RFC 6901 pointer
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidJsonPointer {
    offset: usize,
}

impl InvalidJsonPointer {
    /// Returns the first invalid or missing byte offset
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for InvalidJsonPointer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid JSON Pointer at byte {}", self.offset)
    }
}

impl Error for InvalidJsonPointer {}

/// Bounded eager-work limits for one JsonDocument
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JsonDocumentLimits {
    max_nodes: u64,
    max_depth: u32,
    max_string_bytes: u64,
    max_serialized_bytes: u64,
}

impl JsonDocumentLimits {
    /// Returns the maximum JSON value occurrence count
    #[must_use]
    pub const fn max_nodes(&self) -> u64 {
        self.max_nodes
    }

    /// Returns these limits with a maximum JSON value occurrence count
    #[must_use]
    pub const fn with_max_nodes(mut self, value: u64) -> Self {
        self.max_nodes = default_u64(value, DEFAULT_JSON_DOCUMENT_MAX_NODES);
        self
    }

    /// Returns the maximum one-based JSON value depth
    #[must_use]
    pub const fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Returns these limits with a capped maximum one-based depth
    #[must_use]
    pub const fn with_max_depth(mut self, value: u32) -> Self {
        let value = if value == 0 {
            DEFAULT_JSON_DOCUMENT_MAX_DEPTH
        } else {
            value
        };
        self.max_depth = if value > MAX_JSON_DOCUMENT_DEPTH {
            MAX_JSON_DOCUMENT_DEPTH
        } else {
            value
        };
        self
    }

    /// Returns the maximum decoded bytes across strings and object keys
    #[must_use]
    pub const fn max_string_bytes(&self) -> u64 {
        self.max_string_bytes
    }

    /// Returns these limits with a maximum decoded string byte count
    #[must_use]
    pub const fn with_max_string_bytes(mut self, value: u64) -> Self {
        self.max_string_bytes = default_u64(value, DEFAULT_JSON_DOCUMENT_MAX_STRING_BYTES);
        self
    }

    /// Returns the maximum deterministic compact serialization byte count
    #[must_use]
    pub const fn max_serialized_bytes(&self) -> u64 {
        self.max_serialized_bytes
    }

    /// Returns these limits with a maximum serialized byte count
    #[must_use]
    pub const fn with_max_serialized_bytes(mut self, value: u64) -> Self {
        self.max_serialized_bytes = default_u64(value, DEFAULT_JSON_DOCUMENT_MAX_SERIALIZED_BYTES);
        self
    }
}

impl Default for JsonDocumentLimits {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_JSON_DOCUMENT_MAX_NODES,
            max_depth: DEFAULT_JSON_DOCUMENT_MAX_DEPTH,
            max_string_bytes: DEFAULT_JSON_DOCUMENT_MAX_STRING_BYTES,
            max_serialized_bytes: DEFAULT_JSON_DOCUMENT_MAX_SERIALIZED_BYTES,
        }
    }
}

const fn default_u64(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

/// Stable category of a JSON document resource failure
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JsonDocumentErrorKind {
    /// The value occurrence limit was exceeded
    NodeLimit,
    /// The one-based nesting depth limit was exceeded
    DepthLimit,
    /// The decoded string and key byte limit was exceeded
    StringByteLimit,
    /// The compact serialization byte limit was exceeded
    SerializedByteLimit,
}

impl JsonDocumentErrorKind {
    /// Returns the stable language-independent error identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NodeLimit => "node-limit",
            Self::DepthLimit => "depth-limit",
            Self::StringByteLimit => "string-byte-limit",
            Self::SerializedByteLimit => "serialized-byte-limit",
        }
    }
}

impl fmt::Display for JsonDocumentErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured JSON document resource failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonDocumentError {
    kind: JsonDocumentErrorKind,
    limit: u64,
    observed: u64,
}

impl JsonDocumentError {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> JsonDocumentErrorKind {
        self.kind
    }

    /// Returns the configured limit
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Returns the first observed value beyond the limit
    #[must_use]
    pub const fn observed(&self) -> u64 {
        self.observed
    }
}

impl fmt::Display for JsonDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "JSON document {}: limit {}, observed {}",
            self.kind, self.limit, self.observed
        )
    }
}

impl Error for JsonDocumentError {}

/// Immutable indexed JSON document with deterministic compact serialization
#[derive(Clone, Debug)]
pub struct JsonDocument {
    inner: Arc<JsonDocumentData>,
}

#[derive(Debug)]
struct JsonDocumentData {
    serialized: Arc<str>,
    nodes: Box<[JsonDocumentNode]>,
    positions: HashMap<JsonPointer, usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonDocumentNode {
    pub(crate) path: JsonPointer,
    pub(crate) kind: JsonKind,
    pub(crate) depth: u32,
    pub(crate) parent: Option<usize>,
    pub(crate) child_count: usize,
    pub(crate) subtree_end: usize,
    serialized_start: usize,
    serialized_end: usize,
    pub(crate) label: JsonNodeLabel,
    pub(crate) decoded_string: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub(crate) enum JsonNodeLabel {
    Root,
    ObjectKey(Arc<str>),
    ArrayIndex(usize),
}

impl JsonDocument {
    /// Builds a document using bounded default limits
    pub fn new(root: JsonValue) -> Result<Self, JsonDocumentError> {
        Self::with_limits(root, JsonDocumentLimits::default())
    }

    /// Builds a document after validating all configured resource limits
    pub fn with_limits(
        root: JsonValue,
        limits: JsonDocumentLimits,
    ) -> Result<Self, JsonDocumentError> {
        let validation = validate_document(&root, limits)?;
        let mut serialized = String::with_capacity(validation.serialized_bytes);
        let mut nodes = Vec::with_capacity(validation.nodes);
        build_document_node(
            &root,
            JsonNodeLabel::Root,
            None,
            0,
            JsonPointer::root(),
            &mut serialized,
            &mut nodes,
        );
        let mut positions = HashMap::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            let previous = positions.insert(node.path.clone(), index);
            debug_assert!(previous.is_none());
        }
        Ok(Self {
            inner: Arc::new(JsonDocumentData {
                serialized: Arc::from(serialized),
                nodes: nodes.into_boxed_slice(),
                positions,
            }),
        })
    }

    /// Returns the complete deterministic compact JSON serialization
    #[must_use]
    pub fn serialized(&self) -> &str {
        &self.inner.serialized
    }

    /// Returns the number of indexed JSON values
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.nodes.len()
    }

    /// Reports whether this document contains no indexed value
    ///
    /// A valid document always contains one root value
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.nodes.is_empty()
    }

    /// Returns the root JSON value
    #[must_use]
    pub fn root(&self) -> JsonNode<'_> {
        JsonNode {
            document: self,
            index: 0,
        }
    }

    /// Looks up one value by complete JSON Pointer
    #[must_use]
    pub fn get(&self, pointer: &JsonPointer) -> Option<JsonNode<'_>> {
        self.index_of(pointer).map(|index| JsonNode {
            document: self,
            index,
        })
    }

    /// Iterates every value in deterministic preorder
    #[must_use]
    pub fn nodes(&self) -> JsonNodes<'_> {
        JsonNodes {
            document: self,
            next: 0,
        }
    }

    pub(crate) fn index_of(&self, pointer: &JsonPointer) -> Option<usize> {
        self.inner.positions.get(pointer).copied()
    }

    pub(crate) fn node_at(&self, index: usize) -> JsonNode<'_> {
        JsonNode {
            document: self,
            index,
        }
    }

    pub(crate) fn record(&self, index: usize) -> &JsonDocumentNode {
        &self.inner.nodes[index]
    }

    #[cfg(test)]
    fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Borrowed read-only view of one indexed JSON value
#[derive(Clone, Copy, Debug)]
pub struct JsonNode<'document> {
    document: &'document JsonDocument,
    index: usize,
}

impl<'document> JsonNode<'document> {
    /// Returns the value category
    #[must_use]
    pub fn kind(self) -> JsonKind {
        self.record().kind
    }

    /// Returns the stable complete JSON Pointer
    #[must_use]
    pub fn path(self) -> &'document JsonPointer {
        &self.record().path
    }

    /// Returns the zero-based value depth
    #[must_use]
    pub fn depth(self) -> u32 {
        self.record().depth
    }

    /// Returns the direct child count
    #[must_use]
    pub fn child_count(self) -> usize {
        self.record().child_count
    }

    /// Returns this member's object key, if present
    #[must_use]
    pub fn key(self) -> Option<&'document str> {
        match &self.record().label {
            JsonNodeLabel::ObjectKey(key) => Some(key),
            JsonNodeLabel::Root | JsonNodeLabel::ArrayIndex(_) => None,
        }
    }

    /// Returns this member's array index, if present
    #[must_use]
    pub fn array_index(self) -> Option<usize> {
        match &self.record().label {
            JsonNodeLabel::ArrayIndex(index) => Some(*index),
            JsonNodeLabel::Root | JsonNodeLabel::ObjectKey(_) => None,
        }
    }

    /// Returns the complete compact JSON serialization of this value
    #[must_use]
    pub fn serialized(self) -> &'document str {
        let record = self.record();
        &self.document.inner.serialized[record.serialized_start..record.serialized_end]
    }

    fn record(self) -> &'document JsonDocumentNode {
        self.document.record(self.index)
    }
}

/// Preorder iterator over one immutable JSON document
pub struct JsonNodes<'document> {
    document: &'document JsonDocument,
    next: usize,
}

impl<'document> Iterator for JsonNodes<'document> {
    type Item = JsonNode<'document>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.document.len() {
            return None;
        }
        let node = self.document.node_at(self.next);
        self.next = self.next.saturating_add(1);
        Some(node)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.document.len().saturating_sub(self.next);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for JsonNodes<'_> {}

fn validate_json_number(value: &str) -> Result<(), usize> {
    let bytes = value.as_bytes();
    let mut index = 0;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    match bytes.get(index) {
        Some(b'0') => {
            index += 1;
            if bytes.get(index).is_some_and(u8::is_ascii_digit) {
                return Err(index);
            }
        }
        Some(b'1'..=b'9') => {
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        _ => return Err(index),
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return Err(index);
        }
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return Err(index);
        }
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if index == bytes.len() {
        Ok(())
    } else {
        Err(index)
    }
}

fn validate_json_pointer(value: &str) -> Result<(), InvalidJsonPointer> {
    if value.is_empty() {
        return Ok(());
    }
    if !value.starts_with('/') {
        return Err(InvalidJsonPointer { offset: 0 });
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'~' {
            index += 1;
            continue;
        }
        match bytes.get(index.saturating_add(1)) {
            Some(b'0' | b'1') => index += 2,
            _ => return Err(InvalidJsonPointer { offset: index }),
        }
    }
    Ok(())
}

struct JsonDocumentValidation {
    nodes: usize,
    serialized_bytes: usize,
}

fn validate_document(
    root: &JsonValue,
    limits: JsonDocumentLimits,
) -> Result<JsonDocumentValidation, JsonDocumentError> {
    let mut counters = JsonDocumentCounters::default();
    validate_document_value(root, 1, limits, &mut counters)?;
    Ok(JsonDocumentValidation {
        nodes: usize::try_from(counters.nodes).unwrap_or(usize::MAX),
        serialized_bytes: usize::try_from(counters.serialized_bytes).unwrap_or(usize::MAX),
    })
}

#[derive(Default)]
struct JsonDocumentCounters {
    nodes: u64,
    string_bytes: u64,
    serialized_bytes: u64,
}

fn validate_document_value(
    value: &JsonValue,
    depth: u32,
    limits: JsonDocumentLimits,
    counters: &mut JsonDocumentCounters,
) -> Result<(), JsonDocumentError> {
    counters.nodes = counters.nodes.saturating_add(1);
    check_limit(
        JsonDocumentErrorKind::NodeLimit,
        limits.max_nodes,
        counters.nodes,
    )?;
    check_limit(
        JsonDocumentErrorKind::DepthLimit,
        u64::from(limits.max_depth),
        u64::from(depth),
    )?;
    match value.inner.as_ref() {
        JsonValueData::Null => {
            counters.serialized_bytes = counters.serialized_bytes.saturating_add(4);
        }
        JsonValueData::Boolean(value) => {
            counters.serialized_bytes =
                counters
                    .serialized_bytes
                    .saturating_add(if *value { 4 } else { 5 });
        }
        JsonValueData::Number(value) => {
            counters.serialized_bytes = counters
                .serialized_bytes
                .saturating_add(to_u64(value.as_str().len()));
        }
        JsonValueData::String(value) => {
            counters.string_bytes = counters.string_bytes.saturating_add(to_u64(value.len()));
            counters.serialized_bytes = counters
                .serialized_bytes
                .saturating_add(json_string_serialized_bytes(value));
        }
        JsonValueData::Array(values) => {
            counters.serialized_bytes = counters
                .serialized_bytes
                .saturating_add(2)
                .saturating_add(to_u64(values.len().saturating_sub(1)));
        }
        JsonValueData::Object(members) => {
            counters.serialized_bytes = counters
                .serialized_bytes
                .saturating_add(2)
                .saturating_add(to_u64(members.len().saturating_sub(1)));
            for member in members.iter() {
                counters.string_bytes = counters
                    .string_bytes
                    .saturating_add(to_u64(member.key.len()));
                counters.serialized_bytes = counters
                    .serialized_bytes
                    .saturating_add(json_string_serialized_bytes(&member.key))
                    .saturating_add(1);
            }
        }
    }
    check_limit(
        JsonDocumentErrorKind::StringByteLimit,
        limits.max_string_bytes,
        counters.string_bytes,
    )?;
    check_limit(
        JsonDocumentErrorKind::SerializedByteLimit,
        limits.max_serialized_bytes,
        counters.serialized_bytes,
    )?;
    match value.inner.as_ref() {
        JsonValueData::Array(values) => {
            for child in values.iter() {
                validate_document_value(child, depth.saturating_add(1), limits, counters)?;
            }
        }
        JsonValueData::Object(members) => {
            for member in members.iter() {
                validate_document_value(&member.value, depth.saturating_add(1), limits, counters)?;
            }
        }
        JsonValueData::Null
        | JsonValueData::Boolean(_)
        | JsonValueData::Number(_)
        | JsonValueData::String(_) => {}
    }
    Ok(())
}

fn check_limit(
    kind: JsonDocumentErrorKind,
    limit: u64,
    observed: u64,
) -> Result<(), JsonDocumentError> {
    if observed > limit {
        Err(JsonDocumentError {
            kind,
            limit,
            observed,
        })
    } else {
        Ok(())
    }
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn json_string_serialized_bytes(value: &str) -> u64 {
    value.chars().fold(2_u64, |bytes, character| {
        bytes.saturating_add(match character {
            '"' | '\\' | '\u{0008}' | '\u{000C}' | '\n' | '\r' | '\t' => 2,
            '\u{0000}'..='\u{001F}' => 6,
            _ => character.len_utf8() as u64,
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn build_document_node(
    value: &JsonValue,
    label: JsonNodeLabel,
    parent: Option<usize>,
    depth: u32,
    path: JsonPointer,
    output: &mut String,
    nodes: &mut Vec<JsonDocumentNode>,
) {
    let index = nodes.len();
    let start = output.len();
    let kind = value.kind();
    let (child_count, decoded_string) = match value.inner.as_ref() {
        JsonValueData::String(value) => (0, Some(Arc::clone(value))),
        JsonValueData::Array(values) => (values.len(), None),
        JsonValueData::Object(members) => (members.len(), None),
        JsonValueData::Null | JsonValueData::Boolean(_) | JsonValueData::Number(_) => (0, None),
    };
    nodes.push(JsonDocumentNode {
        path: path.clone(),
        kind,
        depth,
        parent,
        child_count,
        subtree_end: index.saturating_add(1),
        serialized_start: start,
        serialized_end: start,
        label,
        decoded_string,
    });

    match value.inner.as_ref() {
        JsonValueData::Null => output.push_str("null"),
        JsonValueData::Boolean(value) => output.push_str(if *value { "true" } else { "false" }),
        JsonValueData::Number(value) => output.push_str(value.as_str()),
        JsonValueData::String(value) => push_json_string(output, value),
        JsonValueData::Array(values) => {
            output.push('[');
            for (child_index, child) in values.iter().enumerate() {
                if child_index != 0 {
                    output.push(',');
                }
                build_document_node(
                    child,
                    JsonNodeLabel::ArrayIndex(child_index),
                    Some(index),
                    depth.saturating_add(1),
                    array_child_path(&path, child_index),
                    output,
                    nodes,
                );
            }
            output.push(']');
        }
        JsonValueData::Object(members) => {
            output.push('{');
            for (member_index, member) in members.iter().enumerate() {
                if member_index != 0 {
                    output.push(',');
                }
                push_json_string(output, &member.key);
                output.push(':');
                build_document_node(
                    &member.value,
                    JsonNodeLabel::ObjectKey(Arc::clone(&member.key)),
                    Some(index),
                    depth.saturating_add(1),
                    object_child_path(&path, &member.key),
                    output,
                    nodes,
                );
            }
            output.push('}');
        }
    }
    nodes[index].serialized_end = output.len();
    nodes[index].subtree_end = nodes.len();
}

fn array_child_path(parent: &JsonPointer, index: usize) -> JsonPointer {
    JsonPointer(Arc::from(format!("{}/{index}", parent.as_str())))
}

fn object_child_path(parent: &JsonPointer, key: &str) -> JsonPointer {
    let mut path = String::with_capacity(parent.as_str().len().saturating_add(key.len() + 1));
    path.push_str(parent.as_str());
    path.push('/');
    for character in key.chars() {
        match character {
            '~' => path.push_str("~0"),
            '/' => path.push_str("~1"),
            _ => path.push(character),
        }
    }
    JsonPointer(Arc::from(path))
}

fn push_json_string(output: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000C}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0000}'..='\u{001F}' => {
                let value = u32::from(character) as usize;
                output.push_str("\\u00");
                output.push(char::from(HEX[(value >> 4) & 0x0F]));
                output.push(char::from(HEX[value & 0x0F]));
            }
            _ => output.push(character),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/json-document.txt",
            "widget-json-document",
            &[
                "arrangement",
                "max-nodes",
                "max-depth",
                "max-string-bytes",
                "max-serialized-bytes",
                "expected-json",
                "expected-paths",
                "expected-kinds",
                "expected-depths",
                "expected-children",
                "expected-error",
            ],
        ) else {
            return;
        };
        for record in records {
            let limits = JsonDocumentLimits::default()
                .with_max_nodes(number(record.field("max-nodes")))
                .with_max_depth(number(record.field("max-depth")))
                .with_max_string_bytes(number(record.field("max-string-bytes")))
                .with_max_serialized_bytes(number(record.field("max-serialized-bytes")));
            let result =
                JsonDocument::with_limits(arrangement(record.field("arrangement")), limits);
            if record.field("expected-error") != "-" {
                assert_eq!(
                    result.unwrap_err().kind().as_str(),
                    record.field("expected-error"),
                    "case {}",
                    record.id
                );
                continue;
            }
            let document = result.unwrap_or_else(|error| panic!("case {}: {error}", record.id));
            assert_eq!(
                document.serialized(),
                record.text("expected-json"),
                "case {}",
                record.id
            );
            let nodes: Vec<_> = document.nodes().collect();
            assert_eq!(
                nodes
                    .iter()
                    .map(|node| node.path().as_str())
                    .collect::<Vec<_>>(),
                record
                    .text("expected-paths")
                    .split('\n')
                    .collect::<Vec<_>>(),
                "case {} paths",
                record.id
            );
            assert_eq!(
                nodes
                    .iter()
                    .map(|node| node.kind().as_str())
                    .collect::<Vec<_>>(),
                record
                    .field("expected-kinds")
                    .split("\\n")
                    .collect::<Vec<_>>(),
                "case {} kinds",
                record.id
            );
            assert_eq!(
                nodes.iter().map(|node| node.depth()).collect::<Vec<_>>(),
                numbers(record.field("expected-depths")),
                "case {} depths",
                record.id
            );
            assert_eq!(
                nodes
                    .iter()
                    .map(|node| u32::try_from(node.child_count()).unwrap())
                    .collect::<Vec<_>>(),
                numbers(record.field("expected-children")),
                "case {} children",
                record.id
            );
        }
    }

    #[test]
    fn number_grammar_preserves_valid_tokens_and_rejects_invalid_forms() {
        for token in ["0", "-0", "12", "-12.50e+2", "1E-9"] {
            assert_eq!(JsonNumber::new(token).unwrap().as_str(), token);
        }
        for token in ["", "-", "01", "1.", ".1", "1e", "+1", "NaN"] {
            assert!(JsonNumber::new(token).is_err(), "{token}");
        }
    }

    #[test]
    fn pointer_validation_accepts_only_rfc_6901_escapes() {
        for pointer in ["", "/", "/a~0b~1c", "/0"] {
            assert_eq!(JsonPointer::new(pointer).unwrap().as_str(), pointer);
        }
        for pointer in ["a", "/~", "/~2"] {
            assert!(JsonPointer::new(pointer).is_err(), "{pointer}");
        }
    }

    #[test]
    fn object_rejects_duplicates_and_document_clones_share_storage() {
        let duplicate = JsonValue::object([
            JsonMember::new("same", JsonValue::null()),
            JsonMember::new("same", JsonValue::boolean(true)),
        ])
        .unwrap_err();
        assert_eq!(duplicate.key(), "same");

        let document = JsonDocument::new(JsonValue::array([JsonValue::null()])).unwrap();
        assert!(document.shares_storage(&document.clone()));
    }

    fn arrangement(name: &str) -> JsonValue {
        match name {
            "all-scalars" => JsonValue::object([
                JsonMember::new("text", JsonValue::string("A\n日")),
                JsonMember::new(
                    "number",
                    JsonValue::number(JsonNumber::new("-12.50e+2").unwrap()),
                ),
                JsonMember::new("truth", JsonValue::boolean(true)),
                JsonMember::new("nothing", JsonValue::null()),
            ])
            .unwrap(),
            "nested-paths" => JsonValue::object([JsonMember::new(
                "a/b~c",
                JsonValue::array([JsonValue::object([]).unwrap(), JsonValue::array([])]),
            )])
            .unwrap(),
            _ => panic!("unknown arrangement {name}"),
        }
    }

    fn number<T>(value: &str) -> T
    where
        T: std::str::FromStr,
        T::Err: fmt::Display,
    {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }

    fn numbers<T>(value: &str) -> Vec<T>
    where
        T: std::str::FromStr,
        T::Err: fmt::Display,
    {
        value.split(',').map(number).collect()
    }
}
