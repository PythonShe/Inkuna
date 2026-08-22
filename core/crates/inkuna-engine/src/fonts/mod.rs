//! The bundled font set as the engine sees it: eager, validated load of
//! `assets/fonts/`, stable registry ids, and the reading/CJK/symbols
//! selection the shaper drives.

mod registry;

pub use registry::{FontAxis, FontEntry, FontRegistry, LoadedFace};
