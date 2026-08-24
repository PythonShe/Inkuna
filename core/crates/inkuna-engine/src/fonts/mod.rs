//! The engine's font set: validated memory maps of the bundled
//! `assets/fonts/` plus the shell-registered platform system faces,
//! stable registry ids, and the reading/CJK/symbols selection the
//! shaper drives.

mod face;
mod registry;
mod system;

pub use registry::{FontAxis, FontEntry, FontRegistry, LoadedFace, FIRST_DYNAMIC_ID};
pub use system::{SystemFontFace, SystemFontRole, SystemFontWarning};
