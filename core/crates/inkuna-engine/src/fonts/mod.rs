//! The engine's font set: validated memory maps of the bundled
//! `assets/fonts/`, the shell-registered platform system faces, and the
//! per-session publisher (EPUB-embedded) faces — stable registry ids
//! and the reading/CJK/symbols selection the shaper drives.

mod extract;
mod face;
mod publisher;
mod registry;
mod system;

pub use extract::extract_publisher_fonts;
pub use publisher::PublisherFaceSpec;
pub use registry::{
    ChainFace, FontAxis, FontEntry, FontRegistry, LoadedFace, ReadingChain, FIRST_DYNAMIC_ID,
};
pub use system::{SystemFontFace, SystemFontRole, SystemFontWarning};
