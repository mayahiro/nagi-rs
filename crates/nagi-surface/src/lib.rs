//! Cell surface composition and diff primitives for Nagi TUI
//!
//! Surfaces store normalized grapheme cells independently from terminal I/O or
//! VT encoding. Drawing and composition preserve wide-grapheme boundaries

mod cell;
mod geometry;
mod snapshot;
mod surface;

pub use cell::{Cell, CellError, CellSpan, Opacity};
pub use geometry::{Point, Rect, Size};
pub use surface::{ChangedRun, Cursor, MAX_SURFACE_CELLS, Surface, SurfaceError};
