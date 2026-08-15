//! The OPF package document: metadata, the manifest, and the spine.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::model::EpubMetadata;
use super::xml::{attr_value, clean_text, push_word, resolve_ref};
use crate::CoreError;

/// Upper bound on the `<itemref>` entries kept for one publication. Real
/// books run to a few hundred; a crafted OPF can list millions, each
/// costing an entry read and a DB row. Enforced at the push site so the
/// idrefs beyond it are never materialized; extra ones degrade away with
/// a warning at the caller.
pub(crate) const MAX_SPINE_ITEMS: usize = 10_000;

/// Upper bound on `<item>` manifest entries. The manifest names every
/// asset (images, fonts, styles), so it legitimately runs larger than the
/// spine — hence an order of magnitude more headroom than
/// [`MAX_SPINE_ITEMS`] — but a crafted OPF can pack millions of tiny
/// items whose parsed structs cost ~10x their bytes (measured: a 355 KB
/// file inflating to a 616 MB resident set). The manifest is a mandatory
/// part, and exceeding this bound means the file is not a real book, so
/// the parse fails cleanly with `InvalidPublication`.
pub(crate) const MAX_MANIFEST_ITEMS: usize = 100_000;

/// Upper bound on one manifest item's `href` length. Real hrefs are
/// archive paths a few dozen bytes long; a crafted OPF can attach a
/// multi-MB href to a single item and reference it from every spine
/// slot, paying for the string once in the archive while spine
/// resolution would copy it thousands of times. An item with an absurd
/// href is skipped, degrading exactly like an unresolvable idref.
pub(crate) const MAX_HREF_BYTES: usize = 4096;

/// Upper bound on `<dc:creator>` entries retained. Large anthologies
/// credit a few hundred contributors; a crafted OPF can list millions,
/// each a heap `String` destined for one joined DB column. Extra
/// creators degrade: dropped, with a warning at the caller.
pub(crate) const MAX_AUTHORS: usize = 1_000;

/// Upper bound on one retained metadata value — `<dc:title>`, each
/// `<dc:creator>`, `<dc:language>` — in bytes. Real titles and names run
/// under ~200 bytes even in CJK, so 2 KiB is ~10x headroom. The bound
/// matters more than any other string cap because the title rides the two
/// hottest reads in the app — `list()` on every launch and the per-
/// keystroke search fold — and re-crosses the FFI boundary on each, so an
/// uncapped 60 MiB title would be re-materialized forever after a single
/// import. Enforced at the push site while the OPF is walked, so the
/// oversized value is never accumulated; the tail degrades away (cut on a
/// `char` boundary, never a byte offset) with a warning at the caller.
pub(crate) const MAX_METADATA_VALUE_BYTES: usize = 2048;

#[derive(Debug)]
pub(super) struct ManifestItem {
    pub(super) id: String,
    pub(super) href: String,
    pub(super) media_type: String,
    properties: String,
}

impl ManifestItem {
    pub(super) fn has_property(&self, name: &str) -> bool {
        self.properties.split_ascii_whitespace().any(|p| p == name)
    }
}

#[derive(Debug, Default)]
pub(super) struct Opf {
    pub(super) metadata: EpubMetadata,
    pub(super) items: Vec<ManifestItem>,
    pub(super) spine_idrefs: Vec<String>,
    /// The spine's `toc` attribute (NCX manifest id), EPUB 2 style.
    pub(super) spine_toc: Option<String>,
    /// `<meta name="cover" content="…">`, EPUB 2 style.
    pub(super) cover_meta: Option<String>,
    /// Total `<itemref>`s the spine listed, including any dropped at
    /// [`MAX_SPINE_ITEMS`] — lets the caller log the truncation with the
    /// archive path for context.
    pub(super) spine_itemrefs_seen: usize,
    /// Total `<dc:creator>`s listed, including any dropped at
    /// [`MAX_AUTHORS`].
    pub(super) creators_seen: usize,
    /// Manifest items skipped for an href over [`MAX_HREF_BYTES`].
    pub(super) oversized_href_items: usize,
    /// Metadata values cut at [`MAX_METADATA_VALUE_BYTES`] — lets the
    /// caller log the truncation once with the archive path for context.
    pub(super) truncated_metadata_values: usize,
}

