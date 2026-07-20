/// A terminal cell coordinate in the signed 32-bit geometry domain
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Point {
    /// The horizontal coordinate
    pub x: i32,
    /// The vertical coordinate
    pub y: i32,
}

impl Point {
    /// Creates a point from horizontal and vertical coordinates
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Returns the point translated by `dx` and `dy`, or `None` on overflow
    #[must_use]
    pub fn translated(self, dx: i32, dy: i32) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(dx)?,
            y: self.y.checked_add(dy)?,
        })
    }
}

/// A non-negative terminal cell size
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Size {
    /// The number of horizontal cells
    pub width: u32,
    /// The number of vertical cells
    pub height: u32,
}

impl Size {
    /// Creates a size from its width and height
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Reports whether either dimension is zero
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// A half-open terminal cell rectangle
///
/// The covered region is `[x, x + width) x [y, y + height)`. Endpoint
/// calculations use a signed 64-bit intermediate domain
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Rect {
    /// The horizontal coordinate of the origin
    pub x: i32,
    /// The vertical coordinate of the origin
    pub y: i32,
    /// The number of horizontal cells
    pub width: u32,
    /// The number of vertical cells
    pub height: u32,
}

impl Rect {
    /// Creates a rectangle from its origin and size
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Creates a rectangle from structured origin and size values
    #[must_use]
    pub const fn from_parts(origin: Point, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    /// Returns the rectangle origin
    #[must_use]
    pub const fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Returns the rectangle size
    #[must_use]
    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Reports whether either dimension is zero
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Reports whether the half-open rectangle contains `point`
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        if self.is_empty() {
            return false;
        }

        let point_x = i64::from(point.x);
        let point_y = i64::from(point.y);
        point_x >= i64::from(self.x)
            && point_x < self.right()
            && point_y >= i64::from(self.y)
            && point_y < self.bottom()
    }

    /// Returns the half-open intersection with `other`
    ///
    /// Disjoint rectangles produce an empty rectangle whose origin is the
    /// component-wise maximum of the two origins
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        Self::new(
            x,
            y,
            intersection_extent(i64::from(x), right),
            intersection_extent(i64::from(y), bottom),
        )
    }

    /// Returns the rectangle translated by `dx` and `dy`, or `None` when its
    /// origin would leave the signed 32-bit geometry domain
    #[must_use]
    pub fn translated(self, dx: i32, dy: i32) -> Option<Self> {
        let origin = self.origin().translated(dx, dy)?;
        Some(Self::from_parts(origin, self.size()))
    }

    fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

fn intersection_extent(start: i64, end: i64) -> u32 {
    let extent = end.saturating_sub(start).max(0);
    u32::try_from(extent).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Point, Rect, Size};

    #[test]
    fn endpoint_math_does_not_overflow() {
        let rect = Rect::new(i32::MAX, i32::MAX, u32::MAX, u32::MAX);

        assert!(rect.contains(Point::new(i32::MAX, i32::MAX)));
        assert_eq!(rect.intersection(rect), rect);
    }

    #[test]
    fn parts_round_trip() {
        let origin = Point::new(-4, 9);
        let size = Size::new(12, 3);

        let rect = Rect::from_parts(origin, size);

        assert_eq!(rect.origin(), origin);
        assert_eq!(rect.size(), size);
    }
}
