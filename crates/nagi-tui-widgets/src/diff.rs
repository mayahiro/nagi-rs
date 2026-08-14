use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use nagi_text::WidthProfile;

use crate::code::{
    CodeDocument, CodeDocumentError, CodeDocumentErrorKind, CodeDocumentLimits, CodeLayout,
    CodeLayoutError, CodeLayoutErrorKind, CodeLayoutLimits, CodeLayoutOptions, CodeLine,
    DEFAULT_CODE_DOCUMENT_MAX_LINES, DEFAULT_CODE_DOCUMENT_MAX_SPANS,
    DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES, DEFAULT_CODE_LAYOUT_TAB_WIDTH,
    DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH,
};

/// Default maximum logical line count in one diff document
pub const DEFAULT_DIFF_DOCUMENT_MAX_LINES: u64 = DEFAULT_CODE_DOCUMENT_MAX_LINES;

/// Default maximum styled span count in one diff document
pub const DEFAULT_DIFF_DOCUMENT_MAX_SPANS: u64 = DEFAULT_CODE_DOCUMENT_MAX_SPANS;

/// Default maximum conceptual unified UTF-8 byte count in one diff document
pub const DEFAULT_DIFF_DOCUMENT_MAX_TEXT_BYTES: u64 = DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES;

/// Stable semantic kind of one diff logical line
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DiffLineKind {
    /// Exact application-provided metadata such as a file header
    #[default]
    Metadata,
    /// Exact hunk header text carrying typed old and new ranges
    Hunk,
    /// A line present on both the old and new sides
    Context,
    /// A line present only on the new side
    Addition,
    /// A line present only on the old side
    Deletion,
}

impl DiffLineKind {
    /// Returns the stable language-independent identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Hunk => "hunk",
            Self::Context => "context",
            Self::Addition => "addition",
            Self::Deletion => "deletion",
        }
    }

    /// Returns the optional ASCII unified marker added during copy
    #[must_use]
    pub const fn marker(self) -> Option<char> {
        match self {
            Self::Metadata | Self::Hunk => None,
            Self::Context => Some(' '),
            Self::Addition => Some('+'),
            Self::Deletion => Some('-'),
        }
    }
}

impl fmt::Display for DiffLineKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Side carrying one semantic diff line number
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiffSide {
    /// The source before the change
    Old,
    /// The source after the change
    New,
}

impl DiffSide {
    /// Returns the stable language-independent identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

impl fmt::Display for DiffSide {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured non-positive diff line-number error
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidDiffLineNumber {
    side: DiffSide,
    value: u64,
}

impl InvalidDiffLineNumber {
    /// Returns the side containing the invalid number
    #[must_use]
    pub const fn side(&self) -> DiffSide {
        self.side
    }

    /// Returns the rejected line number
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

impl fmt::Display for InvalidDiffLineNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid {} diff line number {}",
            self.side, self.value
        )
    }
}

impl Error for InvalidDiffLineNumber {}

/// Stable category of an invalid diff range
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidDiffRangeKind {
    /// A non-empty range starts at zero
    Start,
    /// The inclusive last line would exceed `u64::MAX`
    Overflow,
}

impl InvalidDiffRangeKind {
    /// Returns the stable language-independent identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Overflow => "overflow",
        }
    }
}

impl fmt::Display for InvalidDiffRangeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured invalid hunk-range error
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidDiffRange {
    kind: InvalidDiffRangeKind,
    start: u64,
    count: u64,
}

impl InvalidDiffRange {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> InvalidDiffRangeKind {
        self.kind
    }

    /// Returns the rejected range start
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// Returns the rejected range line count
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }
}

impl fmt::Display for InvalidDiffRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid diff range {} start {} count {}",
            self.kind, self.start, self.count
        )
    }
}

impl Error for InvalidDiffRange {}

/// One validated source range declared by a diff hunk
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct DiffRange {
    start: u64,
    count: u64,
}

impl DiffRange {
    /// Creates a range after validating its inclusive last line
    ///
    /// An empty range may start at zero. A non-empty range must start at one
    /// or greater
    pub fn new(start: u64, count: u64) -> Result<Self, InvalidDiffRange> {
        if count > 0 && start == 0 {
            return Err(InvalidDiffRange {
                kind: InvalidDiffRangeKind::Start,
                start,
                count,
            });
        }
        if count > 0 && start.checked_add(count - 1).is_none() {
            return Err(InvalidDiffRange {
                kind: InvalidDiffRangeKind::Overflow,
                start,
                count,
            });
        }
        Ok(Self { start, count })
    }

    /// Returns the first declared line number
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Returns the declared line count
    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }

    /// Returns the inclusive last declared line or absence for an empty range
    #[must_use]
    pub const fn last(self) -> Option<u64> {
        if self.count == 0 {
            None
        } else {
            Some(self.start + (self.count - 1))
        }
    }
}

/// Typed old and new ranges attached to one hunk header
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct DiffHunk {
    old: DiffRange,
    new: DiffRange,
}

