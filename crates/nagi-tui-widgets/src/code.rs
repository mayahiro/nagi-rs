use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use nagi_text::{WidthProfile, grapheme_width, graphemes, is_grapheme_boundary, text_width};
use nagi_tui::{Style, TextSpan};

/// Default maximum logical line count in one code document
pub const DEFAULT_CODE_DOCUMENT_MAX_LINES: u64 = 100_000;

/// Default maximum styled span count in one code document
pub const DEFAULT_CODE_DOCUMENT_MAX_SPANS: u64 = 1_000_000;

/// Default maximum semantic UTF-8 byte count in one code document
pub const DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES: u64 = 32 * 1024 * 1024;

/// Default maximum visual row count in one terminal code layout
pub const DEFAULT_CODE_LAYOUT_MAX_VISUAL_ROWS: u64 = 1_000_000;

/// Default maximum expanded display byte count in one terminal code layout
pub const DEFAULT_CODE_LAYOUT_MAX_DISPLAY_BYTES: u64 = 64 * 1024 * 1024;

/// Default terminal viewport width used by [`CodeLayoutOptions`]
pub const DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH: u32 = 80;

/// Default tab stop width used by [`CodeLayoutOptions`]
pub const DEFAULT_CODE_LAYOUT_TAB_WIDTH: u8 = 4;

/// Stable category of an invalid styled code line
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidCodeLineKind {
    /// A CR or LF was supplied inside a logical line
    LineBreak,
    /// A style boundary split one extended grapheme cluster
    GraphemeBoundary,
}

impl InvalidCodeLineKind {
    /// Returns the stable language-independent error identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LineBreak => "line-break",
            Self::GraphemeBoundary => "grapheme-boundary",
        }
    }
}

impl fmt::Display for InvalidCodeLineKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured invalid-line error
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidCodeLine {
    kind: InvalidCodeLineKind,
    span_index: usize,
    byte_offset: usize,
}

impl InvalidCodeLine {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> InvalidCodeLineKind {
        self.kind
    }

    /// Returns the zero-based offending span index
    #[must_use]
    pub const fn span_index(&self) -> usize {
        self.span_index
    }

    /// Returns the UTF-8 byte offset in the complete logical line
    #[must_use]
    pub const fn byte_offset(&self) -> usize {
        self.byte_offset
    }
}

impl fmt::Display for InvalidCodeLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid code line {} at span {} byte {}",
            self.kind, self.span_index, self.byte_offset
        )
    }
}

impl Error for InvalidCodeLine {}

/// One immutable logical line made from ordered styled spans
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLine {
    inner: Arc<CodeLineInner>,
}

#[derive(Debug, Eq, PartialEq)]
struct CodeLineInner {
    spans: Arc<[TextSpan]>,
    concatenated: Option<Arc<str>>,
    copyable: bool,
}

impl CodeLine {
    /// Creates one default-style logical line
    ///
    /// CR and LF are rejected because line boundaries belong to
    /// [`CodeDocument`]
    pub fn plain(text: impl Into<String>) -> Result<Self, InvalidCodeLine> {
        Self::styled([TextSpan::new(text, Style::default())])
    }

    /// Creates one logical line from ordered styled spans
    ///
    /// CR and LF are rejected. Every span boundary must also be an extended
    /// grapheme boundary so one terminal cell never carries conflicting styles
    pub fn styled(spans: impl IntoIterator<Item = TextSpan>) -> Result<Self, InvalidCodeLine> {
        let spans: Vec<TextSpan> = spans.into_iter().collect();
        let mut text = String::new();
        let mut boundaries = Vec::with_capacity(spans.len().saturating_sub(1));
        for (span_index, span) in spans.iter().enumerate() {
            if let Some(relative) = span.text().find(['\r', '\n']) {
                return Err(InvalidCodeLine {
                    kind: InvalidCodeLineKind::LineBreak,
                    span_index,
                    byte_offset: text.len() + relative,
                });
            }
            text.push_str(span.text());
            if span_index + 1 < spans.len() {
                boundaries.push((span_index, text.len()));
            }
        }
        for (span_index, boundary) in boundaries {
            if !is_grapheme_boundary(&text, boundary) {
                return Err(InvalidCodeLine {
                    kind: InvalidCodeLineKind::GraphemeBoundary,
                    span_index,
                    byte_offset: boundary,
                });
            }
        }
        let concatenated = (spans.len() != 1).then(|| Arc::<str>::from(text));
        let copyable = spans.iter().all(|span| !span.style().hidden);
        Ok(Self {
            inner: Arc::new(CodeLineInner {
                spans: Arc::from(spans),
                concatenated,
                copyable,
            }),
        })
    }

