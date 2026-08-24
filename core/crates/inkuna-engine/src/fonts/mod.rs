//! The bundled font set as the engine sees it: validated memory maps of
//! `assets/fonts/`, stable registry ids, and the reading/CJK/symbols
//! selection the shaper drives.

mod registry;

pub use registry::{FontAxis, FontEntry, FontRegistry, LoadedFace, FIRST_DYNAMIC_ID};