impl DiffHunk {
    /// Creates typed hunk metadata from validated ranges
    #[must_use]
    pub const fn new(old: DiffRange, new: DiffRange) -> Self {
        Self { old, new }
    }

    /// Returns the old-side range
    #[must_use]
    pub const fn old_range(self) -> DiffRange {
        self.old
    }

    /// Returns the new-side range
    #[must_use]
    pub const fn new_range(self) -> DiffRange {
        self.new
    }
}

/// One immutable typed diff line
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    kind: DiffLineKind,
    content: CodeLine,
    old_line: Option<u64>,
    new_line: Option<u64>,
    hunk: Option<DiffHunk>,
}

impl DiffLine {
    /// Creates exact metadata with no line number or unified marker
    #[must_use]
    pub fn metadata(content: CodeLine) -> Self {
        Self {
            kind: DiffLineKind::Metadata,
            content,
            old_line: None,
            new_line: None,
            hunk: None,
        }
    }

    /// Creates exact hunk header text carrying typed range metadata
    #[must_use]
    pub fn hunk(hunk: DiffHunk, content: CodeLine) -> Self {
        Self {
            kind: DiffLineKind::Hunk,
            content,
            old_line: None,
            new_line: None,
            hunk: Some(hunk),
        }
    }

    /// Creates one context line with positive old and new line numbers
    pub fn context(
        old_line: u64,
        new_line: u64,
        content: CodeLine,
    ) -> Result<Self, InvalidDiffLineNumber> {
        validate_line_number(DiffSide::Old, old_line)?;
        validate_line_number(DiffSide::New, new_line)?;
        Ok(Self {
            kind: DiffLineKind::Context,
            content,
            old_line: Some(old_line),
            new_line: Some(new_line),
            hunk: None,
        })
    }

    /// Creates one addition with a positive new-side line number
    pub fn addition(new_line: u64, content: CodeLine) -> Result<Self, InvalidDiffLineNumber> {
        validate_line_number(DiffSide::New, new_line)?;
        Ok(Self {
            kind: DiffLineKind::Addition,
            content,
            old_line: None,
            new_line: Some(new_line),
            hunk: None,
        })
    }

    /// Creates one deletion with a positive old-side line number
    pub fn deletion(old_line: u64, content: CodeLine) -> Result<Self, InvalidDiffLineNumber> {
        validate_line_number(DiffSide::Old, old_line)?;
        Ok(Self {
            kind: DiffLineKind::Deletion,
            content,
            old_line: Some(old_line),
            new_line: None,
            hunk: None,
        })
    }

    /// Returns the semantic line kind
    #[must_use]
    pub const fn kind(&self) -> DiffLineKind {
        self.kind
    }

    /// Returns the immutable styled content without a unified marker
    #[must_use]
    pub const fn content(&self) -> &CodeLine {
        &self.content
    }

    /// Returns the optional positive old-side line number
    #[must_use]
    pub const fn old_line(&self) -> Option<u64> {
        self.old_line
    }

    /// Returns the optional positive new-side line number
    #[must_use]
    pub const fn new_line(&self) -> Option<u64> {
        self.new_line
    }

    /// Returns typed hunk metadata only for a Hunk line
    #[must_use]
    pub const fn hunk_metadata(&self) -> Option<DiffHunk> {
        self.hunk
    }

    /// Reports whether copy callbacks may expose this line
    #[must_use]
    pub fn is_copyable(&self) -> bool {
        self.content.is_copyable()
    }
}

impl Default for DiffLine {
    fn default() -> Self {
        Self::metadata(CodeLine::default())
    }
}

fn validate_line_number(side: DiffSide, value: u64) -> Result<(), InvalidDiffLineNumber> {
    if value == 0 {
        Err(InvalidDiffLineNumber { side, value })
    } else {
        Ok(())
    }
}

/// Resource limits applied while constructing a [`DiffDocument`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DiffDocumentLimits {
    max_lines: u64,
    max_spans: u64,
    max_text_bytes: u64,
}

impl DiffDocumentLimits {
    /// Returns the maximum logical line count
    #[must_use]
    pub const fn max_lines(self) -> u64 {
        self.max_lines
    }

    /// Returns these limits with a maximum logical line count
    #[must_use]
    pub const fn with_max_lines(mut self, value: u64) -> Self {
        self.max_lines = default_u64(value, DEFAULT_DIFF_DOCUMENT_MAX_LINES);
        self
    }

    /// Returns the maximum styled span count
    #[must_use]
    pub const fn max_spans(self) -> u64 {
        self.max_spans
    }

    /// Returns these limits with a maximum styled span count
    #[must_use]
    pub const fn with_max_spans(mut self, value: u64) -> Self {
        self.max_spans = default_u64(value, DEFAULT_DIFF_DOCUMENT_MAX_SPANS);
        self
    }

    /// Returns the maximum conceptual unified UTF-8 byte count
    #[must_use]
    pub const fn max_text_bytes(self) -> u64 {
        self.max_text_bytes
    }