    /// Returns the semantic UTF-8 line text without a line terminator
    #[must_use]
    pub fn text(&self) -> &str {
        self.inner.concatenated.as_deref().unwrap_or_else(|| {
            self.inner
                .spans
                .first()
                .map_or("", nagi_tui::TextSpan::text)
        })
    }

    /// Returns the immutable styled runs in source order
    #[must_use]
    pub fn spans(&self) -> &[TextSpan] {
        &self.inner.spans
    }

    fn shared_spans(&self) -> Arc<[TextSpan]> {
        Arc::clone(&self.inner.spans)
    }

    /// Reports whether copy callbacks may expose this line
    #[must_use]
    pub fn is_copyable(&self) -> bool {
        self.inner.copyable
    }
}

impl Default for CodeLine {
    fn default() -> Self {
        Self::styled([]).expect("an empty line is valid")
    }
}

/// Resource limits applied while constructing a [`CodeDocument`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CodeDocumentLimits {
    max_lines: u64,
    max_spans: u64,
    max_text_bytes: u64,
}

impl CodeDocumentLimits {
    /// Returns the maximum logical line count
    #[must_use]
    pub const fn max_lines(self) -> u64 {
        self.max_lines
    }

    /// Returns these limits with a maximum logical line count
    #[must_use]
    pub const fn with_max_lines(mut self, value: u64) -> Self {
        self.max_lines = default_u64(value, DEFAULT_CODE_DOCUMENT_MAX_LINES);
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
        self.max_spans = default_u64(value, DEFAULT_CODE_DOCUMENT_MAX_SPANS);
        self
    }

    /// Returns the maximum complete semantic UTF-8 byte count
    #[must_use]
    pub const fn max_text_bytes(self) -> u64 {
        self.max_text_bytes
    }

    /// Returns these limits with a maximum semantic UTF-8 byte count
    #[must_use]
    pub const fn with_max_text_bytes(mut self, value: u64) -> Self {
        self.max_text_bytes = default_u64(value, DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES);
        self
    }
}

impl Default for CodeDocumentLimits {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_CODE_DOCUMENT_MAX_LINES,
            max_spans: DEFAULT_CODE_DOCUMENT_MAX_SPANS,
            max_text_bytes: DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES,
        }
    }
}

/// Stable category of a code-document resource failure
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodeDocumentErrorKind {
    /// The logical line limit was exceeded
    LineLimit,
    /// The styled span limit was exceeded
    SpanLimit,
    /// The complete semantic UTF-8 byte limit was exceeded
    TextByteLimit,
}

impl CodeDocumentErrorKind {
    /// Returns the stable language-independent error identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LineLimit => "line-limit",
            Self::SpanLimit => "span-limit",
            Self::TextByteLimit => "text-byte-limit",
        }
    }
}

impl fmt::Display for CodeDocumentErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured code-document resource failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeDocumentError {
    kind: CodeDocumentErrorKind,
    limit: u64,
    observed: u64,
}

impl CodeDocumentError {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> CodeDocumentErrorKind {
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

impl fmt::Display for CodeDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "code document {} exceeded limit {} with {}",
            self.kind, self.limit, self.observed
        )
    }
}

impl Error for CodeDocumentError {}

/// Immutable logical lines and complete semantic source text
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeDocument {
    inner: Arc<CodeDocumentInner>,
}

#[derive(Debug, Eq, PartialEq)]
struct CodeDocumentInner {
    lines: Arc<[CodeLine]>,
    text: Arc<str>,
    line_ranges: Arc<[Range<usize>]>,
    hidden_prefix: Arc<[u64]>,
    trailing_newline: bool,
}

impl CodeDocument {
    /// Builds a document using bounded default limits
    pub fn new(
        lines: impl IntoIterator<Item = CodeLine>,
        trailing_newline: bool,
    ) -> Result<Self, CodeDocumentError> {
        Self::new_with_limits(lines, trailing_newline, CodeDocumentLimits::default())
    }

