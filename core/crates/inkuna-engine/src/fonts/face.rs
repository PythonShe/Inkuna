//! Shared face-parsing helpers: memory mapping, identity, and metric
//! extraction over `read-fonts`, used by both the bundled manifest
//! loader and the dynamically registered system faces.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;
use read_fonts::tables::os2::SelectionFlags;
use read_fonts::types::{NameId, Tag};
use read_fonts::{FontRef, TableProvider};

use crate::error::EngineError;

pub(super) const WGHT: Tag = Tag::new(b"wght");

pub(super) fn map_font(path: &Path, name: &str) -> Result<Mmap, EngineError> {
    let file = File::open(path).map_err(|e| missing(name, &e))?;
    // SAFETY: iOS maps read-only app-bundle resources and Android maps read-only
    // extracted assets in noBackupFilesDir, so the mapped font file cannot
    // mutate. System font files are the platform's own read-only font
    // directories, immutable for the process lifetime by the same argument.
    unsafe { Mmap::map(&file) }.map_err(|e| missing(name, &e))
}

pub(super) fn post_script_name(
    font: &FontRef<'_>,
) -> Result<Option<String>, read_fonts::ReadError> {
    let names = font.name()?;
    let data = names.string_data();
    let Some(record) = names
        .name_record()
        .into_iter()
        .filter(|record| record.name_id() == NameId::POSTSCRIPT_NAME)
        .next()
    else {
        return Ok(None);
    };
    let name = record.string(data)?.to_string();
    Ok((!name.is_empty()).then_some(name))
}

/// Matches ttf-parser's default-instance horizontal metric selection:
/// `USE_TYPO_METRICS`, then hhea, then OS/2 typo metrics, then Windows
/// metrics. Weight-instance ids reuse their base face's default-instance
/// metrics deliberately (no MVAR application) — line metrics stay
/// weight-independent.
pub(super) fn font_metrics(
    font: &FontRef<'_>,
) -> Result<(u16, i32, i32), read_fonts::ReadError> {
    let upem = font.head()?.units_per_em();
    let hhea = font.hhea()?;
    let mut ascender = i32::from(hhea.ascender().to_i16());
    let mut descender = i32::from(hhea.descender().to_i16());
    if let Ok(os2) = font.os2() {
        if os2
            .fs_selection()
            .contains(SelectionFlags::USE_TYPO_METRICS)
        {
            return Ok((
                upem,
                i32::from(os2.s_typo_ascender()),
                i32::from(os2.s_typo_descender()).saturating_neg(),
            ));
        }
        if ascender == 0 {
            ascender = i32::from(os2.s_typo_ascender());
            if ascender == 0 {
                ascender = i32::from(os2.us_win_ascent());
            }
        }
        if descender == 0 {
            descender = i32::from(os2.s_typo_descender());
            if descender == 0 {
                descender = -i32::from(os2.us_win_descent());
            }
        }
    }
    Ok((upem, ascender, descender.saturating_neg()))
}

/// The `wght` axis user-space range from a variable face's fvar, or
/// `None` when the face is static or has no weight axis.
pub(super) fn wght_range(font: &FontRef<'_>) -> Option<(f64, f64)> {
    let fvar = font.fvar().ok()?;
    let axes = fvar.axes().ok()?;
    let axis = axes.iter().find(|axis| axis.axis_tag() == WGHT)?;
    Some((
        axis.min_value().to_f64(),
        axis.max_value().to_f64(),
    ))
}

pub(super) fn missing(file: &str, cause: &dyn std::fmt::Display) -> EngineError {
    EngineError::UnsupportedContent {
        detail: format!("font missing: {file} ({cause})"),
    }
}
