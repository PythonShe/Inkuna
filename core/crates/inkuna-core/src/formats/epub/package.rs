//! One pass over the archive that yields everything import needs.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;

use super::archive::{read_entry, read_entry_bytes};
use super::container::rootfile_path;
use super::cover::image_extension;
use super::href::{parent_dir, resolve_href};
use super::model::{Cover, EpubPackage};
use super::opf::{
    parse_opf, ManifestItem, MAX_AUTHORS, MAX_HREF_BYTES, MAX_METADATA_VALUE_BYTES, MAX_SPINE_ITEMS,
};
use super::toc::{parse_nav, parse_ncx};
use crate::CoreError;

/// Parses everything import needs in one pass over the archive: metadata,
/// spine, TOC, and cover bytes. Text extraction is separate
/// (`extract_spine_text`) so it can run in parallel.
pub fn read_package(path: &Path) -> Result<EpubPackage, CoreError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;

    let container = read_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = rootfile_path(&container)?;
    let opf_xml = read_entry(&mut archive, &opf_path)?;
    let opf_dir = parent_dir(&opf_path);
    let opf = parse_opf(&opf_xml)?;
    if let Some(error) = &opf.parse_error {
        log::warn!(
            "OPF {opf_path} of {} is not well-formed ({error}); everything after it was skipped — metadata and spine may be incomplete",
            path.display()
        );
    }
    if opf.oversized_href_items > 0 {
        log::warn!(
            "manifest of {} has {} items with hrefs over {MAX_HREF_BYTES} bytes; skipped",
            path.display(),
            opf.oversized_href_items
        );
    }
    if opf.creators_seen > MAX_AUTHORS {
        log::warn!(
            "metadata of {} lists {} creators; keeping the first {MAX_AUTHORS}",
            path.display(),
            opf.creators_seen
        );
    }
    if opf.truncated_metadata_values > 0 {
        log::warn!(
            "metadata of {} has {} values over {MAX_METADATA_VALUE_BYTES} bytes; truncated",
            path.display(),
            opf.truncated_metadata_values
        );
    }

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

    // The cap itself holds inside `parse_opf`, at the push site — a
    // truncated Vec never exists here — this only reports what was cut.
    if opf.spine_itemrefs_seen > MAX_SPINE_ITEMS {
        log::warn!(
            "spine of {} lists {} itemrefs; processing the first {MAX_SPINE_ITEMS}",
            path.display(),
            opf.spine_itemrefs_seen
        );
    }
    // MAX_HREF_BYTES is enforced again on the *resolved* href: the
    // manifest cap sees the href as written, but resolution prepends the
    // OPF's directory, which a crafted container.xml can push toward
    // zip's 65,535-byte name ceiling while every per-item cap stays in
    // bounds — and each spine row retains its own copy, so uncapped this
    // persists spine × ~64 KiB of `resources` rows out of a
    // few-hundred-KB archive. An oversized result degrades exactly like
    // an unresolvable idref.
    let mut oversized_spine_hrefs = 0usize;
    // Deduplicated on the *resolved* href: EPUB 3 requires each itemref
    // to reference a unique resource, so an honest spine never repeats —
    // while a crafted spine repeating one resource thousands of times
    // would otherwise persist its own ~4 KB `resources` row (and its own
    // copy of the extracted text) per repeat. First occurrence wins,
    // preserving reading order.
    let mut duplicate_spine_hrefs = 0usize;
    let mut seen_hrefs: HashSet<String> = HashSet::new();
    let spine: Vec<String> = opf
        .spine_idrefs
        .iter()
        .filter_map(|idref| item_by_id(idref))
        .filter_map(|item| {
            let resolved = resolve(&item.href);
            if resolved.len() > MAX_HREF_BYTES {
                oversized_spine_hrefs += 1;
                return None;
            }
            // Cloned into the seen-set: the retained spine owns the
            // original, and both live only for this parse.
            if !seen_hrefs.insert(resolved.clone()) {
                duplicate_spine_hrefs += 1;
                return None;
            }
            Some(resolved)
        })
        .collect();
    if oversized_spine_hrefs > 0 {
        log::warn!(
            "spine of {} has {oversized_spine_hrefs} itemrefs whose resolved hrefs exceed {MAX_HREF_BYTES} bytes; skipped",
            path.display()
        );
    }
    if duplicate_spine_hrefs > 0 {
        log::warn!(
            "spine of {} repeats already-listed resources {duplicate_spine_hrefs} times; keeping the first occurrence of each",
            path.display()
        );
    }

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
        let ncx_item = opf.spine_toc.as_deref().and_then(item_by_id).or_else(|| {
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
        // Checked before the read: an item that is not an image, or whose
        // href yields no usable extension, degrades to no cover the same
        // way unreadable cover bytes already do.
        let Some(extension) = image_extension(&item.media_type, &item.href) else {
            log::warn!(
                "skipping cover of {}: {} ({}) is not a usable image",
                path.display(),
                item.href,
                item.media_type
            );
            return None;
        };
        let bytes = read_entry_bytes(&mut archive, &resolve(&item.href)).ok()?;
        Some(Cover { bytes, extension })
    });

    Ok(EpubPackage {
        metadata: opf.metadata,
        spine,
        toc,
        cover,
    })
}
