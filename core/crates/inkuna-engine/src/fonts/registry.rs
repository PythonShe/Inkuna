//! Maps the bundled font set and issues the stable face ids everything
//! downstream (shaping, display lists, the FFI) speaks in.
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
//! | 24–27 | NotoHebrew: Serif Regular, Serif Bold, Sans Regular, Sans Bold |
//! | 28    | NotoSansSymbols2-Regular                                  |
//!
//! Hebrew ids sit between the CJK block and Symbols (Symbols stays the
//! LAST id — the fallback chain's terminal face). No Hebrew italics
//! exist upstream; italic requests map to the regular faces.
//!
//! Above the manifest sits the WEIGHT-INSTANCE block: the four Latin
//! Regular/Italic files are variable (`wght` 100–900) and expose the
//! seven non-default standard weights as `wght` instances, in this
//! fixed order (7 ids per face):
//!
//! | id    | face                    | wght per id                       |
//! |-------|-------------------------|-----------------------------------|
//! | 29–35 | NotoSerif.ttf           | 100, 200, 300, 500, 600, 800, 900 |
//! | 36–42 | NotoSerif-Italic.ttf    | 100, 200, 300, 500, 600, 800, 900 |
//! | 43–49 | NotoSans.ttf            | 100, 200, 300, 500, 600, 800, 900 |
//! | 50–56 | NotoSans-Italic.ttf     | 100, 200, 300, 500, 600, 800, 900 |
//!
//! 400 and 700 deliberately have NO instance ids: 400 is the variable
//! files' default instance (ids 0/1/4/5 unchanged) and 700 keeps the
//! static Bold files (ids 2/3/6/7), so the two ubiquitous weights render
//! exactly as they always have and `select()` stays a table lookup.
//! Instance entries reuse the base face's PostScript name and
//! default-instance metrics (line metrics stay weight-independent by
//! design); only `axes` distinguishes them, which both shells already
//! apply when building platform fonts.
//!
//! Allocation rule: blocks are append-only and fixed-size, so every id
//! is deterministic across runs and platforms. Above the fixed blocks
//! sit the DYNAMIC blocks, allocated once at load and stable for the
//! process lifetime:
//!
//! - the SYSTEM block starts at [`FIRST_DYNAMIC_ID`] (57): the platform
//!   faces the shell registered before load, ids issued append-only in
//!   the registration order the shell passed (a variable face takes
//!   nine consecutive ids — its `wght` instances at 100..=900 ascending;
//!   a static face takes one). A face that fails to load takes NO id.
//! - the PUBLISHER block (a later package) follows immediately after
//!   the system block, from [`FontRegistry::next_free_id`] upward.
//!
//! Shells prime their font tables from `font_registry()` after open, so
//! every entry exists before any session starts and ids never move.
//!
//! The CJK faces live in language-specific OTCs (one file per
//! family+weight); `collection_index` picks the region face inside the
//! collection. The Sans OTCs also carry Mono faces at indices 5–9,
//! which the registry never references.

use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;
use read_fonts::FontRef;

use crate::error::EngineError;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

use super::face::{font_metrics, map_font, missing, post_script_name, wght_range};
use super::system::{load_system_faces, FaceSlots, SystemFontFace, SystemFontWarning};

/// One variation-axis coordinate a face is used at. Empty for the
/// manifest ids 0–28 (default instances / static faces); the
/// weight-instance ids each carry exactly one `wght` coordinate.
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
    /// PostScript name read from the selected face's `name` table.
    pub post_script_name: String,
    pub axes: Vec<FontAxis>,
}

