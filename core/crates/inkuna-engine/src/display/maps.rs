//! The per-page query maps: the bidirectional locate/hit-test data the
//! session layer answers `hit_test` and `selection_rects` from. Geometry
//! stays fixed-point here — these maps never cross the FFI; the session
//! converts to floats at its own boundary.

use std::ops::Range;

use crate::fixed::Fx;
use crate::paginate::FxRect;

/// One page's maps: the page's canonical char range plus one
/// [`LineGeom`] per placed line, in ascending char order.
#[derive(Debug, Clone)]
pub struct PageMaps {
    pub char_range: Range<u64>,
    pub lines: Vec<LineGeom>,
}

/// One line's hit/selection geometry.
///
/// Writing-mode awareness lives in the band's orientation: in horizontal
/// writing the band spans the full frame width and its height is the
/// line's block extent (ruby included); in vertical-rl it spans the full
/// frame height and its width is the column's block extent. Cells run
/// along the inline axis — absolute page `x` in horizontal writing,
/// absolute page `y` in vertical — so nearest-band + nearest-cell
/// resolves any point on the page to a char.
#[derive(Debug, Clone)]
pub struct LineGeom {
    /// Canonical char range the line covers.
    pub char_range: Range<u64>,
    /// Full-bleed hit band across the frame.
    pub band: FxRect,
    /// `(char, inline start, inline extent)` per glyph cluster, in
    /// visual order. Chars without glyphs (separators) have no cell;
    /// hit tests resolve to the nearest cell.
    pub cells: Vec<(u64, Fx, Fx)>,
}