    /// Returns these limits with a maximum conceptual UTF-8 byte count
    #[must_use]
    pub const fn with_max_text_bytes(mut self, value: u64) -> Self {
        self.max_text_bytes = default_u64(value, DEFAULT_DIFF_DOCUMENT_MAX_TEXT_BYTES);
        self
    }
}

impl Default for DiffDocumentLimits {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_DIFF_DOCUMENT_MAX_LINES,
            max_spans: DEFAULT_DIFF_DOCUMENT_MAX_SPANS,
            max_text_bytes: DEFAULT_DIFF_DOCUMENT_MAX_TEXT_BYTES,
        }
    }
}

/// Stable category of a diff-document resource failure
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiffDocumentErrorKind {
    /// The logical line limit was exceeded
    LineLimit,
    /// The styled span limit was exceeded
    SpanLimit,
    /// The conceptual unified UTF-8 byte limit was exceeded
    TextByteLimit,
}

impl DiffDocumentErrorKind {
    /// Returns the stable language-independent identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LineLimit => "line-limit",
            Self::SpanLimit => "span-limit",
            Self::TextByteLimit => "text-byte-limit",
        }
    }
}

impl fmt::Display for DiffDocumentErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured diff-document resource failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffDocumentError {
    kind: DiffDocumentErrorKind,
    limit: u64,
    observed: u64,
}

impl DiffDocumentError {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> DiffDocumentErrorKind {
        self.kind
    }

    /// Returns the configured limit
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Returns the first observed value beyond the configured limit
    #[must_use]
    pub const fn observed(&self) -> u64 {
        self.observed
    }
}

impl fmt::Display for DiffDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "diff document {} exceeded limit {} with {}",
            self.kind, self.limit, self.observed
        )
    }
}

impl Error for DiffDocumentError {}

/// Immutable typed diff lines and conceptual unified source ranges
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffDocument {
    inner: Arc<DiffDocumentInner>,
}

#[derive(Debug, Eq, PartialEq)]
struct DiffDocumentInner {
    lines: Arc<[DiffLine]>,
    projection: CodeDocument,
    line_ranges: Arc<[Range<usize>]>,
    hidden_prefix: Arc<[u64]>,
    text_bytes: usize,
    trailing_newline: bool,
}

impl DiffDocument {
    /// Builds a diff document using bounded default limits
    pub fn new(
        lines: impl IntoIterator<Item = DiffLine>,
        trailing_newline: bool,
    ) -> Result<Self, DiffDocumentError> {
        Self::new_with_limits(lines, trailing_newline, DiffDocumentLimits::default())
    }

    /// Builds a diff document after validating every resource limit
    pub fn new_with_limits(
        source: impl IntoIterator<Item = DiffLine>,
        trailing_newline: bool,
        limits: DiffDocumentLimits,
    ) -> Result<Self, DiffDocumentError> {
        let mut lines = Vec::new();
        let mut projection_lines = Vec::new();
        let mut line_ranges = Vec::new();
        let mut hidden_prefix = vec![0_u64];
        let mut span_count = 0_u64;
        let mut text_bytes = 0_u64;
        for line in source {
            let line_count = u64::try_from(lines.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1);
            check_document_limit(
                DiffDocumentErrorKind::LineLimit,
                limits.max_lines,
                line_count,
            )?;
            span_count = span_count
                .saturating_add(u64::try_from(line.content().spans().len()).unwrap_or(u64::MAX));
            check_document_limit(
                DiffDocumentErrorKind::SpanLimit,
                limits.max_spans,
                span_count,
            )?;
            if !lines.is_empty() {
                text_bytes = text_bytes.saturating_add(1);
                check_document_limit(
                    DiffDocumentErrorKind::TextByteLimit,
                    limits.max_text_bytes,
                    text_bytes,
                )?;
            }
            let start = usize::try_from(text_bytes).unwrap_or(usize::MAX);
            if line.kind().marker().is_some() {
                text_bytes = text_bytes.saturating_add(1);
                check_document_limit(
                    DiffDocumentErrorKind::TextByteLimit,
                    limits.max_text_bytes,
                    text_bytes,
                )?;
            }
            text_bytes = text_bytes
                .saturating_add(u64::try_from(line.content().text().len()).unwrap_or(u64::MAX));
            check_document_limit(
                DiffDocumentErrorKind::TextByteLimit,
                limits.max_text_bytes,
                text_bytes,
            )?;
            let end = usize::try_from(text_bytes).unwrap_or(usize::MAX);
            line_ranges.push(start..end);
            let hidden = *hidden_prefix.last().unwrap_or(&0) + u64::from(!line.is_copyable());
            hidden_prefix.push(hidden);
            projection_lines.push(line.content().clone());
            lines.push(line);
        }
        if trailing_newline && !lines.is_empty() {
            text_bytes = text_bytes.saturating_add(1);
            check_document_limit(
                DiffDocumentErrorKind::TextByteLimit,
                limits.max_text_bytes,
                text_bytes,
            )?;
        }
        let code_limits = CodeDocumentLimits::default()
            .with_max_lines(limits.max_lines)
            .with_max_spans(limits.max_spans)
            .with_max_text_bytes(limits.max_text_bytes);
        let projection = CodeDocument::new_with_limits(projection_lines, false, code_limits)
            .map_err(map_code_document_error)?;
        let trailing_newline = trailing_newline && !lines.is_empty();
        Ok(Self {
            inner: Arc::new(DiffDocumentInner {
                lines: Arc::from(lines),
                projection,
                line_ranges: Arc::from(line_ranges),
                hidden_prefix: Arc::from(hidden_prefix),
                text_bytes: usize::try_from(text_bytes).unwrap_or(usize::MAX),
                trailing_newline,
            }),
        })
    }