/// A parsed, memory-mapped face. harfrust's `FontRef` borrows the
/// mapped bytes, so shaping call sites rebuild it from `data` +
/// `collection_index` per shape (cheap; revisit with `self_cell` only
/// if profiling demands).
#[derive(Debug, Clone)]
pub struct LoadedFace {
    pub data: Arc<Mmap>,
    pub upem: u16,
    pub ascender: i32,
    pub descender: i32,
    pub collection_index: u32,
    /// The variation coordinates shaping applies (mirrors the entry's
    /// `axes`). Empty for manifest faces; `[wght]` for instance ids.
    pub axes: Vec<FontAxis>,
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
const MANIFEST: [ManifestRow; 29] = [
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
    row("NotoSerifHebrew-Regular.ttf", 0),
    row("NotoSerifHebrew-Bold.ttf", 0),
    row("NotoSansHebrew-Regular.ttf", 0),
    row("NotoSansHebrew-Bold.ttf", 0),
    row("NotoSansSymbols2-Regular.ttf", 0),
];

const SYMBOLS_ID: u32 = 28;
const CJK_BASE_ID: u32 = 8;
const CJK_FAMILY_STRIDE: u32 = 8; // serif block → sans block
const HEBREW_BASE_ID: u32 = 24;
const HEBREW_FAMILY_STRIDE: u32 = 2; // serif pair → sans pair

/// The variable Latin faces that grow weight instances, in block order
/// (the module doc's instance table). The `u32` is the face's manifest
/// id — its file, metrics, and PostScript name back the instances.
const VARIABLE_FACES: [u32; 4] = [0, 1, 4, 5];
/// The static Bold/BoldItalic ids paired with [`VARIABLE_FACES`], used
/// for weight 700.
const STATIC_BOLD_IDS: [u32; 4] = [2, 3, 6, 7];
/// The `wght` coordinates instanced per variable face, in id order.
/// 400 (default instance) and 700 (static Bold files) are absent on
/// purpose — see the module doc.
const INSTANCE_WEIGHTS: [u16; 7] = [100, 200, 300, 500, 600, 800, 900];
/// First id of the weight-instance block.
const INSTANCE_BASE_ID: u32 = SYMBOLS_ID + 1;
/// First id available to future dynamically-registered faces
/// (system/publisher fonts). Everything below is fixed at load.
pub const FIRST_DYNAMIC_ID: u32 =
    INSTANCE_BASE_ID + (VARIABLE_FACES.len() * INSTANCE_WEIGHTS.len()) as u32;

/// The loaded, validated font set: the fixed bundled blocks plus the
/// dynamic system block registered at load. Immutable once built — ids
/// are stable for the process lifetime.
pub struct FontRegistry {
    entries: Vec<FontEntry>,
    faces: Vec<LoadedFace>,
    /// The registered platform faces `System*` requests resolve from.
    system: FaceSlots,
    /// The publisher-embedded faces `Publisher` requests resolve from.
    /// Deliberately empty until publisher registration lands (B2): the
    /// lookup path is real, its data is not — so `Publisher` explicitly
    /// falls back to NotoSerif at selection time today.
    publisher: FaceSlots,
}

impl FontRegistry {
    /// The bundled set alone — [`FontRegistry::load_with_system`] with
    /// no system faces.
    pub fn load(font_dir: &Path) -> Result<Arc<FontRegistry>, EngineError> {
        Ok(Self::load_with_system(font_dir, &[])?.0)
    }