/// Appends `text` to `acc` through [`push_word`], never letting `acc`
/// grow past [`MAX_METADATA_VALUE_BYTES`]. `push_word` appends at most
/// `text.len()` bytes (whitespace collapse only shrinks), so pushing the
/// largest prefix that fits the remaining room keeps the bound exact —
/// and a crafted 60 MiB title costs its decode and nothing more, because
/// the oversized tail is never accumulated. The cut lands on a `char`
/// boundary, never a raw byte offset: titles are routinely CJK, and a
/// byte slice could split a character. Returns whether anything was cut.
fn push_word_capped(acc: &mut String, text: &str) -> bool {
    let room = MAX_METADATA_VALUE_BYTES.saturating_sub(acc.len());
    if text.len() <= room {
        push_word(acc, text);
        return false;
    }
    let mut end = room;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    push_word(acc, &text[..end]);
    true
}

pub(super) fn parse_opf(opf_xml: &str) -> Result<Opf, CoreError> {
    let mut opf = Opf::default();
    let mut reader = Reader::from_str(opf_xml);
    // quick-xml keeps a stack of open element names while validating end
    // tags; a crafted OPF nesting millions of elements would grow it
    // without bound, and mismatched end names are already tolerated by
    // every other parser in this module.
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    // Tracks which dc: element we are inside so text (and entity-reference)
    // nodes accumulate into the right field, committed at the element's
    // end. Only the first title/language wins.
    let mut current: Option<&'static str> = None;
    let mut acc = String::new();
    // Whether the value being accumulated was cut at the cap; committed
    // into the counter with the value, so one oversized value logs once.
    let mut acc_truncated = false;
    loop {
        let event = reader.read_event_into(&mut buf);
        match &event {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let is_empty = matches!(&event, Ok(Event::Empty(_)));
                match e.local_name().as_ref() {
                    b"title" if !is_empty => current = Some("title"),
                    b"creator" if !is_empty => current = Some("creator"),
                    b"language" if !is_empty => current = Some("language"),
                    b"item" => {
                        if opf.items.len() == MAX_MANIFEST_ITEMS {
                            return Err(CoreError::InvalidPublication(format!(
                                "manifest lists more than {MAX_MANIFEST_ITEMS} items"
                            )));
                        }
                        let href = attr_value(e, b"href").unwrap_or_default();
                        if href.len() > MAX_HREF_BYTES {
                            opf.oversized_href_items += 1;
                        } else {
                            opf.items.push(ManifestItem {
                                id: attr_value(e, b"id").unwrap_or_default(),
                                href,
                                media_type: attr_value(e, b"media-type").unwrap_or_default(),
                                properties: attr_value(e, b"properties").unwrap_or_default(),
                            });
                        }
                    }
                    b"itemref" => {
                        if let Some(idref) = attr_value(e, b"idref") {
                            opf.spine_itemrefs_seen += 1;
                            if opf.spine_idrefs.len() < MAX_SPINE_ITEMS {
                                opf.spine_idrefs.push(idref);
                            }
                        }
                    }
                    b"spine" => opf.spine_toc = attr_value(e, b"toc"),
                    b"meta" => {
                        if attr_value(e, b"name").as_deref() == Some("cover") {
                            opf.cover_meta = attr_value(e, b"content");
                        }
                    }
                    _ if !is_empty => current = None,
                    _ => {}
                }
                if current.is_none() {
                    acc.clear();
                    acc_truncated = false;
                }
            }
            Ok(Event::Text(t)) => {
                if current.is_some() {
                    if let Ok(text) = t.decode() {
                        acc_truncated |= push_word_capped(&mut acc, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if current.is_some() {
                    let resolved = resolve_ref(r);
                    // An entity resolves to one short string; past the cap
                    // it drops whole, so no char is ever split.
                    if acc.len() + resolved.len() <= MAX_METADATA_VALUE_BYTES {
                        acc.push_str(&resolved);
                    } else {
                        acc_truncated = true;
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(field) = current.take() {
                    if let Some(text) = clean_text(Some(&acc)) {
                        match field {
                            "title" if opf.metadata.title.is_none() => {
                                opf.metadata.title = Some(text)
                            }
                            "creator" => {
                                opf.creators_seen += 1;
                                if opf.metadata.authors.len() < MAX_AUTHORS {
                                    opf.metadata.authors.push(text);
                                }
                            }
                            "language" if opf.metadata.language.is_none() => {
                                opf.metadata.language = Some(text)
                            }
                            _ => {}
                        }
                    }
                    if acc_truncated {
                        opf.truncated_metadata_values += 1;
                    }
                    acc.clear();
                    acc_truncated = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(opf)
}

#[cfg(test)]
#[path = "opf_tests.rs"]
mod tests;
