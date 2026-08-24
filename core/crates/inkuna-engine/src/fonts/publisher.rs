//! Publisher-embedded faces: the per-session dynamic block layered
//! above the system block, and the family table `font-family` stacks
//! resolve against when the reading font is `publisher`.
//!
//! Registration NEVER mutates the process-global registry: the session
//! derives its own extended registry ([`FontRegistry::with_publisher`])
//! whose publisher block starts at the base registry's `next_free_id()`.
//! Ids are deterministic — same base registry + same spec list (itself a
//! deterministic function of the publication) → same ids — and two
//! sessions cannot collide because each holds its own derived table
//! (the FFI is additionally last-open-wins, one live session per
//! `Bookshelf`).
//!
//! Degradation contract mirrors the system block: a face that fails to
//! load is skipped with a warning log, never an error, never a crash.

use std::path::PathBuf;
use std::sync::Arc;

use super::face::{font_metrics, map_font, post_script_name, wght_range};
use super::registry::{FontAxis, FontEntry, LoadedFace};
use super::system::nearest_weight;
use crate::style::FontStyle;

/// One extracted, validated face file ready to register: produced by
/// the session's extraction pass (deobfuscated, decompressed, written
/// under the per-book cache dir).
#[derive(Debug, Clone, PartialEq)]
pub struct PublisherFaceSpec {
    /// Absolute path of the extracted font file.
    pub file_path: PathBuf,
    /// The family name requests match against: the `@font-face` declared
    /// family, or the font's own name-table family for manifest-only
    /// faces.
    pub family: String,
    pub italic: bool,
    /// Inclusive weight range the face serves (a single weight is
    /// `(w, w)`).
    pub weight: (u16, u16),
}

/// The nine standard CSS weights a variable publisher face is instanced
/// at (those inside its declared range), mirroring the system block.
const INSTANCE_WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// One selectable slot: the weight range it serves and its face id.
#[derive(Debug, Clone, Copy)]
struct Slot {
    min: u16,
    max: u16,
    id: u32,
}

/// One declared family's faces, split by slant.
#[derive(Debug, Default)]
struct Family {
    /// The family name, Unicode-lowercased for case-insensitive match.
    folded: String,
    upright: Vec<Slot>,
    italic: Vec<Slot>,
}

/// The registered publisher families of one derived registry. Empty on
/// the base registry, so `Publisher` requests without a stack fall back
/// to NotoSerif exactly as before B2.
#[derive(Debug, Default)]
pub(super) struct PublisherFamilies {
    families: Vec<Family>,
}

impl PublisherFamilies {
    /// The face id for a named family + style + weight, or `None` when
    /// no such family was registered (the caller walks on down the
    /// stack). Style prefers its own slant and synthesizes from the
    /// other; weight follows the css-fonts-4 rule over slot ranges.
    pub(super) fn select(&self, name: &str, style: FontStyle, weight: u16) -> Option<u32> {
        let folded = name.to_lowercase();
        let family = self.families.iter().find(|f| f.folded == folded)?;
        let (preferred, other) = match style {
            FontStyle::Italic => (&family.italic, &family.upright),
            FontStyle::Normal => (&family.upright, &family.italic),
        };
        let slots = if preferred.is_empty() { other } else { preferred };
        select_slot(slots, weight)
    }

    fn push(&mut self, family: &str, italic: bool, slot: Slot) {
        let folded = family.to_lowercase();
        let entry = match self.families.iter_mut().find(|f| f.folded == folded) {
            Some(entry) => entry,
            None => {
                self.families.push(Family {
                    folded,
                    ..Family::default()
                });
                // Just pushed above.
                match self.families.last_mut() {
                    Some(entry) => entry,
                    None => return,
                }
            }
        };
        if italic {
            entry.italic.push(slot);
        } else {
            entry.upright.push(slot);
        }
    }
}

/// Range-aware CSS weight matching: a slot whose range contains the
/// desired weight wins (first registered on overlap, deterministic);
/// otherwise each slot is represented by its endpoint nearest the
/// desired weight and the css-fonts-4 §5.2 rule picks among those.
fn select_slot(slots: &[Slot], desired: u16) -> Option<u32> {
    if let Some(hit) = slots
        .iter()
        .find(|slot| slot.min <= desired && desired <= slot.max)
    {
        return Some(hit.id);
    }
    let mut candidates: Vec<(u16, u32)> = slots
        .iter()
        .map(|slot| {
            let representative = if desired < slot.min { slot.min } else { slot.max };
            (representative, slot.id)
        })
        .collect();
    candidates.sort_by_key(|(weight, _)| *weight);
    nearest_weight(&candidates, desired)
}