    /// Maps and parses EVERY manifest face, so per-face failure after a
    /// successful load is impossible by construction. The OS faults map
    /// pages in only as parsing and shaping touch them. Load once per
    /// process, off the UI thread.
    ///
    /// `system` is the shell's platform face set, registered into the
    /// dynamic block in call order (see the module doc). System faces
    /// degrade instead of failing: each unloadable one is skipped with
    /// a returned [`SystemFontWarning`], and only a broken BUNDLED set
    /// errors.
    pub fn load_with_system(
        font_dir: &Path,
        system: &[SystemFontFace],
    ) -> Result<(Arc<FontRegistry>, Vec<SystemFontWarning>), EngineError> {
        let mut entries = Vec::with_capacity(FIRST_DYNAMIC_ID as usize);
        let mut faces = Vec::with_capacity(FIRST_DYNAMIC_ID as usize);
        // Maps cached per file: the four OTCs each back eight ids.
        let mut cache: Vec<(&'static str, Arc<Mmap>)> = Vec::new();
        for (id, spec) in MANIFEST.iter().enumerate() {
            let path = font_dir.join(spec.file);
            let data = match cache.iter().find(|(name, _)| *name == spec.file) {
                Some((_, data)) => Arc::clone(data),
                None => {
                    let data = Arc::new(map_font(&path, spec.file)?);
                    cache.push((spec.file, Arc::clone(&data)));
                    data
                }
            };
            let font = FontRef::from_index(&data, spec.collection_index)
                .map_err(|e| missing(spec.file, &e))?;
            let (upem, ascender, descender) =
                font_metrics(&font).map_err(|e| missing(spec.file, &e))?;
            if upem == 0 {
                return Err(missing(spec.file, &"units_per_em is 0"));
            }
            let post_script_name = post_script_name(&font)
                .map_err(|e| missing(spec.file, &e))?
                .ok_or_else(|| missing(spec.file, &"PostScript name is missing"))?;
            let abs = std::path::absolute(&path)
                .map_err(|e| missing(spec.file, &e))?
                .to_string_lossy()
                .into_owned();
            entries.push(FontEntry {
                id: id as u32,
                file_path: abs,
                collection_index: spec.collection_index,
                post_script_name,
                axes: Vec::new(),
            });
            faces.push(LoadedFace {
                data,
                upem,
                ascender,
                descender,
                collection_index: spec.collection_index,
                axes: Vec::new(),
            });
        }

        // The weight-instance block (module doc): seven `wght`
        // coordinates per variable Latin face, ids issued in fixed
        // order right above the manifest. Coordinates are validated
        // against the face's actual fvar range and clamped into it, so
        // an upstream font swap can narrow rendering but never break
        // determinism or id allocation.
        for base_id in VARIABLE_FACES {
            let base_entry = entries[base_id as usize].clone();
            let base_face = faces[base_id as usize].clone();
            let file = MANIFEST[base_id as usize].file;
            let font =
                FontRef::from_index(&base_face.data, 0).map_err(|e| missing(file, &e))?;
            let (min, max) = wght_range(&font).ok_or_else(|| {
                missing(file, &"expected a variable font with a wght axis")
            })?;
            for weight in INSTANCE_WEIGHTS {
                let axes = vec![FontAxis {
                    tag: "wght".to_string(),
                    value: f64::from(weight).clamp(min, max),
                }];
                entries.push(FontEntry {
                    id: entries.len() as u32,
                    file_path: base_entry.file_path.clone(),
                    collection_index: 0,
                    // Instances keep the base face's PostScript
                    // identity; `axes` is what distinguishes them and
                    // what the shells instantiate platform fonts with.
                    post_script_name: base_entry.post_script_name.clone(),
                    axes: axes.clone(),
                });
                // Default-instance metrics on purpose: line metrics
                // stay weight-independent, so toggling weight never
                // reflows line heights.
                faces.push(LoadedFace {
                    data: Arc::clone(&base_face.data),
                    upem: base_face.upem,
                    ascender: base_face.ascender,
                    descender: base_face.descender,
                    collection_index: 0,
                    axes,
                });
            }
        }
        debug_assert_eq!(entries.len() as u32, FIRST_DYNAMIC_ID);

        // The system block: ids from FIRST_DYNAMIC_ID upward in the
        // shell's registration order; failures skip with warnings.
        let (system_slots, warnings) = load_system_faces(system, &mut entries, &mut faces);
        Ok((
            Arc::new(FontRegistry {
                entries,
                faces,
                system: system_slots,
                publisher: FaceSlots::default(),
            }),
            warnings,
        ))
    }

    /// The full face table, for the FFI. Never forces byte loads —
    /// entries carry paths, not data.
    pub fn entries(&self) -> Vec<FontEntry> {
        self.entries.clone()
    }

    /// The first id the next dynamic block (publisher faces, B2) will
    /// allocate from — one past the system block.
    pub fn next_free_id(&self) -> u32 {
        self.entries.len() as u32
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
    ///
    /// `System*` requests resolve from the registered system faces
    /// (style + numeric weight, CSS nearest rule; an italic request
    /// with no italic face synthesizes from the uprights) and fall back
    /// to the Noto equivalent when the role has no registered upright.
    /// `Publisher` resolves from the publisher slots — empty until B2 —
    /// so it falls back to NotoSerif today. The bundled Notos map their
    /// numeric weight onto the nine standard weights via
    /// [`nearest_standard_weight`]: 400 keeps the base ids, 700 the
    /// static Bold ids, everything else its instance id.
    pub fn select(&self, family: FontFamily, style: FontStyle, weight: FontWeight) -> u32 {
        let dynamic = match family {
            FontFamily::Publisher => self.publisher.select(true, style, weight.value()),
            FontFamily::SystemSerif => self.system.select(true, style, weight.value()),
            FontFamily::SystemSans => self.system.select(false, style, weight.value()),
            FontFamily::NotoSerif | FontFamily::NotoSans => None,
        };
        if let Some(id) = dynamic {
            return id;
        }
        let face = match (family.is_serif(), style) {
            (true, FontStyle::Normal) => 0usize,
            (true, FontStyle::Italic) => 1,
            (false, FontStyle::Normal) => 2,
            (false, FontStyle::Italic) => 3,
        };
        match nearest_standard_weight(weight) {
            400 => VARIABLE_FACES[face],
            700 => STATIC_BOLD_IDS[face],
            snapped => {
                // Present by construction: `snapped` is one of the nine
                // standard weights and 400/700 matched above.
                let slot = INSTANCE_WEIGHTS
                    .iter()
                    .position(|&w| w == snapped)
                    .unwrap_or(0);
                INSTANCE_BASE_ID + (face * INSTANCE_WEIGHTS.len() + slot) as u32
            }
        }
    }

    /// The CJK face id for a BCP-47 language tag: `ja*` → JP, `ko*` →
    /// KR, `zh-Hant`/`zh-TW`/`zh-HK` → TC, everything else (incl.
    /// `zh`, `zh-Hans`, `None`) → SC. Italic does not exist in CJK;
    /// `style_serif` picks the family.
    pub fn cjk(&self, lang: Option<&str>, style_serif: bool, weight: FontWeight) -> u32 {
        let region = cjk_region(lang);
        let family = if style_serif { 0 } else { CJK_FAMILY_STRIDE };
        // Static R/B pairs threshold at 600 (the WebView-era semantic).
        let bold = u32::from(weight.is_bold());
        CJK_BASE_ID + family + region * 2 + bold
    }

    /// The Hebrew face id for a family + weight. No Hebrew italics
    /// exist upstream, so callers with an italic request pass their
    /// family/weight here unchanged — italic maps to regular by
    /// construction. `style_serif` picks the family.
    pub fn hebrew(&self, style_serif: bool, weight: FontWeight) -> u32 {
        let family = if style_serif { 0 } else { HEBREW_FAMILY_STRIDE };
        // Static R/B pairs threshold at 600 (the WebView-era semantic).
        let bold = u32::from(weight.is_bold());
        HEBREW_BASE_ID + family + bold
    }

    /// The terminal fallback face before `.notdef`.
    pub fn symbols(&self) -> u32 {
        SYMBOLS_ID
    }
}

/// The CSS font-matching weight rule (css-fonts-4 §5.2) over the nine
/// standard weights 100..=900, which the Latin roster covers in full:
/// - desired 400..=500: 400 stays 400, otherwise the first weight
///   ascending toward 500 (⇒ 500);
/// - desired < 400: weights below first (⇒ floor to the lower hundred,
///   or 100 when nothing sits below);
/// - desired > 500: weights above first (⇒ ceil to the upper hundred,
///   or 900 when nothing sits above).
fn nearest_standard_weight(weight: FontWeight) -> u16 {
    let w = weight.value();
    if (100..=900).contains(&w) && w % 100 == 0 {
        return w;
    }
    match w {
        ..400 => (w / 100 * 100).max(100),
        400..=500 => 500, // exact 400 returned above
        _ => (w.div_ceil(100) * 100).min(900),
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

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
