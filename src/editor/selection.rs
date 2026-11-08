// fork from helix-core/src/selection.rs (helix ba40e54), trimmed to `sek`'s
// single char indexed line buffer.

//! Selections are the primary editing construct. Even cursors are
//! defined as a selection range.
//!
//! All positioning is done via `char` offsets into the buffer.

#![allow(dead_code)] // the fork keeps the upstream surface

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

#[inline]
fn prev_boundary(pos: usize) -> usize {
    pos.saturating_sub(1)
}

#[inline]
fn next_boundary(pos: usize, char_len: usize) -> usize {
    (pos + 1).min(char_len)
}

/// A single selection range.
///
/// A range consists of an "anchor" and "head" position in
/// the text.  The head is the part that the user moves when
/// directly extending a selection.  The head and anchor
/// can be in any order, or even share the same position.
///
/// The anchor and head positions use gap indexing, meaning
/// that their indices represent the gaps *between* `char`s
/// rather than the `char`s themselves. For example, 1
/// represents the position between the first and second `char`.
///
/// Below are some examples of `Range` configurations.
/// The anchor and head indices are shown as "(anchor, head)"
/// tuples, followed by example text with "[" and "]" symbols
/// representing the anchor and head positions:
///
/// - (0, 3): `[Som]e text`.
/// - (3, 0): `]Som[e text`.
/// - (2, 7): `So[me te]xt`.
/// - (1, 1): `S[]ome text`.
///
/// Ranges are considered to be inclusive on the left and
/// exclusive on the right, regardless of anchor-head ordering.
/// This means, for example, that non-zero-width ranges that
/// are directly adjacent, sharing an edge, do not overlap.
/// However, a zero-width range will overlap with the shared
/// left-edge of another range.
///
/// By convention, user-facing ranges are considered to have
/// a block cursor on the head-side of the range that spans a
/// single grapheme inward from the range's edge.  There are a
/// variety of helper methods on `Range` for working in terms of
/// that block cursor, all of which have `cursor` in their name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// The anchor of the range: the side that doesn't move when extending.
    pub anchor: usize,
    /// The head of the range, moved when extending.
    pub head: usize,
}

impl Range {
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn point(head: usize) -> Self {
        Self::new(head, head)
    }

    /// Start of the range.
    #[inline]
    #[must_use]
    pub fn from(&self) -> usize {
        std::cmp::min(self.anchor, self.head)
    }

    /// End of the range.
    #[inline]
    #[must_use]
    pub fn to(&self) -> usize {
        std::cmp::max(self.anchor, self.head)
    }

    /// Total length of the range.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.to() - self.from()
    }

    /// `true` when head and anchor are at the same position.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// `Direction::Backward` when head < anchor.
    /// `Direction::Forward` otherwise.
    #[inline]
    #[must_use]
    pub fn direction(&self) -> Direction {
        if self.head < self.anchor {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// Flips the direction of the selection
    pub fn flip(&self) -> Self {
        Self {
            anchor: self.head,
            head: self.anchor,
        }
    }

    /// Returns the selection if it goes in the direction of `direction`,
    /// flipping the selection otherwise.
    pub fn with_direction(self, direction: Direction) -> Self {
        if self.direction() == direction {
            self
        } else {
            self.flip()
        }
    }

    /// Check two ranges for overlap.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        // To my eye, it's non-obvious why this works, but I arrived
        // at it after transforming the slower version that explicitly
        // enumerated more cases.  The unit tests are thorough.
        self.from() == other.from() || (self.to() > other.from() && other.to() > self.from())
    }

    #[inline]
    pub fn contains_range(&self, other: &Self) -> bool {
        self.from() <= other.from() && self.to() >= other.to()
    }

    pub fn contains(&self, pos: usize) -> bool {
        self.from() <= pos && pos < self.to()
    }

    /// Extend the range to cover at least `from` `to`.
    #[must_use]
    pub fn extend(&self, from: usize, to: usize) -> Self {
        debug_assert!(from <= to);

        if self.anchor <= self.head {
            Self {
                anchor: self.anchor.min(from),
                head: self.head.max(to),
            }
        } else {
            Self {
                anchor: self.anchor.max(to),
                head: self.head.min(from),
            }
        }
    }

    /// Returns a range that encompasses both input ranges.
    ///
    /// This is like `extend()`, but tries to negotiate the
    /// anchor/head ordering between the two input ranges.
    #[must_use]
    pub fn merge(&self, other: Self) -> Self {
        if self.anchor > self.head && other.anchor > other.head {
            Range {
                anchor: self.anchor.max(other.anchor),
                head: self.head.min(other.head),
            }
        } else {
            Range {
                anchor: self.from().min(other.from()),
                head: self.to().max(other.to()),
            }
        }
    }
}

