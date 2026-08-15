//! One pass over the archive that yields everything import needs.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use super::archive::{read_entry, read_entry_bytes};
use super::container::rootfile_path;
use super::cover::image_extension;
use super::href::{parent_dir, resolve_href};
use super::model::{Cover, EpubPackage};
use super::opf::{parse_opf, ManifestItem};
use super::toc::{parse_ncx, parse_nav};
use crate::CoreError;

/// Upper bound on the `<itemref>` entries processed for one publication.
/// Real books run to a few hundred; a crafted OPF can list millions, each
/// costing an entry read and a DB row, so the spine is truncated here
/// before any of that work is scheduled.
pub(super) const MAX_SPINE_ITEMS: usize = 10_000;

/// Parses everything import needs in one pass over the archive: metadata,
/// spine, TOC, and cover bytes. Text extraction is separate
/// (`extract_spine_text`) so it can run in parallel.
pub fn read_package(path: &Path) -> Result<EpubPackage, CoreError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;

    let container = read_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = rootfile_path(&container)?;
    let opf_xml = read_entry(&mut archive, &opf_path)?;
    let opf_dir = parent_dir(&opf_path);
    let opf = parse_opf(&opf_xml);

    let resolve = |href: &str| resolve_href(opf_dir, href);
    // A map, not a per-idref linear scan: a crafted OPF can carry millions
    // of manifest items alongside millions of itemrefs, and the scan made
    // the pairing quadratic — a CPU hang out of a ~100 KB file.
    let mut items_by_id: HashMap<&str, &ManifestItem> = HashMap::with_capacity(opf.items.len());
    for item in &opf.items {
        // First occurrence wins, matching the `find` this replaces.
        items_by_id.entry(item.id.as_str()).or_insert(item);
    }
    let item_by_id = |id: &str| items_by_id.get(id).copied();

    if opf.spine_idrefs.len() > MAX_SPINE_ITEMS {
        log::warn!(
            "spine of {} lists {} itemrefs; processing the first {MAX_SPINE_ITEMS}",
            path.display(),
            opf.spine_idrefs.len()
        );
    }
    let spine: Vec<String> = opf
        .spine_idrefs
        .iter()
        .take(MAX_SPINE_ITEMS)
        .filter_map(|idref| item_by_id(idref))
        .map(|item| resolve(&item.href))
        .collect();

    // EPUB 3 nav doc → NCX fallback. A nav doc that yields no entries
    // (present but empty or unparseable) also falls back.
    let mut toc = Vec::new();
    if let Some(nav_item) = opf.items.iter().find(|i| i.has_property("nav")) {
        let nav_path = resolve(&nav_item.href);
        if let Ok(xml) = read_entry(&mut archive, &nav_path) {
            toc = parse_nav(&xml, &nav_path);
        }
    }
    if toc.is_empty() {
        let ncx_item = opf
            .spine_toc
            .as_deref()
            .and_then(item_by_id)
            .or_else(|| {
                opf.items
                    .iter()
                    .find(|i| i.media_type == "application/x-dtbncx+xml")
            });
        if let Some(item) = ncx_item {
            let ncx_path = resolve(&item.href);
            if let Ok(xml) = read_entry(&mut archive, &ncx_path) {
                toc = parse_ncx(&xml, &ncx_path);
            }
        }
    }

    // Cover: EPUB 3 `cover-image` property → EPUB 2 `<meta name="cover">`
    // (whose content is the manifest id — or, in broken files, the href).
    let cover_item = opf
        .items
        .iter()
        .find(|i| i.has_property("cover-image"))
        .or_else(|| {
            opf.cover_meta.as_deref().and_then(|content| {
                item_by_id(content).or_else(|| opf.items.iter().find(|i| i.href == content))
            })
        });
    let cover = cover_item.and_then(|item| {
        let bytes = read_entry_bytes(&mut archive, &resolve(&item.href)).ok()?;
        Some(Cover {
            bytes,
            extension: image_extension(&item.media_type, &item.href),
        })
    });

    Ok(EpubPackage {
        metadata: opf.metadata,
        spine,
        toc,
        cover,
    })
}
