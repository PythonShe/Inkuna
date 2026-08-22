//! The one coordinate type the whole system shares.

/// A position in a publication: a spine resource plus a Unicode-scalar
/// offset into that resource's canonical [`super::Projection`] stream.
/// `inkuna-core` re-exports this and the FFI mirrors it; every stored
/// position, locator, and search hit speaks these units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coordinate {
    /// Index into the spine, in reading order.
    pub spine_idx: u32,
    /// Unicode scalar count into the resource's projection.
    pub char_offset: u64,
}
