//! Unicode grapheme and terminal cell text primitives for Nagi terminal applications
//!
//! Segmentation follows Unicode Standard Annex #29 extended grapheme clusters
//! using the committed Unicode data version. Width is an explicit terminal
//! policy and does not normalize input text

mod generated;
mod grapheme;
mod operations;
mod unicode;
mod utf8;
mod width;

pub use grapheme::{
    Grapheme, Graphemes, grapheme_boundaries, graphemes, is_grapheme_boundary,
    next_grapheme_boundary, previous_grapheme_boundary,
};
pub use operations::{byte_at_cell, cell_at_byte, text_width, truncate, wrap};
pub use utf8::normalize_utf8;
pub use width::{CellCount, WidthOverride, WidthProfile, grapheme_width};

/// The Unicode data version used by segmentation and width calculations
pub const UNICODE_VERSION: &str = generated::UNICODE_VERSION;