    /// Builds a document after validating every configured resource limit
    pub fn new_with_limits(
        source: impl IntoIterator<Item = CodeLine>,
        trailing_newline: bool,
        limits: CodeDocumentLimits,
    ) -> Result<Self, CodeDocumentError> {
        let mut lines = Vec::new();
        let mut span_count = 0_u64;
        let mut text_bytes = 0_u64;
        for line in source {
            let line_count = u64::try_from(lines.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1);
            check_limit(
                CodeDocumentErrorKind::LineLimit,
                limits.max_lines,
                line_count,
            )?;
            span_count =
                span_count.saturating_add(u64::try_from(line.spans().len()).unwrap_or(u64::MAX));
            check_limit(
                CodeDocumentErrorKind::SpanLimit,
                limits.max_spans,
                span_count,
            )?;
            if !lines.is_empty() {
                text_bytes = text_bytes.saturating_add(1);
                check_limit(
                    CodeDocumentErrorKind::TextByteLimit,
                    limits.max_text_bytes,
                    text_bytes,
                )?;
            }
            text_bytes =
                text_bytes.saturating_add(u64::try_from(line.text().len()).unwrap_or(u64::MAX));
            check_limit(
                CodeDocumentErrorKind::TextByteLimit,
                limits.max_text_bytes,
                text_bytes,
            )?;
            lines.push(line);
        }
        if trailing_newline && !lines.is_empty() {
            text_bytes = text_bytes.saturating_add(1);
            check_limit(
                CodeDocumentErrorKind::TextByteLimit,
                limits.max_text_bytes,
                text_bytes,
            )?;
        }

        let trailing_newline = trailing_newline && !lines.is_empty();
        let capacity = usize::try_from(text_bytes).unwrap_or(usize::MAX);
        let mut text = String::with_capacity(capacity);
        let mut line_ranges = Vec::with_capacity(lines.len());
        let mut hidden_prefix = Vec::with_capacity(lines.len() + 1);
        hidden_prefix.push(0_u64);
        for (index, line) in lines.iter().enumerate() {
            let start = text.len();
            text.push_str(line.text());
            line_ranges.push(start..text.len());
            let previous = *hidden_prefix.last().unwrap_or(&0);
            hidden_prefix.push(previous + u64::from(!line.is_copyable()));
            if index + 1 < lines.len() || trailing_newline {
                text.push('\n');
            }
        }
        Ok(Self {
            inner: Arc::new(CodeDocumentInner {
                lines: Arc::from(lines),
                text: Arc::from(text),
                line_ranges: Arc::from(line_ranges),
                hidden_prefix: Arc::from(hidden_prefix),
                trailing_newline,
            }),
        })
    }

    /// Returns the logical line count
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.inner.lines.len()
    }

    /// Returns one logical line by zero-based index
    #[must_use]
    pub fn line(&self, index: usize) -> Option<&CodeLine> {
        self.inner.lines.get(index)
    }

    /// Returns the complete semantic UTF-8 source text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.inner.text
    }

    /// Reports whether the complete source ends with LF
    #[must_use]
    pub fn has_trailing_newline(&self) -> bool {
        self.inner.trailing_newline
    }

    /// Returns the semantic UTF-8 byte range for ordered logical lines
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
                .map_or(self.text().len(), |range| range.start);
            return Some(offset..offset);
        }
        let start = self.inner.line_ranges[lines.start].start;
        let end = if lines.end == self.line_count() && self.has_trailing_newline() {
            self.text().len()
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
}

impl Default for CodeDocument {
    fn default() -> Self {
        Self::new([], false).expect("an empty document is within default limits")
    }
}

/// Terminal-dependent code layout options
#[derive(Clone, Copy, Debug)]
pub struct CodeLayoutOptions {
    viewport_width: u32,
    tab_width: u8,
    wrap: bool,
    line_numbers: bool,
    width_profile: WidthProfile<'static>,
}

impl CodeLayoutOptions {
    /// Returns the total terminal viewport width
    #[must_use]
    pub const fn viewport_width(self) -> u32 {
        self.viewport_width
    }

    /// Returns these options with a positive total viewport width
    ///
    /// Zero restores [`DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH`]
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
    ///
    /// Zero restores [`DEFAULT_CODE_LAYOUT_TAB_WIDTH`]
    #[must_use]
    pub const fn with_tab_width(mut self, value: u8) -> Self {
        self.tab_width = if value == 0 {
            DEFAULT_CODE_LAYOUT_TAB_WIDTH
        } else {
            value
        };
        self
    }

    /// Reports whether long logical lines hard-wrap at grapheme boundaries
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

    /// Reports whether the view reserves a sticky line-number gutter
    #[must_use]
    pub const fn shows_line_numbers(self) -> bool {
        self.line_numbers
    }

    /// Returns these options with the line-number gutter enabled or disabled
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

    /// Returns these options with a replacement terminal cell-width profile
    #[must_use]
    pub const fn with_width_profile(mut self, value: WidthProfile<'static>) -> Self {
        self.width_profile = value;
        self
    }
}

impl Default for CodeLayoutOptions {
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

/// Resource limits applied while constructing a [`CodeLayout`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CodeLayoutLimits {
    max_visual_rows: u64,
    max_display_bytes: u64,
}

impl CodeLayoutLimits {
    /// Returns the maximum projected visual row count
    #[must_use]
    pub const fn max_visual_rows(self) -> u64 {
        self.max_visual_rows
    }

    /// Returns these limits with a maximum projected visual row count
    #[must_use]
    pub const fn with_max_visual_rows(mut self, value: u64) -> Self {
        self.max_visual_rows = default_u64(value, DEFAULT_CODE_LAYOUT_MAX_VISUAL_ROWS);
        self
    }

