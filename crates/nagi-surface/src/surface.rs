use std::error::Error;
use std::fmt;

use nagi_text::{WidthProfile, graphemes};
use nagi_vt::Style;

use crate::{Cell, CellSpan, Opacity};

/// Maximum number of cells accepted by a surface constructor
///
/// The limit keeps construction failure deterministic across Rust and Go and
/// is substantially larger than ordinary terminal dimensions
pub const MAX_SURFACE_CELLS: u64 = 1_048_576;

/// An error returned while constructing a surface
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceError {
    /// The requested dimensions exceed [`MAX_SURFACE_CELLS`]
    DimensionsTooLarge,
    /// The cell storage allocation failed
    AllocationFailed,
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsTooLarge => formatter.write_str("surface dimensions are too large"),
            Self::AllocationFailed => formatter.write_str("surface allocation failed"),
        }
    }
}

impl Error for SurfaceError {}

/// A visible surface cursor position
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Cursor {
    /// Horizontal cell coordinate
    pub x: u32,
    /// Vertical cell coordinate
    pub y: u32,
}

impl Cursor {
    /// Creates a cursor position
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// A half-open changed row interval
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChangedRun {
    /// Row coordinate
    pub row: u32,
    /// Inclusive horizontal start
    pub start: u32,
    /// Exclusive horizontal end
    pub end: u32,
}

/// A fixed-size grid of normalized terminal cells
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Surface {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
    cursor: Option<Cursor>,
}

impl Surface {
    /// Creates an opaque surface filled with default-style blanks
    pub fn new(width: u32, height: u32) -> Result<Self, SurfaceError> {
        Self::allocate(width, height, Cell::default())
    }

    /// Creates a surface filled with default-style transparent cells
    pub fn transparent(width: u32, height: u32) -> Result<Self, SurfaceError> {
        Self::allocate(width, height, Cell::transparent(Style::default()))
    }

    fn allocate(width: u32, height: u32, initial: Cell) -> Result<Self, SurfaceError> {
        let count = u64::from(width) * u64::from(height);
        if count > MAX_SURFACE_CELLS
            || u64::from(width) > MAX_SURFACE_CELLS
            || u64::from(height) > MAX_SURFACE_CELLS
        {
            return Err(SurfaceError::DimensionsTooLarge);
        }
        let count = usize::try_from(count).map_err(|_| SurfaceError::DimensionsTooLarge)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| SurfaceError::AllocationFailed)?;
        cells.resize(count, initial);
        Ok(Self {
            width,
            height,
            cells,
            cursor: None,
        })
    }

    /// Returns the surface width
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the surface height
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Reports whether either dimension is zero
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the cell at signed coordinates, or `None` outside the surface
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> Option<&Cell> {
        let x = u32::try_from(x).ok()?;
        let y = u32::try_from(y).ok()?;
        self.cell_u32(x, y)
    }

    /// Returns a complete row, or `None` when `row` is outside the surface
    #[must_use]
    pub fn row(&self, row: u32) -> Option<&[Cell]> {
        if row >= self.height {
            return None;
        }
        let start = usize::try_from(row).ok()? * self.width_usize();
        Some(&self.cells[start..start + self.width_usize()])
    }

    /// Returns the visible cursor, if any
    #[must_use]
    pub const fn cursor(&self) -> Option<Cursor> {
        self.cursor
    }

    /// Sets or hides the cursor
    ///
    /// An out-of-bounds position hides the cursor and returns `false`
    pub fn set_cursor(&mut self, cursor: Option<Cursor>) -> bool {
        match cursor {
            Some(cursor) if cursor.x < self.width && cursor.y < self.height => {
                self.cursor = Some(cursor);
                true
            }
            Some(_) => {
                self.cursor = None;
                false
            }
            None => {
                self.cursor = None;
                true
            }
        }
    }

    /// Replaces every cell with a default-style opaque blank and hides the
    /// cursor
    pub fn clear(&mut self) {
        self.clear_with_style(Style::default());
    }

    /// Replaces every cell with an opaque blank using `style` and hides the
    /// cursor
    pub fn clear_with_style(&mut self, style: Style) {
        self.cells.fill(Cell::blank(style));
        self.cursor = None;
    }

    /// Fills a clipped rectangle with opaque blanks using `style`
    pub fn fill(&mut self, x: i32, y: i32, width: u32, height: u32, style: Style) {
        self.fill_cell(x, y, width, height, Cell::blank(style));
    }

    /// Fills a clipped rectangle with style-only transparent cells
    pub fn fill_transparent(&mut self, x: i32, y: i32, width: u32, height: u32, style: Style) {
        self.fill_cell(x, y, width, height, Cell::transparent(style));
    }

