//! Loads the bundled font set eagerly and issues the stable face ids
//! everything downstream (shaping, display lists, the FFI) speaks in.
//!
//! Id order is FIXED and documented here — stable across platforms
//! because the file set is fixed:
//!
//! | id    | face                                                      |
//! |-------|-----------------------------------------------------------|
//! | 0–3   | NotoSerif: Regular, Italic, Bold, BoldItalic              |
//! | 4–7   | NotoSans: Regular, Italic, Bold, BoldItalic               |
//! | 8–15  | NotoSerifCJK: SC, TC, JP, KR — Regular before Bold        |
//! | 16–23 | NotoSansCJK: SC, TC, JP, KR — Regular before Bold         |
//! | 24    | NotoSansSymbols2-Regular                                  |
//!
//! The CJK faces live in language-specific OTCs (one file per
//! family+weight); `collection_index` picks the region face inside the
//! collection. The Sans OTCs also carry Mono faces at indices 5–9,
//! which the registry never references.

use std::path::Path;
use std::sync::Arc;

use crate::error::EngineError;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

/// One variation-axis coordinate a face is used at. Empty everywhere
/// today: the variable Latin regular/italic files shape at their
/// default instances, and the CJK OTCs are static.
#[derive(Debug, Clone, PartialEq)]
pub struct FontAxis {
    pub tag: String,
    pub value: f64,
}

/// One registry face, as the FFI hands it to the shells (they rebuild
/// platform fonts from `file_path` + `collection_index`).
#[derive(Debug, Clone, PartialEq)]
pub struct FontEntry {
    pub id: u32,
    /// Absolute path under the registry's font dir.
    pub file_path: String,
    pub collection_index: u32,
    pub axes: Vec<FontAxis>,
}

/// A parsed, memory-resident face. rustybuzz's `Face` borrows the
/// bytes, so shaping call sites rebuild it from `data` +
/// `collection_index` per shape (cheap; revisit with `self_cell` only
/// if profiling demands).
#[derive(Debug, Clone)]
pub struct LoadedFace {
    pub data: Arc<Vec<u8>>,
    pub upem: u16,
    pub collection_index: u32,
}

/// The manifest row: file name and index into a collection (0 for
/// single faces). Order here IS the id order.
struct ManifestRow {
    file: &'static str,
    collection_index: u32,
}

const fn row(file: &'static str, collection_index: u32) -> ManifestRow {
    ManifestRow {
        file,
        collection_index,
    }
}

/// OTC region indices, per the noto-cjk collection layout
/// (jp, kr, sc, tc, hk — proven by the 3.2 loader smoke test).
const OTC_JP: u32 = 0;
const OTC_KR: u32 = 1;
const OTC_SC: u32 = 2;
const OTC_TC: u32 = 3;

const SERIF_CJK_REGULAR: &str = "NotoSerifCJK-Regular.ttc";
const SERIF_CJK_BOLD: &str = "NotoSerifCJK-Bold.ttc";
const SANS_CJK_REGULAR: &str = "NotoSansCJK-Regular.ttc";
const SANS_CJK_BOLD: &str = "NotoSansCJK-Bold.ttc";

/// The fixed face set, in id order.
const MANIFEST: [ManifestRow; 25] = [
    row("NotoSerif.ttf", 0),
    row("NotoSerif-Italic.ttf", 0),
    row("NotoSerif-Bold.ttf", 0),
    row("NotoSerif-BoldItalic.ttf", 0),
    row("NotoSans.ttf", 0),
    row("NotoSans-Italic.ttf", 0),
    row("NotoSans-Bold.ttf", 0),
    row("NotoSans-BoldItalic.ttf", 0),
    row(SERIF_CJK_REGULAR, OTC_SC),
    row(SERIF_CJK_BOLD, OTC_SC),
    row(SERIF_CJK_REGULAR, OTC_TC),
    row(SERIF_CJK_BOLD, OTC_TC),
    row(SERIF_CJK_REGULAR, OTC_JP),
    row(SERIF_CJK_BOLD, OTC_JP),
    row(SERIF_CJK_REGULAR, OTC_KR),
    row(SERIF_CJK_BOLD, OTC_KR),
    row(SANS_CJK_REGULAR, OTC_SC),
    row(SANS_CJK_BOLD, OTC_SC),
    row(SANS_CJK_REGULAR, OTC_TC),
    row(SANS_CJK_BOLD, OTC_TC),
    row(SANS_CJK_REGULAR, OTC_JP),
    row(SANS_CJK_BOLD, OTC_JP),
    row(SANS_CJK_REGULAR, OTC_KR),
    row(SANS_CJK_BOLD, OTC_KR),
    row("NotoSansSymbols2-Regular.ttf", 0),
];