impl Range {
    //--------------------------------
    // Alignment methods.

    /// Compute a possibly new range from this range, attempting to ensure
    /// a minimum range width of 1 char by shifting the head in the forward
    /// direction as needed.
    ///
    /// This method will never shift the anchor, and will only shift the
    /// head in the forward direction.  Therefore, this method can fail
    /// at ensuring the minimum width if and only if the passed range is
    /// both zero-width and at the end of the `RopeSlice`.
    ///
    /// If the input range is grapheme-boundary aligned, the returned range
    /// will also be.  Specifically, if the head needs to shift to achieve
    /// the minimum width, it will shift to the next grapheme boundary.
    #[must_use]
    #[inline]
    pub fn min_width_1(&self, char_len: usize) -> Self {
        if self.anchor == self.head {
            Range {
                anchor: self.anchor,
                head: next_boundary(self.head, char_len),
            }
        } else {
            *self
        }
    }

    //--------------------------------
    // Block-cursor methods.

    /// Gets the left-side position of the block cursor.
    #[must_use]
    #[inline]
    pub fn cursor(self, char_len: usize) -> usize {
        if self.head > self.anchor {
            prev_boundary(self.head).min(char_len)
        } else {
            self.head
        }
    }

    /// Puts the left side of the block cursor at `char_idx`, optionally extending.
    ///
    /// This follows "1-width" semantics, and therefore does a combination of anchor
    /// and head moves to behave as if both the front and back of the range are 1-width
    /// blocks
    ///
    /// This method assumes that the range and `char_idx` are already properly
    /// grapheme-aligned.
    #[must_use]
    #[inline]
    pub fn put_cursor(self, char_len: usize, char_idx: usize, extend: bool) -> Range {
        if extend {
            let anchor = if self.head >= self.anchor && char_idx < self.anchor {
                next_boundary(self.anchor, char_len)
            } else if self.head < self.anchor && char_idx >= self.anchor {
                prev_boundary(self.anchor)
            } else {
                self.anchor
            };

            if anchor <= char_idx {
                Range::new(anchor, next_boundary(char_idx, char_len))
            } else {
                Range::new(anchor, char_idx)
            }
        } else {
            Range::point(char_idx)
        }
    }
}

impl From<(usize, usize)> for Range {
    fn from((anchor, head): (usize, usize)) -> Self {
        Self { anchor, head }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const TEXT: usize = 10;

    #[test]
    fn test_contains() {
        let range = Range::new(10, 12);

        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(11));
        assert!(!range.contains(12));
        assert!(!range.contains(13));

        let range = Range::new(9, 6);
        assert!(!range.contains(9));
        assert!(range.contains(7));
        assert!(range.contains(6));
    }

    #[test]
    fn test_overlaps() {
        fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
            Range::new(a.0, a.1).overlaps(&Range::new(b.0, b.1))
        }

        // Two non-zero-width ranges, no overlap.
        assert!(!overlaps((0, 3), (3, 6)));
        assert!(!overlaps((0, 3), (6, 3)));
        assert!(!overlaps((3, 0), (3, 6)));
        assert!(!overlaps((3, 0), (6, 3)));
        assert!(!overlaps((3, 6), (0, 3)));
        assert!(!overlaps((3, 6), (3, 0)));
        assert!(!overlaps((6, 3), (0, 3)));
        assert!(!overlaps((6, 3), (3, 0)));

        // Two non-zero-width ranges, overlap.
        assert!(overlaps((0, 4), (3, 6)));
        assert!(overlaps((0, 4), (6, 3)));
        assert!(overlaps((4, 0), (3, 6)));
        assert!(overlaps((4, 0), (6, 3)));
        assert!(overlaps((3, 6), (0, 4)));
        assert!(overlaps((3, 6), (4, 0)));
        assert!(overlaps((6, 3), (0, 4)));
        assert!(overlaps((6, 3), (4, 0)));

        // Zero-width and non-zero-width range, no overlap.
        assert!(!overlaps((0, 3), (3, 3)));
        assert!(!overlaps((3, 0), (3, 3)));
        assert!(!overlaps((3, 3), (0, 3)));
        assert!(!overlaps((3, 3), (3, 0)));

        // Zero-width and non-zero-width range, overlap.
        assert!(overlaps((1, 4), (1, 1)));
        assert!(overlaps((4, 1), (1, 1)));
        assert!(overlaps((1, 1), (1, 4)));
        assert!(overlaps((1, 1), (4, 1)));