    fn fill_cell(&mut self, x: i32, y: i32, width: u32, height: u32, cell: Cell) {
        let start_x = i64::from(x).max(0);
        let start_y = i64::from(y).max(0);
        let end_x = (i64::from(x) + i64::from(width)).min(i64::from(self.width));
        let end_y = (i64::from(y) + i64::from(height)).min(i64::from(self.height));
        if start_x >= end_x || start_y >= end_y {
            return;
        }
        for row in start_y..end_y {
            for column in start_x..end_x {
                self.place_cell(column as usize, row as usize, cell.clone());
            }
        }
    }

    /// Writes text left-to-right on one row using complete grapheme clusters
    ///
    /// Drawing is clipped to the surface. A wide grapheme that would be only
    /// partly visible is skipped as one indivisible unit
    pub fn write(&mut self, x: i32, y: i32, text: &str, style: Style, profile: WidthProfile<'_>) {
        if y < 0 || i64::from(y) >= i64::from(self.height) {
            return;
        }
        let row = y as usize;
        let mut column = i64::from(x);
        for grapheme in graphemes(text) {
            if column >= i64::from(self.width) {
                break;
            }
            let cell = Cell::from_cluster(grapheme.text(), style, profile);
            let span = cell.span().cells() as i64;
            let end = column.saturating_add(span);
            if column >= 0 && end <= i64::from(self.width) {
                self.place_cell(column as usize, row, cell);
            }
            column = end;
        }
    }

    /// Replaces the style of a cell's complete grapheme unit
    ///
    /// Returns `false` when the coordinate is outside the surface
    pub fn set_style(&mut self, x: i32, y: i32, style: Style) -> bool {
        let Some((start, end)) = self.cluster_bounds_signed(x, y) else {
            return false;
        };
        for index in start..end {
            self.cells[index].replace_style(style);
        }
        true
    }

    /// Composites `source` at a signed destination offset
    ///
    /// Opaque cells replace destination content. Transparent cells preserve
    /// content and merge their non-default style over the complete destination
    /// grapheme unit. Partly clipped wide graphemes are skipped
    pub fn composite(&mut self, source: &Self, offset_x: i32, offset_y: i32) {
        for source_y in 0..source.height_usize() {
            let target_y = i64::from(offset_y) + source_y as i64;
            if target_y < 0 || target_y >= i64::from(self.height) {
                continue;
            }
            for source_x in 0..source.width_usize() {
                let source_cell = &source.cells[source.index(source_x, source_y)];
                if source_cell.is_continuation() {
                    continue;
                }
                let target_x = i64::from(offset_x) + source_x as i64;
                let target_end = target_x.saturating_add(source_cell.span().cells() as i64);
                if target_x < 0 || target_end > i64::from(self.width) {
                    continue;
                }
                let target_x = target_x as usize;
                let target_y = target_y as usize;
                match source_cell.opacity() {
                    Opacity::Opaque => {
                        self.place_cell(target_x, target_y, source_cell.clone());
                    }
                    Opacity::Transparent => {
                        self.merge_style_at(target_x, target_y, source_cell.style());
                    }
                }
            }
        }

        if let Some(cursor) = source.cursor {
            let x = i64::from(offset_x) + i64::from(cursor.x);
            let y = i64::from(offset_y) + i64::from(cursor.y);
            self.cursor =
                if x >= 0 && x < i64::from(self.width) && y >= 0 && y < i64::from(self.height) {
                    Some(Cursor::new(x as u32, y as u32))
                } else {
                    None
                };
        }
    }

    /// Returns row-contiguous changed intervals relative to `previous`
    ///
    /// Run boundaries are expanded across wide grapheme units in either
    /// surface. When dimensions differ, every row in this surface is changed
    #[must_use]
    pub fn changed_runs(&self, previous: &Self) -> Vec<ChangedRun> {
        if self.width != previous.width || self.height != previous.height {
            return (0..self.height)
                .filter(|_| self.width != 0)
                .map(|row| ChangedRun {
                    row,
                    start: 0,
                    end: self.width,
                })
                .collect();
        }

        let width = self.width_usize();
        let mut runs = Vec::new();
        for row in 0..self.height_usize() {
            let mut changed = vec![false; width];
            for (column, changed_cell) in changed.iter_mut().enumerate() {
                let index = self.index(column, row);
                *changed_cell = self.cells[index] != previous.cells[index];
            }

            loop {
                let mut expanded = false;
                for column in 0..width {
                    if !changed[column] {
                        continue;
                    }
                    expanded |= self.mark_cluster(row, column, &mut changed);
                    expanded |= previous.mark_cluster(row, column, &mut changed);
                }
                if !expanded {
                    break;
                }
            }

            let mut column = 0;
            while column < width {
                if !changed[column] {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < width && changed[column] {
                    column += 1;
                }
                runs.push(ChangedRun {
                    row: row as u32,
                    start: start as u32,
                    end: column as u32,
                });
            }
        }
        runs
    }

    /// Returns the canonical language-independent snapshot
    #[must_use]
    pub fn snapshot(&self) -> String {
        crate::snapshot::snapshot(self)
    }

    fn cell_u32(&self, x: u32, y: u32) -> Option<&Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(&self.cells[self.index(x as usize, y as usize)])
    }

