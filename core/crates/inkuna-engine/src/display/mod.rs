//! Display-list emission: laid pages → the engine-native records the FFI
//! mirrors 1:1 (glyph runs, images, decorations, link regions, a11y
//! blocks), the bidirectional locate/hit-test maps, and the canonical
//! serialization + blake3 digest that proves cross-platform identity.
//!
//! The `Fx` → float conversion happens here and ONLY here (`Fx::to_f32`);
//! everything upstream is fixed-point, so the emitted bit patterns are
//! identical on every platform by construction. No hash-ordered
//! collection orders anything in this module.

mod a11y;
mod digest;
mod list;
mod maps;

#[cfg(test)]
mod tests;

pub use digest::{digest_hex, page_digest};
pub use list::{
    build_page, A11yBlock, A11yRole, ColorRole, Decoration, DecorationKind, DisplayContext,
    GlyphRun, ImagePlacement, LinkRegion, PageDisplayList, Rect,
};
pub use maps::{LineGeom, PageMaps};