/// Appends every loadable publisher face to `entries`/`faces` (ids in
/// spec order from the current length) and returns the family table.
/// Infallible by design — a face that fails to load is skipped with a
/// warning log and takes no id... which stays deterministic because
/// loadability is a property of the extracted file, not of timing.
pub(super) fn load_publisher_faces(
    specs: &[PublisherFaceSpec],
    entries: &mut Vec<FontEntry>,
    faces: &mut Vec<LoadedFace>,
) -> PublisherFamilies {
    let mut table = PublisherFamilies::default();
    // Maps cached per file: several specs may share one extracted file.
    let mut cache: Vec<(PathBuf, Arc<memmap2::Mmap>)> = Vec::new();
    for spec in specs {
        if let Err(detail) = load_one(spec, &mut cache, entries, faces, &mut table) {
            log::warn!(
                "publisher font skipped: {} ({detail})",
                spec.file_path.display()
            );
        }
    }
    table
}

/// Loads one spec: maps the extracted file, validates it, and registers
/// either the static face at its declared range or standard-weight
/// `wght` instances across it.
fn load_one(
    spec: &PublisherFaceSpec,
    cache: &mut Vec<(PathBuf, Arc<memmap2::Mmap>)>,
    entries: &mut Vec<FontEntry>,
    faces: &mut Vec<LoadedFace>,
    table: &mut PublisherFamilies,
) -> Result<(), String> {
    let name = spec.file_path.display().to_string();
    let data = match cache.iter().find(|(path, _)| *path == spec.file_path) {
        Some((_, data)) => Arc::clone(data),
        None => {
            let data =
                Arc::new(map_font(&spec.file_path, &name).map_err(|e| e.to_string())?);
            cache.push((spec.file_path.clone(), Arc::clone(&data)));
            data
        }
    };
    // `from_index(_, 0)` serves both single faces and (rare) publisher
    // collections — the first face is the one registered.
    let font = read_fonts::FontRef::from_index(&data, 0).map_err(|e| e.to_string())?;
    let (upem, ascender, descender) = font_metrics(&font).map_err(|e| e.to_string())?;
    if upem == 0 {
        return Err("units_per_em is 0".to_string());
    }
    // Identity for the shells: the parsed PostScript name, or the
    // declared family when the font's name table lacks one (publisher
    // files are wilder than platform files; the shells rebuild from
    // file_path either way).
    let post_script = post_script_name(&font)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| spec.family.clone());
    let abs = std::path::absolute(&spec.file_path)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();

    let mut push = |axes: Vec<FontAxis>, slot_range: (u16, u16)| {
        let id = entries.len() as u32;
        entries.push(FontEntry {
            id,
            file_path: abs.clone(),
            collection_index: 0,
            post_script_name: post_script.clone(),
            axes: axes.clone(),
        });
        // Default-instance metrics for every instance, mirroring the
        // bundled and system blocks: line metrics stay weight-independent.
        faces.push(LoadedFace {
            data: Arc::clone(&data),
            upem,
            ascender,
            descender,
            collection_index: 0,
            axes,
        });
        table.push(
            &spec.family,
            spec.italic,
            Slot {
                min: slot_range.0,
                max: slot_range.1,
                id,
            },
        );
    };

    let (declared_min, declared_max) = spec.weight;
    match wght_range(&font) {
        // Variable: standard-weight instances inside the declared range,
        // slot edges stretched so the whole declared range stays served.
        Some((axis_min, axis_max)) => {
            let stops: Vec<u16> = INSTANCE_WEIGHTS
                .iter()
                .copied()
                .filter(|w| (declared_min..=declared_max).contains(w))
                .collect();
            let stops = if stops.is_empty() {
                vec![declared_min]
            } else {
                stops
            };
            let last = stops.len() - 1;
            for (at, weight) in stops.iter().copied().enumerate() {
                let axes = vec![FontAxis {
                    tag: "wght".to_string(),
                    value: f64::from(weight).clamp(axis_min, axis_max),
                }];
                let slot = (
                    if at == 0 { declared_min } else { weight },
                    if at == last { declared_max } else { weight },
                );
                push(axes, slot);
            }
        }
        // Static: one id serving the whole declared range.
        None => push(Vec::new(), (declared_min, declared_max)),
    }
    Ok(())
}

#[cfg(test)]
#[path = "publisher_tests.rs"]
mod tests;