    /// Returns the logical diff line count
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.inner.lines.len()
    }

    /// Returns one typed logical diff line by zero-based index
    #[must_use]
    pub fn line(&self, index: usize) -> Option<&DiffLine> {
        self.inner.lines.get(index)
    }

    /// Returns the conceptual complete unified UTF-8 byte count
    #[must_use]
    pub fn text_bytes(&self) -> usize {
        self.inner.text_bytes
    }

    /// Reports whether conceptual unified text ends with LF
    #[must_use]
    pub fn has_trailing_newline(&self) -> bool {
        self.inner.trailing_newline
    }

    /// Returns the conceptual UTF-8 byte range for ordered logical lines
    #[must_use]
    pub fn byte_range_for_lines(&self, lines: Range<usize>) -> Option<Range<usize>> {
        if lines.start > lines.end || lines.end > self.line_count() {
            return None;
        }
        if lines.is_empty() {
            let offset = self
                .inner
                .line_ranges
                .get(lines.start)
                .map_or(self.text_bytes(), |range| range.start);
            return Some(offset..offset);
        }
        let start = self.inner.line_ranges[lines.start].start;
        let end = if lines.end == self.line_count() && self.has_trailing_newline() {
            self.text_bytes()
        } else {
            self.inner.line_ranges[lines.end - 1].end
        };
        Some(start..end)
    }

    /// Reports whether every line in an ordered range may be copied
    #[must_use]
    pub fn is_range_copyable(&self, lines: Range<usize>) -> bool {
        lines.start <= lines.end
            && lines.end <= self.line_count()
            && self.inner.hidden_prefix[lines.start] == self.inner.hidden_prefix[lines.end]
    }

    /// Generates independently owned unified text for a copyable line range
    #[must_use]
    pub fn copy_text_for_lines(&self, lines: Range<usize>) -> Option<String> {
        if !self.is_range_copyable(lines.clone()) {
            return None;
        }
        let bytes = self.byte_range_for_lines(lines.clone())?;
        let mut output = String::with_capacity(bytes.len());
        for (relative, line) in self.inner.lines[lines.clone()].iter().enumerate() {
            if relative > 0 {
                output.push('\n');
            }
            if let Some(marker) = line.kind().marker() {
                output.push(marker);
            }
            output.push_str(line.content().text());
        }
        if !lines.is_empty() && lines.end == self.line_count() && self.has_trailing_newline() {
            output.push('\n');
        }
        debug_assert_eq!(output.len(), bytes.len());
        Some(output)
    }

    pub(crate) fn projection(&self) -> &CodeDocument {
        &self.inner.projection
    }

    fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Default for DiffDocument {
    fn default() -> Self {
        Self::new([], false).expect("an empty diff document is within default limits")
    }
}

fn map_code_document_error(error: CodeDocumentError) -> DiffDocumentError {
    let kind = match error.kind() {
        CodeDocumentErrorKind::LineLimit => DiffDocumentErrorKind::LineLimit,
        CodeDocumentErrorKind::SpanLimit => DiffDocumentErrorKind::SpanLimit,
        CodeDocumentErrorKind::TextByteLimit => DiffDocumentErrorKind::TextByteLimit,
    };
    DiffDocumentError {
        kind,
        limit: error.limit(),
        observed: error.observed(),
    }
}

fn check_document_limit(
    kind: DiffDocumentErrorKind,
    limit: u64,
    observed: u64,
) -> Result<(), DiffDocumentError> {
    if observed > limit {
        Err(DiffDocumentError {
            kind,
            limit,
            observed,
        })
    } else {
        Ok(())
    }
}

/// Terminal-dependent diff projection options
#[derive(Clone, Copy, Debug)]
pub struct DiffLayoutOptions {
    viewport_width: u32,
    tab_width: u8,
    wrap: bool,
    line_numbers: bool,
    width_profile: WidthProfile<'static>,
}

impl DiffLayoutOptions {
    /// Returns the total terminal viewport width
    #[must_use]
    pub const fn viewport_width(self) -> u32 {
        self.viewport_width
    }