        assert!(overlaps((1, 4), (3, 3)));
        assert!(overlaps((4, 1), (3, 3)));
        assert!(overlaps((3, 3), (1, 4)));
        assert!(overlaps((3, 3), (4, 1)));

        // Two zero-width ranges, no overlap.
        assert!(!overlaps((0, 0), (1, 1)));
        assert!(!overlaps((1, 1), (0, 0)));

        // Two zero-width ranges, overlap.
        assert!(overlaps((1, 1), (1, 1)));
    }

    #[test]
    fn test_min_width_1() {
        // Zero-width.
        assert_eq!(Range::new(0, 0).min_width_1(6), Range::new(0, 1));
        assert_eq!(Range::new(1, 1).min_width_1(6), Range::new(1, 2));
        assert_eq!(Range::new(2, 2).min_width_1(6), Range::new(2, 3));
        assert_eq!(Range::new(3, 3).min_width_1(6), Range::new(3, 4));
        assert_eq!(Range::new(4, 4).min_width_1(6), Range::new(4, 5));
        assert_eq!(Range::new(5, 5).min_width_1(6), Range::new(5, 6));
        assert_eq!(Range::new(6, 6).min_width_1(6), Range::new(6, 6));

        // Forward.
        assert_eq!(Range::new(0, 1).min_width_1(6), Range::new(0, 1));
        assert_eq!(Range::new(1, 2).min_width_1(6), Range::new(1, 2));
        assert_eq!(Range::new(2, 3).min_width_1(6), Range::new(2, 3));
        assert_eq!(Range::new(3, 4).min_width_1(6), Range::new(3, 4));
        assert_eq!(Range::new(4, 5).min_width_1(6), Range::new(4, 5));
        assert_eq!(Range::new(5, 6).min_width_1(6), Range::new(5, 6));

        // Reverse.
        assert_eq!(Range::new(1, 0).min_width_1(6), Range::new(1, 0));
        assert_eq!(Range::new(2, 1).min_width_1(6), Range::new(2, 1));
        assert_eq!(Range::new(3, 2).min_width_1(6), Range::new(3, 2));
        assert_eq!(Range::new(4, 3).min_width_1(6), Range::new(4, 3));
        assert_eq!(Range::new(5, 4).min_width_1(6), Range::new(5, 4));
        assert_eq!(Range::new(6, 5).min_width_1(6), Range::new(6, 5));
    }

    #[test]
    fn test_cursor() {
        // Zero-width ranges.
        assert_eq!(Range::new(0, 0).cursor(TEXT), 0);
        assert_eq!(Range::new(2, 2).cursor(TEXT), 2);
        assert_eq!(Range::new(9, 9).cursor(TEXT), 9);

        // Forward ranges.
        assert_eq!(Range::new(0, 2).cursor(TEXT), 1);
        assert_eq!(Range::new(0, 3).cursor(TEXT), 2);
        assert_eq!(Range::new(3, 6).cursor(TEXT), 5);

        // Reverse ranges.
        assert_eq!(Range::new(2, 0).cursor(TEXT), 0);
        assert_eq!(Range::new(6, 2).cursor(TEXT), 2);
        assert_eq!(Range::new(6, 3).cursor(TEXT), 3);
    }

    #[test]
    fn test_put_cursor() {
        // Zero-width ranges.
        assert_eq!(Range::new(0, 0).put_cursor(TEXT, 0, true), Range::new(0, 1));
        assert_eq!(Range::new(0, 0).put_cursor(TEXT, 2, true), Range::new(0, 3));
        assert_eq!(Range::new(2, 3).put_cursor(TEXT, 4, true), Range::new(2, 5));
        assert_eq!(Range::new(2, 8).put_cursor(TEXT, 4, true), Range::new(2, 5));
        assert_eq!(Range::new(8, 8).put_cursor(TEXT, 4, true), Range::new(9, 4));

        // Forward ranges.
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 0, true), Range::new(4, 0));
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 2, true), Range::new(4, 2));
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 3, true), Range::new(3, 4));
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 4, true), Range::new(3, 5));
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 6, true), Range::new(3, 7));
        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 8, true), Range::new(3, 9));

        // Reverse ranges.
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 0, true), Range::new(6, 0));
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 2, true), Range::new(6, 2));
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 3, true), Range::new(6, 3));
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 4, true), Range::new(6, 4));
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 6, true), Range::new(5, 7));
        assert_eq!(Range::new(6, 3).put_cursor(TEXT, 8, true), Range::new(5, 9));

        assert_eq!(Range::new(3, 6).put_cursor(TEXT, 5, false), Range::point(5));
    }
}