const SYMBOLS_ID: u32 = 24;
const CJK_BASE_ID: u32 = 8;
const CJK_FAMILY_STRIDE: u32 = 8; // serif block → sans block

/// The loaded, validated bundled font set.
pub struct FontRegistry {
    entries: Vec<FontEntry>,
    faces: Vec<LoadedFace>,
}

impl FontRegistry {
    /// Reads and parses EVERY manifest face eagerly, so per-face
    /// failure after a successful load is impossible by construction.
    /// Load once per process, off the UI thread.
    pub fn load(font_dir: &Path) -> Result<Arc<FontRegistry>, EngineError> {
        let mut entries = Vec::with_capacity(MANIFEST.len());
        let mut faces = Vec::with_capacity(MANIFEST.len());
        // Font bytes cached per file: the four OTCs each back eight ids.
        let mut cache: Vec<(&'static str, Arc<Vec<u8>>)> = Vec::new();
        for (id, spec) in MANIFEST.iter().enumerate() {
            let path = font_dir.join(spec.file);
            let data = match cache.iter().find(|(name, _)| *name == spec.file) {
                Some((_, data)) => Arc::clone(data),
                None => {
                    let bytes = std::fs::read(&path).map_err(|e| missing(spec.file, &e))?;
                    let data = Arc::new(bytes);
                    cache.push((spec.file, Arc::clone(&data)));
                    data
                }
            };
            let face = rustybuzz::ttf_parser::Face::parse(&data, spec.collection_index)
                .map_err(|e| missing(spec.file, &e))?;
            let upem = face.units_per_em();
            if upem == 0 {
                return Err(missing(spec.file, &"units_per_em is 0"));
            }
            let abs = std::path::absolute(&path)
                .map_err(|e| missing(spec.file, &e))?
                .to_string_lossy()
                .into_owned();
            entries.push(FontEntry {
                id: id as u32,
                file_path: abs,
                collection_index: spec.collection_index,
                axes: Vec::new(),
            });
            faces.push(LoadedFace {
                data,
                upem,
                collection_index: spec.collection_index,
            });
        }
        Ok(Arc::new(FontRegistry { entries, faces }))
    }

    /// The full face table, for the FFI. Never forces byte loads —
    /// entries carry paths, not data.
    pub fn entries(&self) -> Vec<FontEntry> {
        self.entries.clone()
    }

    /// Panic-free: ids are registry-issued; an out-of-range id (which
    /// no caller can obtain from this registry) falls back to face 0,
    /// which `load` guarantees exists.
    pub fn face(&self, id: u32) -> &LoadedFace {
        match self.faces.get(id as usize) {
            Some(face) => face,
            None => &self.faces[0],
        }
    }

    /// The reading-face id for a family + style + weight.
    pub fn select(&self, family: FontFamily, style: FontStyle, weight: FontWeight) -> u32 {
        let base = match family {
            FontFamily::NotoSerif => 0,
            FontFamily::NotoSans => 4,
        };
        let bold = match weight {
            FontWeight::Normal => 0,
            FontWeight::Bold => 2,
        };
        let italic = match style {
            FontStyle::Normal => 0,
            FontStyle::Italic => 1,
        };
        base + bold + italic
    }

    /// The CJK face id for a BCP-47 language tag: `ja*` → JP, `ko*` →
    /// KR, `zh-Hant`/`zh-TW`/`zh-HK` → TC, everything else (incl.
    /// `zh`, `zh-Hans`, `None`) → SC. Italic does not exist in CJK;
    /// `style_serif` picks the family.
    pub fn cjk(&self, lang: Option<&str>, style_serif: bool, weight: FontWeight) -> u32 {
        let region = cjk_region(lang);
        let family = if style_serif { 0 } else { CJK_FAMILY_STRIDE };
        let bold = match weight {
            FontWeight::Normal => 0,
            FontWeight::Bold => 1,
        };
        CJK_BASE_ID + family + region * 2 + bold
    }

    /// The terminal fallback face before `.notdef`.
    pub fn symbols(&self) -> u32 {
        SYMBOLS_ID
    }
}

/// Region offset in id order: SC 0, TC 1, JP 2, KR 3.
fn cjk_region(lang: Option<&str>) -> u32 {
    let Some(lang) = lang else { return 0 };
    let lower = lang.to_ascii_lowercase();
    let mut subtags = lower.split(['-', '_']);
    match subtags.next() {
        Some("ja") => 2,
        Some("ko") => 3,
        Some("zh") => {
            if subtags.any(|s| matches!(s, "hant" | "tw" | "hk")) {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn missing(file: &str, cause: &dyn std::fmt::Display) -> EngineError {
    EngineError::UnsupportedContent {
        detail: format!("font missing: {file} ({cause})"),
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