    /// Returns these options with a positive total viewport width
    #[must_use]
    pub const fn with_viewport_width(mut self, value: u32) -> Self {
        self.viewport_width = if value == 0 {
            DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH
        } else {
            value
        };
        self
    }

    /// Returns the positive tab stop width
    #[must_use]
    pub const fn tab_width(self) -> u8 {
        self.tab_width
    }

    /// Returns these options with a positive tab stop width
    #[must_use]
    pub const fn with_tab_width(mut self, value: u8) -> Self {
        self.tab_width = if value == 0 {
            DEFAULT_CODE_LAYOUT_TAB_WIDTH
        } else {
            value
        };
        self
    }

    /// Reports whether content hard-wraps at grapheme boundaries
    #[must_use]
    pub const fn wraps(self) -> bool {
        self.wrap
    }

    /// Returns these options with hard wrapping enabled or disabled
    #[must_use]
    pub const fn with_wrap(mut self, value: bool) -> Self {
        self.wrap = value;
        self
    }

    /// Reports whether the full old and new line-number gutter is requested
    #[must_use]
    pub const fn shows_line_numbers(self) -> bool {
        self.line_numbers
    }

    /// Returns these options with old and new line numbers configured
    #[must_use]
    pub const fn with_line_numbers(mut self, value: bool) -> Self {
        self.line_numbers = value;
        self
    }

    /// Returns the terminal cell-width profile used for projection
    #[must_use]
    pub const fn width_profile(self) -> WidthProfile<'static> {
        self.width_profile
    }

    /// Returns these options with a replacement terminal WidthProfile
    #[must_use]
    pub const fn with_width_profile(mut self, value: WidthProfile<'static>) -> Self {
        self.width_profile = value;
        self
    }
}

impl Default for DiffLayoutOptions {
    fn default() -> Self {
        Self {
            viewport_width: DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH,
            tab_width: DEFAULT_CODE_LAYOUT_TAB_WIDTH,
            wrap: false,
            line_numbers: true,
            width_profile: WidthProfile::MODERN,
        }
    }
}

/// Diff layout limits are the CodeLayout visual-row and display-byte limits
pub type DiffLayoutLimits = CodeLayoutLimits;

/// Diff layout failures are CodeLayout projection resource failures
pub type DiffLayoutError = CodeLayoutError;

/// Diff layout failure categories are CodeLayout projection categories
pub type DiffLayoutErrorKind = CodeLayoutErrorKind;

/// Immutable terminal projection of one [`DiffDocument`]
#[derive(Clone, Debug)]
pub struct DiffLayout {
    inner: Rc<DiffLayoutInner>,
}

#[derive(Debug)]
struct DiffLayoutInner {
    document: DiffDocument,
    options: DiffLayoutOptions,
    code: CodeLayout,
    gutter_width: u32,
    old_number_width: u32,
    new_number_width: u32,
}

/// Single-entry memo for rebuilding terminal diff layouts from immutable views
#[derive(Default)]
pub struct DiffLayoutCache {
    entry: RefCell<Option<DiffLayoutCacheEntry>>,
}

struct DiffLayoutCacheEntry {
    key: u64,
    document: DiffDocument,
    limits: DiffLayoutLimits,
    layout: DiffLayout,
}

impl DiffLayoutCache {
    /// Returns a cached layout or projects one using bounded default limits
    ///
    /// The key must represent every option including Custom width behavior
    pub fn resolve(
        &self,
        key: u64,
        document: &DiffDocument,
        options: DiffLayoutOptions,
    ) -> Result<DiffLayout, DiffLayoutError> {
        self.resolve_with_limits(key, document, options, DiffLayoutLimits::default())
    }

    /// Returns a cached layout or projects one using explicit limits
    ///
    /// The key must represent every option including Custom width behavior
    pub fn resolve_with_limits(
        &self,
        key: u64,
        document: &DiffDocument,
        options: DiffLayoutOptions,
        limits: DiffLayoutLimits,
    ) -> Result<DiffLayout, DiffLayoutError> {
        if let Some(entry) = self.entry.borrow().as_ref()
            && entry.key == key
            && entry.document.shares_storage_with(document)
            && entry.limits == limits
        {
            return Ok(entry.layout.clone());
        }
        let layout = DiffLayout::new_with_limits(document.clone(), options, limits)?;
        *self.entry.borrow_mut() = Some(DiffLayoutCacheEntry {
            key,
            document: document.clone(),
            limits,
            layout: layout.clone(),
        });
        Ok(layout)
    }

    /// Removes the cached document and layout
    pub fn clear(&self) {
        *self.entry.borrow_mut() = None;
    }
}

impl DiffLayout {
    /// Projects a document using bounded default layout limits
    pub fn new(
        document: DiffDocument,
        options: DiffLayoutOptions,
    ) -> Result<Self, DiffLayoutError> {
        Self::new_with_limits(document, options, DiffLayoutLimits::default())
    }

