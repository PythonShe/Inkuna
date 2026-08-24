//! Dynamically registered platform system faces: the types the shells
//! describe their serif/sans system fonts with, the loader that appends
//! them to the registry's dynamic id block, and the weight-slot
//! selector `select()` resolves `System*` (and later publisher)
//! requests against.
//!
//! Degradation contract: a face that fails to load is SKIPPED with a
//! [`SystemFontWarning`] the shell can inspect — never an error, never
//! a crash. A role with no usable upright face simply resolves to the
//! bundled Noto equivalent at selection time.

use std::sync::Arc;

use read_fonts::FontRef;

use crate::style::FontStyle;

use super::face::{font_metrics, map_font, post_script_name, wght_range};
use super::registry::{FontAxis, FontEntry, LoadedFace};

/// Which reading role a registered system face serves. The fallback
/// stages (CJK/Hebrew/Symbols) always stay the bundled Notos — system
/// faces only replace the Reading stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemFontRole {
    Serif,
    Sans,
}

/// One platform face the shell hands the core before the first reader
/// open. iOS resolves a CTFont's file URL + PostScript name; Android
/// uses `SystemFonts` (file path + ttcIndex).
#[derive(Debug, Clone, PartialEq)]
pub struct SystemFontFace {
    pub role: SystemFontRole,
    pub italic: bool,
    /// Declared CSS weight of a STATIC face (clamped to 1..=1000).
    /// Variable faces ignore it: they register the nine standard
    /// weights as `wght` instances instead.
    pub weight: u16,
    pub file_path: String,
    /// When given, the collection indices are scanned for the face with
    /// this PostScript name; otherwise `ttc_hint` (default 0) picks it.
    pub post_script_name: Option<String>,
    pub ttc_hint: Option<u32>,
}

/// One skipped face: which file and why, for shell-side logging. The
/// registry stays fully usable — the affected role falls back to Noto.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemFontWarning {
    pub file_path: String,
    pub detail: String,
}

/// The nine standard CSS weights a variable system face is instanced
/// at, in id order — mirroring the bundled weight-instance block's
/// design, except system faces have no static Bold twin so 400 and 700
/// are instances like the rest.
const SYSTEM_INSTANCE_WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// Collection-index scan cap for PostScript-name matching — the
/// bounded-resource discipline applied to attacker-shaped input (a
/// platform .ttc is trusted, but bounded is bounded).
const MAX_TTC_SCAN: u32 = 64;

/// Registered faces for the serif/sans × upright/italic slots, each a
/// `(weight, id)` list in ascending weight order. (Publisher faces use
/// their own family table in `publisher.rs`, not these slots.)
#[derive(Debug, Default, Clone)]
pub(super) struct FaceSlots {
    slots: [Vec<(u16, u32)>; 4],
}

impl FaceSlots {
    fn index(serif: bool, italic: bool) -> usize {
        usize::from(!serif) * 2 + usize::from(italic)
    }

    fn push(&mut self, serif: bool, italic: bool, weight: u16, id: u32) {
        let slot = &mut self.slots[Self::index(serif, italic)];
        // Ascending weight order; first registration of a weight wins
        // (append-only ids, deterministic selection).
        if slot.iter().any(|(w, _)| *w == weight) {
            return;
        }
        let at = slot.partition_point(|(w, _)| *w < weight);
        slot.insert(at, (weight, id));
    }

    /// The face id for a style + weight, or `None` when the role has no
    /// usable upright face (the caller then falls back to Noto).
    ///
    /// A role counts as registered only when its UPRIGHT slot is
    /// non-empty — an italic-only registration falls back to Noto
    /// entirely rather than rendering body text italic. An italic
    /// request with no italic face synthesizes from the uprights.
    pub(super) fn select(&self, serif: bool, style: FontStyle, weight: u16) -> Option<u32> {
        let upright = &self.slots[Self::index(serif, false)];
        if upright.is_empty() {
            return None;
        }
        let italic = &self.slots[Self::index(serif, true)];
        let candidates = if style == FontStyle::Italic && !italic.is_empty() {
            italic
        } else {
            upright
        };
        nearest_weight(candidates, weight)
    }
}

/// The CSS font-matching weight rule (css-fonts-4 §5.2) over an
/// arbitrary ascending candidate list:
/// - desired 400..=500: exact, else ascending toward 500, else
///   descending below, else ascending above 500;
/// - desired < 400: exact, else descending below, else ascending above;
/// - desired > 500: exact, else ascending above, else descending below.
pub(super) fn nearest_weight(candidates: &[(u16, u32)], desired: u16) -> Option<u32> {
    let id = |pair: &(u16, u32)| pair.1;
    if let Some(found) = candidates.iter().find(|(w, _)| *w == desired) {
        return Some(id(found));
    }
    let below = candidates.iter().rev().find(|(w, _)| *w < desired);
    let above = candidates.iter().find(|(w, _)| *w > desired);
    match desired {
        400..=500 => candidates
            .iter()
            .find(|(w, _)| *w > desired && *w <= 500)
            .or(below)
            .or(above)
            .map(id),
        ..400 => below.or(above).map(id),
        _ => above.or(below).map(id),
    }
}