    /// Returns the maximum expanded display byte count
    #[must_use]
    pub const fn max_display_bytes(self) -> u64 {
        self.max_display_bytes
    }

    /// Returns these limits with a maximum expanded display byte count
    #[must_use]
    pub const fn with_max_display_bytes(mut self, value: u64) -> Self {
        self.max_display_bytes = default_u64(value, DEFAULT_CODE_LAYOUT_MAX_DISPLAY_BYTES);
        self
    }
}

impl Default for CodeLayoutLimits {
    fn default() -> Self {
        Self {
            max_visual_rows: DEFAULT_CODE_LAYOUT_MAX_VISUAL_ROWS,
            max_display_bytes: DEFAULT_CODE_LAYOUT_MAX_DISPLAY_BYTES,
        }
    }
}

/// Stable category of a terminal code-layout resource failure
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodeLayoutErrorKind {
    /// The projected visual row limit was exceeded
    VisualRowLimit,
    /// The expanded display byte limit was exceeded
    DisplayByteLimit,
}

impl CodeLayoutErrorKind {
    /// Returns the stable language-independent error identifier
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VisualRowLimit => "visual-row-limit",
            Self::DisplayByteLimit => "display-byte-limit",
        }
    }
}

impl fmt::Display for CodeLayoutErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured terminal code-layout resource failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLayoutError {
    kind: CodeLayoutErrorKind,
    limit: u64,
    observed: u64,
}

impl CodeLayoutError {
    /// Returns the stable error category
    #[must_use]
    pub const fn kind(&self) -> CodeLayoutErrorKind {
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

impl fmt::Display for CodeLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "code layout {} exceeded limit {} with {}",
            self.kind, self.limit, self.observed
        )
    }
}

impl Error for CodeLayoutError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CodeVisualRow {
    pub(crate) line: usize,
    pub(crate) continuation: bool,
    pub(crate) spans: Arc<[TextSpan]>,
    pub(crate) checkpoints: Arc<[CodeRowCheckpoint]>,
    pub(crate) width: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodeRowCheckpoint {
    pub(crate) cell: u32,
    pub(crate) span: usize,
    pub(crate) byte: usize,
}

/// Immutable terminal projection of one [`CodeDocument`]
#[derive(Clone, Debug)]
pub struct CodeLayout {
    inner: Rc<CodeLayoutInner>,
}

/// Single-entry memo for rebuilding terminal code layouts from immutable views
///
/// The application-defined key must change whenever any projection option or
/// custom width policy changes. Document identity and explicit resource limits
/// are compared independently. The zero value is an empty cache
#[derive(Default)]
pub struct CodeLayoutCache {
    entry: RefCell<Option<CodeLayoutCacheEntry>>,
}

struct CodeLayoutCacheEntry {
    key: u64,
    document: CodeDocument,
    limits: CodeLayoutLimits,
    layout: CodeLayout,
}

impl CodeLayoutCache {
    /// Returns a cached layout or projects one using bounded default limits
    ///
    /// The key must represent every field in `options`, including the identity
    /// and behavior of a Custom width callback
    pub fn resolve(
        &self,
        key: u64,
        document: &CodeDocument,
        options: CodeLayoutOptions,
    ) -> Result<CodeLayout, CodeLayoutError> {
        self.resolve_with_limits(key, document, options, CodeLayoutLimits::default())
    }