    /// Projects a document after validating every layout resource limit
    pub fn new_with_limits(
        document: DiffDocument,
        options: DiffLayoutOptions,
        limits: DiffLayoutLimits,
    ) -> Result<Self, DiffLayoutError> {
        let (gutter_width, old_number_width, new_number_width) =
            diff_layout_widths(&document, options);
        let code_width = options.viewport_width.saturating_sub(gutter_width).max(1);
        let code_options = CodeLayoutOptions::default()
            .with_viewport_width(code_width)
            .with_tab_width(options.tab_width)
            .with_wrap(options.wrap)
            .with_line_numbers(false)
            .with_width_profile(options.width_profile);
        let code =
            CodeLayout::new_with_limits(document.projection().clone(), code_options, limits)?;
        Ok(Self {
            inner: Rc::new(DiffLayoutInner {
                document,
                options,
                code,
                gutter_width,
                old_number_width,
                new_number_width,
            }),
        })
    }

    /// Returns the immutable typed source document
    #[must_use]
    pub fn document(&self) -> &DiffDocument {
        &self.inner.document
    }

    /// Returns the terminal projection options
    #[must_use]
    pub fn options(&self) -> DiffLayoutOptions {
        self.inner.options
    }

    /// Returns the projected visual row count
    #[must_use]
    pub fn visual_row_count(&self) -> usize {
        self.inner.code.visual_row_count()
    }

    /// Returns the complete sticky gutter width
    #[must_use]
    pub fn gutter_width(&self) -> u32 {
        self.inner.gutter_width
    }

    /// Returns the displayed old-side number field width or zero
    #[must_use]
    pub fn old_number_width(&self) -> u32 {
        self.inner.old_number_width
    }

    /// Returns the displayed new-side number field width or zero
    #[must_use]
    pub fn new_number_width(&self) -> u32 {
        self.inner.new_number_width
    }

    /// Returns the content region width after reserving the sticky gutter
    #[must_use]
    pub fn code_width(&self) -> u32 {
        self.inner.code.code_width()
    }

    /// Returns the widest projected content row before horizontal cropping
    #[must_use]
    pub fn maximum_row_width(&self) -> u32 {
        self.inner.code.maximum_row_width()
    }

    pub(crate) fn code_layout(&self) -> &CodeLayout {
        &self.inner.code
    }
}

fn diff_layout_widths(document: &DiffDocument, options: DiffLayoutOptions) -> (u32, u32, u32) {
    let mut maximum_old = 0_u64;
    let mut maximum_new = 0_u64;
    for line in document.inner.lines.iter() {
        maximum_old = maximum_old.max(line.old_line().unwrap_or(0));
        maximum_new = maximum_new.max(line.new_line().unwrap_or(0));
        if let Some(hunk) = line.hunk_metadata() {
            maximum_old = maximum_old.max(hunk.old_range().last().unwrap_or(0));
            maximum_new = maximum_new.max(hunk.new_range().last().unwrap_or(0));
        }
    }
    let old_width = decimal_digits(maximum_old.max(1));
    let new_width = decimal_digits(maximum_new.max(1));
    let full = old_width.saturating_add(new_width).saturating_add(4);
    if options.line_numbers && options.viewport_width > full {
        (full, old_width, new_width)
    } else if options.viewport_width > 2 {
        (2, 0, 0)
    } else {
        (0, 0, 0)
    }
}

fn decimal_digits(mut value: u64) -> u32 {
    let mut digits = 1_u32;
    while value >= 10 {
        value /= 10;
        digits = digits.saturating_add(1);
    }
    digits
}