    fn width_usize(&self) -> usize {
        self.width as usize
    }

    fn height_usize(&self) -> usize {
        self.height as usize
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width_usize() + x
    }

    fn place_cell(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        let span = cell.span().cells();
        if cell.is_continuation() || x >= self.width_usize() || y >= self.height_usize() {
            return false;
        }
        if span == 2 && x + 1 >= self.width_usize() {
            return false;
        }
        self.erase_cluster_at(x, y);
        if span == 2 {
            self.erase_cluster_at(x + 1, y);
        }
        let index = self.index(x, y);
        if span == 2 {
            let continuation = Cell::continuation(&cell);
            self.cells[index] = cell;
            self.cells[index + 1] = continuation;
        } else {
            self.cells[index] = cell;
        }
        true
    }

    fn erase_cluster_at(&mut self, x: usize, y: usize) {
        let index = self.index(x, y);
        let cell = &self.cells[index];
        let leading_x = if cell.is_continuation() {
            let Some(leading_x) = x.checked_sub(1) else {
                return;
            };
            leading_x
        } else if cell.span() == CellSpan::Two {
            x
        } else {
            return;
        };
        if leading_x + 1 >= self.width_usize() {
            return;
        }
        let leading_index = self.index(leading_x, y);
        let style = self.cells[leading_index].style();
        self.cells[leading_index] = Cell::blank(style);
        self.cells[leading_index + 1] = Cell::blank(style);
    }

    fn merge_style_at(&mut self, x: usize, y: usize, overlay: Style) {
        let Some((start, end)) = self.cluster_bounds(x, y) else {
            return;
        };
        for index in start..end {
            let style = self.cells[index].style().merged(overlay);
            self.cells[index].replace_style(style);
        }
    }

    fn cluster_bounds_signed(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let x = u32::try_from(x).ok()?;
        let y = u32::try_from(y).ok()?;
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cluster_bounds(x as usize, y as usize)
    }

    fn cluster_bounds(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        if x >= self.width_usize() || y >= self.height_usize() {
            return None;
        }
        let index = self.index(x, y);
        let cell = &self.cells[index];
        if cell.is_continuation() {
            let start = index.checked_sub(1)?;
            Some((start, index + 1))
        } else if cell.span() == CellSpan::Two && x + 1 < self.width_usize() {
            Some((index, index + 2))
        } else {
            Some((index, index + 1))
        }
    }

    fn mark_cluster(&self, row: usize, column: usize, changed: &mut [bool]) -> bool {
        let cell = &self.cells[self.index(column, row)];
        let (start, end) = if cell.is_continuation() {
            (column.saturating_sub(1), column + 1)
        } else if cell.span() == CellSpan::Two {
            (column, (column + 2).min(self.width_usize()))
        } else {
            (column, column + 1)
        };
        let mut expanded = false;
        for value in &mut changed[start..end] {
            if !*value {
                *value = true;
                expanded = true;
            }
        }
        expanded
    }
}

#[cfg(test)]
mod tests {
    use nagi_text::WidthProfile;

    use super::{MAX_SURFACE_CELLS, Surface, SurfaceError};
    use nagi_vt::Style;

    use crate::CellSpan;

    #[test]
    fn dimensions_fail_without_overflow_or_allocation_panic() {
        assert_eq!(
            Surface::new(0, u32::MAX),
            Err(SurfaceError::DimensionsTooLarge)
        );
        assert_eq!(
            Surface::new(u32::MAX, u32::MAX),
            Err(SurfaceError::DimensionsTooLarge)
        );
        assert_eq!(
            Surface::new(MAX_SURFACE_CELLS as u32 + 1, 1),
            Err(SurfaceError::DimensionsTooLarge)
        );
        assert!(Surface::new(1_024, 1_024).is_ok());
    }

    #[test]
    fn overwriting_a_continuation_repairs_the_wide_unit() {
        let mut surface = Surface::new(3, 1).expect("surface");
        surface.write(0, 0, "日", Style::default(), WidthProfile::MODERN);
        surface.write(1, 0, "X", Style::default(), WidthProfile::MODERN);

        assert_eq!(surface.cell(0, 0).expect("cell").content(), " ");
        assert_eq!(surface.cell(0, 0).expect("cell").span(), CellSpan::One);
        assert_eq!(surface.cell(1, 0).expect("cell").content(), "X");
        assert!(!surface.cell(1, 0).expect("cell").is_continuation());
    }

    #[test]
    fn changed_run_expands_over_an_unchanged_continuation() {
        let mut previous = Surface::new(2, 1).expect("surface");
        previous.write(0, 0, "日", Style::default(), WidthProfile::MODERN);
        let mut current = Surface::new(2, 1).expect("surface");
        current.write(0, 0, "本", Style::default(), WidthProfile::MODERN);

        assert_eq!(
            current.changed_runs(&previous),
            [super::ChangedRun {
                row: 0,
                start: 0,
                end: 2,
            }]
        );
    }
}