/// Appends every loadable system face to `entries`/`faces` (ids issued
/// in call order from the current length) and returns the selection
/// slots plus one warning per skipped face. Infallible by design — see
/// the module doc's degradation contract.
pub(super) fn load_system_faces(
    system: &[SystemFontFace],
    entries: &mut Vec<FontEntry>,
    faces: &mut Vec<LoadedFace>,
) -> (FaceSlots, Vec<SystemFontWarning>) {
    let mut slots = FaceSlots::default();
    let mut warnings = Vec::new();
    // Maps cached per file: several roles may share one .ttc.
    let mut cache: Vec<(String, Arc<memmap2::Mmap>)> = Vec::new();
    for face in system {
        match load_one(face, &mut cache, entries, faces) {
            Ok(registered) => {
                for (weight, id) in registered {
                    slots.push(
                        face.role == SystemFontRole::Serif,
                        face.italic,
                        weight,
                        id,
                    );
                }
            }
            Err(detail) => warnings.push(SystemFontWarning {
                file_path: face.file_path.clone(),
                detail,
            }),
        }
    }
    (slots, warnings)
}

/// Loads one declared face: maps the file, locates the collection
/// index, validates it parses, and registers either the static face or
/// its nine weight instances. Returns the `(weight, id)` pairs it
/// registered, or the reason it was skipped.
fn load_one(
    face: &SystemFontFace,
    cache: &mut Vec<(String, Arc<memmap2::Mmap>)>,
    entries: &mut Vec<FontEntry>,
    faces: &mut Vec<LoadedFace>,
) -> Result<Vec<(u16, u32)>, String> {
    let path = std::path::Path::new(&face.file_path);
    let data = match cache.iter().find(|(name, _)| *name == face.file_path) {
        Some((_, data)) => Arc::clone(data),
        None => {
            let data = Arc::new(
                map_font(path, &face.file_path).map_err(|e| e.to_string())?,
            );
            cache.push((face.file_path.clone(), Arc::clone(&data)));
            data
        }
    };
    let (collection_index, font) = locate_face(&data, face)?;
    let (upem, ascender, descender) = font_metrics(&font).map_err(|e| e.to_string())?;
    if upem == 0 {
        return Err("units_per_em is 0".to_string());
    }
    // Identity for the shells' verification: the parsed name wins; a
    // face with none keeps the caller's claim; neither → skip, because
    // the shells rebuild platform fonts against this name.
    let parsed_name = post_script_name(&font).map_err(|e| e.to_string())?;
    let post_script = match parsed_name.or_else(|| face.post_script_name.clone()) {
        Some(name) => name,
        None => return Err("PostScript name is missing".to_string()),
    };
    let abs = std::path::absolute(path)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();

    let mut registered = Vec::new();
    let mut push = |weight: u16, axes: Vec<FontAxis>| {
        let id = entries.len() as u32;
        entries.push(FontEntry {
            id,
            file_path: abs.clone(),
            collection_index,
            post_script_name: post_script.clone(),
            axes: axes.clone(),
        });
        // Default-instance metrics for every instance, mirroring the
        // bundled block: line metrics stay weight-independent, so
        // toggling weight never reflows line heights.
        faces.push(LoadedFace {
            data: Arc::clone(&data),
            upem,
            ascender,
            descender,
            collection_index,
            axes,
        });
        registered.push((weight, id));
    };

    match wght_range(&font) {
        // Variable: the nine standard weights as `wght` instances,
        // deterministic ascending order, clamped into the actual range.
        Some((min, max)) => {
            for weight in SYSTEM_INSTANCE_WEIGHTS {
                push(
                    weight,
                    vec![FontAxis {
                        tag: "wght".to_string(),
                        value: f64::from(weight).clamp(min, max),
                    }],
                );
            }
        }
        // Static: registered as-is at the caller's declared weight.
        None => push(face.weight.clamp(1, 1000), Vec::new()),
    }
    Ok(registered)
}

/// The collection index the declaration selects: with a PostScript
/// name, scan indices until the name matches (bounded); otherwise
/// `ttc_hint` or 0, which must parse.
fn locate_face<'a>(
    data: &'a Arc<memmap2::Mmap>,
    face: &SystemFontFace,
) -> Result<(u32, FontRef<'a>), String> {
    if let Some(wanted) = face.post_script_name.as_deref() {
        for index in 0..MAX_TTC_SCAN {
            let Ok(font) = FontRef::from_index(data, index) else {
                // Past the collection's end (or a bad single face at 0).
                break;
            };
            if post_script_name(&font)
                .ok()
                .flatten()
                .is_some_and(|name| name == wanted)
            {
                return Ok((index, font));
            }
        }
        return Err(format!("no face named {wanted:?} in the file"));
    }
    let index = face.ttc_hint.unwrap_or(0);
    let font = FontRef::from_index(data, index)
        .map_err(|e| format!("face index {index} failed to parse: {e}"))?;
    Ok((index, font))
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