const fn default_u64(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use nagi_tui::{Style, TextSpan};

    use super::*;

    fn plain(value: &str) -> CodeLine {
        CodeLine::plain(value).unwrap()
    }

    fn sample_document() -> DiffDocument {
        let hunk = DiffHunk::new(DiffRange::new(1, 3).unwrap(), DiffRange::new(1, 4).unwrap());
        DiffDocument::new(
            [
                DiffLine::metadata(plain("diff --git a/a.txt b/a.txt")),
                DiffLine::hunk(hunk, plain("@@ -1,3 +1,4 @@")),
                DiffLine::context(1, 1, plain("same")).unwrap(),
                DiffLine::deletion(2, plain("old")).unwrap(),
                DiffLine::addition(2, plain("new")).unwrap(),
                DiffLine::addition(3, plain("more")).unwrap(),
                DiffLine::context(3, 4, plain("last")).unwrap(),
            ],
            true,
        )
        .unwrap()
    }

    #[test]
    fn range_and_line_constructors_reject_invalid_numbers() {
        let error = DiffRange::new(0, 1).unwrap_err();
        assert_eq!(error.kind(), InvalidDiffRangeKind::Start);
        let error = DiffRange::new(u64::MAX, 2).unwrap_err();
        assert_eq!(error.kind(), InvalidDiffRangeKind::Overflow);
        assert_eq!(DiffRange::new(0, 0).unwrap(), DiffRange::default());
        assert_eq!(DiffRange::new(u64::MAX, 1).unwrap().last(), Some(u64::MAX));

        let error = DiffLine::context(0, 1, plain("a")).unwrap_err();
        assert_eq!(error.side(), DiffSide::Old);
        let error = DiffLine::context(1, 0, plain("a")).unwrap_err();
        assert_eq!(error.side(), DiffSide::New);
        let error = DiffLine::addition(0, plain("a")).unwrap_err();
        assert_eq!(error.side(), DiffSide::New);
        let error = DiffLine::deletion(0, plain("a")).unwrap_err();
        assert_eq!(error.side(), DiffSide::Old);
    }

    #[test]
    fn document_copy_is_generated_with_exact_markers_and_ranges() {
        let document = sample_document();
        assert_eq!(document.text_bytes(), 71);
        assert_eq!(document.byte_range_for_lines(0..2), Some(0..42));
        assert_eq!(
            document.copy_text_for_lines(0..2).as_deref(),
            Some("diff --git a/a.txt b/a.txt\n@@ -1,3 +1,4 @@")
        );
        assert_eq!(document.byte_range_for_lines(2..7), Some(43..71));
        assert_eq!(
            document.copy_text_for_lines(2..7).as_deref(),
            Some(" same\n-old\n+new\n+more\n last\n")
        );
        assert_eq!(document.byte_range_for_lines(7..7), Some(71..71));
        assert_eq!(document.copy_text_for_lines(7..7).as_deref(), Some(""));
    }

    #[test]
    fn hidden_content_blocks_only_overlapping_copy_ranges() {
        let hidden = CodeLine::styled([TextSpan::new(
            "secret",
            Style {
                hidden: true,
                ..Style::default()
            },
        )])
        .unwrap();
        let document = DiffDocument::new(
            [
                DiffLine::addition(1, plain("public")).unwrap(),
                DiffLine::deletion(1, hidden).unwrap(),
            ],
            false,
        )
        .unwrap();
        assert!(document.is_range_copyable(0..1));
        assert!(!document.is_range_copyable(1..2));
        assert!(document.copy_text_for_lines(1..2).is_none());
    }

    #[test]
    fn limits_include_unified_markers_and_stop_consuming_input() {
        let consumed = Cell::new(0_u64);
        let source = (0_u64..3).map(|value| {
            consumed.set(value + 1);
            DiffLine::addition(value + 1, plain("a")).unwrap()
        });
        let error = DiffDocument::new_with_limits(
            source,
            false,
            DiffDocumentLimits::default().with_max_lines(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), DiffDocumentErrorKind::LineLimit);
        assert_eq!(error.observed(), 2);
        assert_eq!(consumed.get(), 2);

        let error = DiffDocument::new_with_limits(
            [DiffLine::metadata(
                CodeLine::styled([
                    TextSpan::new("a", Style::default()),
                    TextSpan::new("b", Style::default()),
                ])
                .unwrap(),
            )],
            false,
            DiffDocumentLimits::default().with_max_spans(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), DiffDocumentErrorKind::SpanLimit);
        assert_eq!(error.observed(), 2);

        let error = DiffDocument::new_with_limits(
            [DiffLine::addition(1, plain("a")).unwrap()],
            false,
            DiffDocumentLimits::default().with_max_text_bytes(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), DiffDocumentErrorKind::TextByteLimit);
        assert_eq!(error.observed(), 2);
    }

    #[test]
    fn layout_cache_uses_document_identity_key_and_limits() {
        let document = sample_document();
        let cache = DiffLayoutCache::default();
        let first = cache
            .resolve(7, &document, DiffLayoutOptions::default())
            .unwrap();
        let second = cache
            .resolve(7, &document, DiffLayoutOptions::default())
            .unwrap();
        assert!(Rc::ptr_eq(&first.inner, &second.inner));
        let changed = cache
            .resolve(8, &document, DiffLayoutOptions::default())
            .unwrap();
        assert!(!Rc::ptr_eq(&first.inner, &changed.inner));
        cache.clear();
        let cleared = cache
            .resolve(8, &document, DiffLayoutOptions::default())
            .unwrap();
        assert!(!Rc::ptr_eq(&changed.inner, &cleared.inner));
    }

    #[test]
    fn hunk_ranges_reserve_number_width_before_body_lines_exist() {
        let document = DiffDocument::new(
            [DiffLine::hunk(
                DiffHunk::new(
                    DiffRange::new(995, 10).unwrap(),
                    DiffRange::new(9_995, 10).unwrap(),
                ),
                plain("@@ pending @@"),
            )],
            false,
        )
        .unwrap();
        let layout = DiffLayout::new(
            document,
            DiffLayoutOptions::default().with_viewport_width(20),
        )
        .unwrap();
        assert_eq!(layout.old_number_width(), 4);
        assert_eq!(layout.new_number_width(), 5);
        assert_eq!(layout.gutter_width(), 13);
    }

    #[test]
    fn diff_document_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/diff-document.txt",
            "widget-diff-document",
            &[
                "operation",
                "line",
                "expected-kind",
                "expected-old",
                "expected-new",
                "expected-hunk",
                "start",
                "end",
                "expected-copy",
                "expected-bytes",
            ],
        ) else {
            return;
        };
        let document = sample_document();
        for record in records {
            match record.field("operation") {
                "line" => {
                    let line = document
                        .line(fixture_usize(record.field("line")))
                        .expect("fixture line");
                    assert_eq!(line.kind().as_str(), record.field("expected-kind"));
                    assert_eq!(optional_u64(line.old_line()), record.field("expected-old"));
                    assert_eq!(optional_u64(line.new_line()), record.field("expected-new"));
                    let hunk = line.hunk_metadata().map_or_else(
                        || "-".to_owned(),
                        |hunk| {
                            format!(
                                "{}:{}:{}:{}",
                                hunk.old_range().start(),
                                hunk.old_range().count(),
                                hunk.new_range().start(),
                                hunk.new_range().count()
                            )
                        },
                    );
                    assert_eq!(hunk, record.field("expected-hunk"));
                }
                "copy" => {
                    let lines =
                        fixture_usize(record.field("start"))..fixture_usize(record.field("end"));
                    let bytes = document
                        .byte_range_for_lines(lines.clone())
                        .expect("fixture range");
                    assert_eq!(
                        format!("{}:{}", bytes.start, bytes.end),
                        record.field("expected-bytes")
                    );
                    assert_eq!(
                        document.copy_text_for_lines(lines).expect("fixture copy"),
                        record.text("expected-copy")
                    );
                }
                value => panic!("case {} has unknown operation {value}", record.id),
            }
        }
    }

    #[test]
    fn diff_layout_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/diff-layout.txt",
            "widget-diff-layout",
            &[
                "kind",
                "input",
                "old",
                "new",
                "profile",
                "viewport",
                "tab",
                "wrap",
                "line-numbers",
                "expected-gutter",
                "expected-code-width",
                "expected-rows",
                "expected-widths",
                "expected-continuations",
            ],
        ) else {
            return;
        };
        for record in records {
            let content = plain(&record.text("input"));
            let line = match record.field("kind") {
                "context" => DiffLine::context(
                    fixture_u64(record.field("old")),
                    fixture_u64(record.field("new")),
                    content,
                )
                .unwrap(),
                "addition" => {
                    DiffLine::addition(fixture_u64(record.field("new")), content).unwrap()
                }
                "deletion" => {
                    DiffLine::deletion(fixture_u64(record.field("old")), content).unwrap()
                }
                value => panic!("case {} has unknown kind {value}", record.id),
            };
            let profile = match record.field("profile") {
                "modern" => WidthProfile::MODERN,
                "cjk" => WidthProfile::CJK,
                value => panic!("case {} has unknown profile {value}", record.id),
            };
            let document = DiffDocument::new([line], false).unwrap();
            let layout = DiffLayout::new(
                document,
                DiffLayoutOptions::default()
                    .with_viewport_width(fixture_u32(record.field("viewport")))
                    .with_tab_width(fixture_u8(record.field("tab")))
                    .with_wrap(fixture_bool(record.field("wrap")))
                    .with_line_numbers(fixture_bool(record.field("line-numbers")))
                    .with_width_profile(profile),
            )
            .unwrap();
            assert_eq!(
                layout.gutter_width(),
                fixture_u32(record.field("expected-gutter")),
                "case {} gutter",
                record.id
            );
            assert_eq!(
                layout.code_width(),
                fixture_u32(record.field("expected-code-width")),
                "case {} code width",
                record.id
            );
            let mut rows = Vec::new();
            let mut widths = Vec::new();
            let mut continuations = Vec::new();
            for index in 0..layout.visual_row_count() {
                let row = layout.code_layout().row(index).unwrap();
                rows.push(row.spans.iter().map(TextSpan::text).collect::<String>());
                widths.push(row.width.to_string());
                continuations.push(row.continuation.to_string());
            }
            assert_eq!(
                rows.join("|"),
                record.text("expected-rows"),
                "case {} rows",
                record.id
            );
            assert_eq!(
                widths.join(","),
                record.field("expected-widths"),
                "case {} widths",
                record.id
            );
            assert_eq!(
                continuations.join(","),
                record.field("expected-continuations"),
                "case {} continuations",
                record.id
            );
        }
    }

    fn optional_u64(value: Option<u64>) -> String {
        value.map_or_else(|| "-".to_owned(), |value| value.to_string())
    }

    fn fixture_u64(value: &str) -> u64 {
        value.parse().expect("fixture u64")
    }

    fn fixture_usize(value: &str) -> usize {
        value.parse().expect("fixture usize")
    }

    fn fixture_u32(value: &str) -> u32 {
        value.parse().expect("fixture u32")
    }

    fn fixture_u8(value: &str) -> u8 {
        value.parse().expect("fixture u8")
    }

    fn fixture_bool(value: &str) -> bool {
        value.parse().expect("fixture bool")
    }
}