    /// Returns a cached layout or projects one using explicit resource limits
    ///
    /// The key must represent every field in `options`, including the identity
    /// and behavior of a Custom width callback
    pub fn resolve_with_limits(
        &self,
        key: u64,
        document: &CodeDocument,
        options: CodeLayoutOptions,
        limits: CodeLayoutLimits,
    ) -> Result<CodeLayout, CodeLayoutError> {
        if let Some(entry) = self.entry.borrow().as_ref()
            && entry.key == key
            && Arc::ptr_eq(&entry.document.inner, &document.inner)
            && entry.limits == limits
        {
            return Ok(entry.layout.clone());
        }
        let layout = CodeLayout::new_with_limits(document.clone(), options, limits)?;
        *self.entry.borrow_mut() = Some(CodeLayoutCacheEntry {
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

#[derive(Debug)]
struct CodeLayoutInner {
    document: CodeDocument,
    options: CodeLayoutOptions,
    rows: Arc<[CodeVisualRow]>,
    line_rows: Arc<[Range<usize>]>,
    gutter_width: u32,
    code_width: u32,
    maximum_row_width: u32,
}

impl CodeLayout {
    /// Projects a document using bounded default layout limits
    pub fn new(
        document: CodeDocument,
        options: CodeLayoutOptions,
    ) -> Result<Self, CodeLayoutError> {
        Self::new_with_limits(document, options, CodeLayoutLimits::default())
    }

    /// Projects a document after validating every layout resource limit
    pub fn new_with_limits(
        document: CodeDocument,
        options: CodeLayoutOptions,
        limits: CodeLayoutLimits,
    ) -> Result<Self, CodeLayoutError> {
        let (gutter_width, code_width) = code_layout_widths(&document, options);
        let row_capacity = document
            .line_count()
            .min(usize::try_from(limits.max_visual_rows).unwrap_or(usize::MAX));
        let mut builder = CodeLayoutBuilder::new(options, code_width, limits, row_capacity);
        let mut line_rows = Vec::with_capacity(row_capacity);
        for line_index in 0..document.line_count() {
            let start = builder.rows.len();
            builder.push_line(line_index, document.line(line_index).expect("known line"))?;
            line_rows.push(start..builder.rows.len());
        }
        Ok(Self {
            inner: Rc::new(CodeLayoutInner {
                document,
                options,
                rows: Arc::from(builder.rows),
                line_rows: Arc::from(line_rows),
                gutter_width,
                code_width,
                maximum_row_width: builder.maximum_row_width,
            }),
        })
    }

    /// Returns the immutable semantic source document
    #[must_use]
    pub fn document(&self) -> &CodeDocument {
        &self.inner.document
    }

    /// Returns the terminal projection options
    #[must_use]
    pub fn options(&self) -> CodeLayoutOptions {
        self.inner.options
    }

    /// Returns the projected visual row count
    #[must_use]
    pub fn visual_row_count(&self) -> usize {
        self.inner.rows.len()
    }

    /// Returns the sticky line-number gutter width
    #[must_use]
    pub fn gutter_width(&self) -> u32 {
        self.inner.gutter_width
    }

    /// Returns the code region width after reserving the optional gutter
    #[must_use]
    pub fn code_width(&self) -> u32 {
        self.inner.code_width
    }

    /// Returns the widest projected visual row before horizontal cropping
    #[must_use]
    pub fn maximum_row_width(&self) -> u32 {
        self.inner.maximum_row_width
    }

    pub(crate) fn row(&self, index: usize) -> Option<&CodeVisualRow> {
        self.inner.rows.get(index)
    }

    pub(crate) fn rows_for_line(&self, line: usize) -> Range<usize> {
        self.inner.line_rows.get(line).cloned().unwrap_or_default()
    }
}

fn code_layout_widths(document: &CodeDocument, options: CodeLayoutOptions) -> (u32, u32) {
    let digits = decimal_digits(document.line_count().max(1));
    let desired_gutter = u32::try_from(digits).unwrap_or(u32::MAX).saturating_add(3);
    let gutter = if options.line_numbers && options.viewport_width > desired_gutter {
        desired_gutter
    } else {
        0
    };
    (gutter, options.viewport_width.saturating_sub(gutter).max(1))
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

struct CodeLayoutBuilder {
    options: CodeLayoutOptions,
    code_width: u32,
    limits: CodeLayoutLimits,
    rows: Vec<CodeVisualRow>,
    display_bytes: u64,
    maximum_row_width: u32,
}

impl CodeLayoutBuilder {
    fn new(
        options: CodeLayoutOptions,
        code_width: u32,
        limits: CodeLayoutLimits,
        row_capacity: usize,
    ) -> Self {
        Self {
            options,
            code_width,
            limits,
            rows: Vec::with_capacity(row_capacity),
            display_bytes: 0,
            maximum_row_width: 0,
        }
    }

    fn push_line(&mut self, line_index: usize, line: &CodeLine) -> Result<(), CodeLayoutError> {
        if !self.options.wrap && !line.text().contains('\t') {
            let width = line.spans().iter().fold(0_u32, |total, span| {
                total.saturating_add(
                    u32::try_from(text_width(span.text(), self.options.width_profile))
                        .unwrap_or(u32::MAX),
                )
            });
            self.add_display_bytes(line.text().len())?;
            return self.push_row(line_index, false, line.shared_spans(), width);
        }

        let mut continuation = false;
        let mut logical_column = 0_u32;
        let mut row_width = 0_u32;
        let mut row = StyledRowBuilder::default();
        for span in line.spans() {
            for grapheme in graphemes(span.text()) {
                if grapheme.text() == "\t" {
                    let tab_width = u32::from(self.options.tab_width);
                    let count = tab_width - logical_column % tab_width;
                    for _ in 0..count {
                        self.push_atom(
                            line_index,
                            &mut continuation,
                            &mut row,
                            &mut row_width,
                            " ",
                            1,
                            span.style(),
                        )?;
                        logical_column = logical_column.saturating_add(1);
                    }
                } else {
                    let width =
                        u32::try_from(grapheme_width(grapheme.text(), self.options.width_profile))
                            .unwrap_or(u32::MAX);
                    self.push_atom(
                        line_index,
                        &mut continuation,
                        &mut row,
                        &mut row_width,
                        grapheme.text(),
                        width,
                        span.style(),
                    )?;
                    logical_column = logical_column.saturating_add(width);
                }
            }
        }
        if !row.is_empty()
            || self
                .rows
                .last()
                .is_none_or(|value| value.line != line_index)
        {
            self.finish_row(line_index, continuation, row, row_width)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_atom(
        &mut self,
        line_index: usize,
        continuation: &mut bool,
        row: &mut StyledRowBuilder,
        row_width: &mut u32,
        text: &str,
        width: u32,
        style: Style,
    ) -> Result<(), CodeLayoutError> {
        if self.options.wrap && *row_width > 0 && row_width.saturating_add(width) > self.code_width
        {
            let complete = std::mem::take(row);
            self.finish_row(line_index, *continuation, complete, *row_width)?;
            *continuation = true;
            *row_width = 0;
        }
        self.add_display_bytes(text.len())?;
        row.push(text, style);
        *row_width = row_width.saturating_add(width);
        Ok(())
    }

    fn finish_row(
        &mut self,
        line_index: usize,
        continuation: bool,
        row: StyledRowBuilder,
        width: u32,
    ) -> Result<(), CodeLayoutError> {
        let spans = row.finish();
        self.push_row(line_index, continuation, Arc::from(spans), width)
    }

    fn push_row(
        &mut self,
        line: usize,
        continuation: bool,
        spans: Arc<[TextSpan]>,
        width: u32,
    ) -> Result<(), CodeLayoutError> {
        let observed = u64::try_from(self.rows.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        check_layout_limit(
            CodeLayoutErrorKind::VisualRowLimit,
            self.limits.max_visual_rows,
            observed,
        )?;
        self.maximum_row_width = self.maximum_row_width.max(width);
        let checkpoints = if self.options.wrap || width <= CODE_ROW_CHECKPOINT_CELLS {
            Arc::default()
        } else {
            Arc::from(code_row_checkpoints(&spans, self.options.width_profile))
        };
        self.rows.push(CodeVisualRow {
            line,
            continuation,
            spans,
            checkpoints,
            width,
        });
        Ok(())
    }

    fn add_display_bytes(&mut self, bytes: usize) -> Result<(), CodeLayoutError> {
        self.display_bytes = self
            .display_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        check_layout_limit(
            CodeLayoutErrorKind::DisplayByteLimit,
            self.limits.max_display_bytes,
            self.display_bytes,
        )
    }
}

const CODE_ROW_CHECKPOINT_CELLS: u32 = 256;

fn code_row_checkpoints(spans: &[TextSpan], profile: WidthProfile<'_>) -> Vec<CodeRowCheckpoint> {
    let mut checkpoints = Vec::new();
    let mut cell = 0_u32;
    let mut checkpoint_cell = 0_u32;
    for (span_index, span) in spans.iter().enumerate() {
        for grapheme in graphemes(span.text()) {
            if cell.saturating_sub(checkpoint_cell) >= CODE_ROW_CHECKPOINT_CELLS {
                checkpoints.push(CodeRowCheckpoint {
                    cell,
                    span: span_index,
                    byte: grapheme.start(),
                });
                checkpoint_cell = cell;
            }
            cell = cell.saturating_add(
                u32::try_from(grapheme_width(grapheme.text(), profile)).unwrap_or(u32::MAX),
            );
        }
    }
    checkpoints
}

#[derive(Default)]
struct StyledRowBuilder {
    spans: Vec<(String, Style)>,
}

impl StyledRowBuilder {
    fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn push(&mut self, text: &str, style: Style) {
        if let Some((existing, existing_style)) = self.spans.last_mut()
            && *existing_style == style
        {
            existing.push_str(text);
            return;
        }
        self.spans.push((text.to_owned(), style));
    }

    fn finish(self) -> Vec<TextSpan> {
        self.spans
            .into_iter()
            .map(|(text, style)| TextSpan::new(text, style))
            .collect()
    }
}

fn check_limit(
    kind: CodeDocumentErrorKind,
    limit: u64,
    observed: u64,
) -> Result<(), CodeDocumentError> {
    if observed > limit {
        Err(CodeDocumentError {
            kind,
            limit,
            observed,
        })
    } else {
        Ok(())
    }
}

fn check_layout_limit(
    kind: CodeLayoutErrorKind,
    limit: u64,
    observed: u64,
) -> Result<(), CodeLayoutError> {
    if observed > limit {
        Err(CodeLayoutError {
            kind,
            limit,
            observed,
        })
    } else {
        Ok(())
    }
}

const fn default_u64(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use nagi_tui::Color;

    use super::*;

    #[test]
    fn line_rejects_breaks_and_split_graphemes() {
        let line = CodeLine::plain("a\nb").unwrap_err();
        assert_eq!(line.kind(), InvalidCodeLineKind::LineBreak);
        let split = CodeLine::styled([
            TextSpan::new("e", Style::default()),
            TextSpan::new("\u{301}", Style::default()),
        ])
        .unwrap_err();
        assert_eq!(split.kind(), InvalidCodeLineKind::GraphemeBoundary);
    }

    #[test]
    fn document_preserves_line_ranges_and_trailing_newline() {
        let document = CodeDocument::new(
            [
                CodeLine::plain("a").unwrap(),
                CodeLine::plain("日").unwrap(),
            ],
            true,
        )
        .unwrap();
        assert_eq!(document.text(), "a\n日\n");
        assert_eq!(document.byte_range_for_lines(0..1), Some(0..1));
        assert_eq!(document.byte_range_for_lines(1..2), Some(2..6));
        assert_eq!(document.byte_range_for_lines(0..2), Some(0..6));
    }

    #[test]
    fn hidden_line_makes_only_overlapping_ranges_non_copyable() {
        let hidden = CodeLine::styled([TextSpan::new(
            "secret",
            Style {
                hidden: true,
                ..Style::default()
            },
        )])
        .unwrap();
        let document =
            CodeDocument::new([CodeLine::plain("public").unwrap(), hidden], false).unwrap();
        assert!(document.is_range_copyable(0..1));
        assert!(!document.is_range_copyable(1..2));
        assert!(!document.is_range_copyable(0..2));
    }

    #[test]
    fn layout_expands_tabs_wraps_and_preserves_styles() {
        let line = CodeLine::styled([
            TextSpan::new(
                "A\t",
                Style {
                    foreground: Color::Indexed(1),
                    ..Style::default()
                },
            ),
            TextSpan::new("日B", Style::default()),
        ])
        .unwrap();
        let document = CodeDocument::new([line], false).unwrap();
        let layout = CodeLayout::new(
            document,
            CodeLayoutOptions::default()
                .with_viewport_width(8)
                .with_line_numbers(false)
                .with_wrap(true),
        )
        .unwrap();
        assert_eq!(layout.visual_row_count(), 1);
        let row = layout.row(0).unwrap();
        assert_eq!(row.width, 7);
        assert_eq!(row.spans[0].text(), "A   ");
        assert_eq!(row.spans[1].text(), "日B");
    }

    #[test]
    fn no_wrap_layout_shares_validated_source_spans() {
        let line = CodeLine::plain("shared").unwrap();
        let source_spans = line.shared_spans();
        let document = CodeDocument::new([line], false).unwrap();
        let layout = CodeLayout::new(document, CodeLayoutOptions::default()).unwrap();
        let row = layout.row(0).unwrap();
        assert!(Arc::ptr_eq(&source_spans, &row.spans));
        assert!(row.checkpoints.is_empty());
    }

    #[test]
    fn cjk_profile_changes_wrapping() {
        let document = CodeDocument::new([CodeLine::plain("·A").unwrap()], false).unwrap();
        let modern = CodeLayout::new(
            document.clone(),
            CodeLayoutOptions::default()
                .with_viewport_width(2)
                .with_line_numbers(false)
                .with_wrap(true),
        )
        .unwrap();
        let cjk = CodeLayout::new(
            document,
            CodeLayoutOptions::default()
                .with_viewport_width(2)
                .with_line_numbers(false)
                .with_wrap(true)
                .with_width_profile(WidthProfile::CJK),
        )
        .unwrap();
        assert_eq!(modern.visual_row_count(), 1);
        assert_eq!(cjk.visual_row_count(), 2);
    }

    #[test]
    fn source_and_layout_limits_fail_before_publication() {
        let line = CodeLine::plain("abcd").unwrap();
        let error = CodeDocument::new_with_limits(
            [line.clone(), line.clone()],
            false,
            CodeDocumentLimits::default().with_max_lines(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeDocumentErrorKind::LineLimit);

        let error = CodeDocument::new_with_limits(
            [CodeLine::styled([
                TextSpan::new("a", Style::default()),
                TextSpan::new("b", Style::default()),
            ])
            .unwrap()],
            false,
            CodeDocumentLimits::default().with_max_spans(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeDocumentErrorKind::SpanLimit);
        assert_eq!(error.observed(), 2);

        let error = CodeDocument::new_with_limits(
            [CodeLine::plain("ab").unwrap()],
            false,
            CodeDocumentLimits::default().with_max_text_bytes(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeDocumentErrorKind::TextByteLimit);
        assert_eq!(error.observed(), 2);

        let document = CodeDocument::new([line], false).unwrap();
        let error = CodeLayout::new_with_limits(
            document,
            CodeLayoutOptions::default()
                .with_viewport_width(1)
                .with_line_numbers(false)
                .with_wrap(true),
            CodeLayoutLimits::default().with_max_visual_rows(2),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeLayoutErrorKind::VisualRowLimit);
        assert_eq!(error.observed(), 3);

        let error = CodeLayout::new_with_limits(
            CodeDocument::new([CodeLine::plain("ab").unwrap()], false).unwrap(),
            CodeLayoutOptions::default().with_line_numbers(false),
            CodeLayoutLimits::default().with_max_display_bytes(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeLayoutErrorKind::DisplayByteLimit);
        assert_eq!(error.observed(), 2);

        let error = CodeLayout::new_with_limits(
            CodeDocument::new([CodeLine::plain("a\t").unwrap()], false).unwrap(),
            CodeLayoutOptions::default()
                .with_line_numbers(false)
                .with_tab_width(u8::MAX),
            CodeLayoutLimits::default().with_max_display_bytes(2),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeLayoutErrorKind::DisplayByteLimit);
        assert_eq!(error.observed(), 3);
    }

    #[test]
    fn document_stops_consuming_source_at_the_first_line_over_limit() {
        let consumed = Cell::new(0_u64);
        let line = CodeLine::plain("a").unwrap();
        let source = std::iter::from_fn(|| {
            consumed.set(consumed.get() + 1);
            Some(line.clone())
        });
        let error = CodeDocument::new_with_limits(
            source,
            false,
            CodeDocumentLimits::default().with_max_lines(2),
        )
        .unwrap_err();
        assert_eq!(error.kind(), CodeDocumentErrorKind::LineLimit);
        assert_eq!(error.observed(), 3);
        assert_eq!(consumed.get(), 3);
    }

    #[test]
    fn layout_cache_reuses_only_matching_document_key_and_limits() {
        let document = CodeDocument::new([CodeLine::plain("a").unwrap()], false).unwrap();
        let cache = CodeLayoutCache::default();
        let first = cache
            .resolve(1, &document, CodeLayoutOptions::default())
            .unwrap();
        let second = cache
            .resolve(1, &document, CodeLayoutOptions::default())
            .unwrap();
        assert!(Rc::ptr_eq(&first.inner, &second.inner));
        let changed = cache
            .resolve(2, &document, CodeLayoutOptions::default().with_wrap(true))
            .unwrap();
        assert!(!Rc::ptr_eq(&first.inner, &changed.inner));
        cache.clear();
        let cleared = cache
            .resolve(2, &document, CodeLayoutOptions::default())
            .unwrap();
        assert!(!Rc::ptr_eq(&changed.inner, &cleared.inner));
    }

    #[test]
    fn code_layout_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/code-layout.txt",
            "widget-code-layout",
            &[
                "input",
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
            let profile = match record.field("profile") {
                "modern" => WidthProfile::MODERN,
                "cjk" => WidthProfile::CJK,
                value => panic!("case {} has unknown profile {value}", record.id),
            };
            let options = CodeLayoutOptions::default()
                .with_viewport_width(fixture_u32(record.field("viewport")))
                .with_tab_width(fixture_u8(record.field("tab")))
                .with_wrap(fixture_bool(record.field("wrap")))
                .with_line_numbers(fixture_bool(record.field("line-numbers")))
                .with_width_profile(profile);
            let document =
                CodeDocument::new([CodeLine::plain(record.text("input")).unwrap()], false).unwrap();
            let layout = CodeLayout::new(document, options).unwrap();
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
            let actual_rows = (0..layout.visual_row_count())
                .map(|index| {
                    layout
                        .row(index)
                        .unwrap()
                        .spans
                        .iter()
                        .map(TextSpan::text)
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual_rows,
                record
                    .text("expected-rows")
                    .split('|')
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
                "case {} rows",
                record.id
            );
            let actual_widths = (0..layout.visual_row_count())
                .map(|index| layout.row(index).unwrap().width)
                .collect::<Vec<_>>();
            assert_eq!(
                actual_widths,
                record
                    .field("expected-widths")
                    .split(',')
                    .map(fixture_u32)
                    .collect::<Vec<_>>(),
                "case {} widths",
                record.id
            );
            let actual_continuations = (0..layout.visual_row_count())
                .map(|index| layout.row(index).unwrap().continuation)
                .collect::<Vec<_>>();
            assert_eq!(
                actual_continuations,
                record
                    .field("expected-continuations")
                    .split(',')
                    .map(fixture_bool)
                    .collect::<Vec<_>>(),
                "case {} continuations",
                record.id
            );
        }
    }

    fn fixture_bool(value: &str) -> bool {
        match value {
            "true" => true,
            "false" => false,
            _ => panic!("invalid fixture Boolean {value}"),
        }
    }

    fn fixture_u8(value: &str) -> u8 {
        value.parse().unwrap()
    }

    fn fixture_u32(value: &str) -> u32 {
        value.parse().unwrap()
    }
}
