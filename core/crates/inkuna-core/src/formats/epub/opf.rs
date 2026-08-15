//! The OPF package document: metadata, the manifest, and the spine.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::model::EpubMetadata;
use super::xml::{attr_value, clean_text, push_word, resolve_ref};

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
}

pub(super) fn parse_opf(opf_xml: &str) -> Opf {
    let mut opf = Opf::default();
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    // Tracks which dc: element we are inside so text (and entity-reference)
    // nodes accumulate into the right field, committed at the element's
    // end. Only the first title/language wins.
    let mut current: Option<&'static str> = None;
    let mut acc = String::new();
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
                        opf.items.push(ManifestItem {
                            id: attr_value(e, b"id").unwrap_or_default(),
                            href: attr_value(e, b"href").unwrap_or_default(),
                            media_type: attr_value(e, b"media-type").unwrap_or_default(),
                            properties: attr_value(e, b"properties").unwrap_or_default(),
                        });
                    }
                    b"itemref" => {
                        if let Some(idref) = attr_value(e, b"idref") {
                            opf.spine_idrefs.push(idref);
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
                }
            }
            Ok(Event::Text(t)) => {
                if current.is_some() {
                    if let Ok(text) = t.decode() {
                        push_word(&mut acc, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if current.is_some() {
                    acc.push_str(&resolve_ref(r));
                }
            }
            Ok(Event::End(_)) => {
                if let Some(field) = current.take() {
                    if let Some(text) = clean_text(Some(&acc)) {
                        match field {
                            "title" if opf.metadata.title.is_none() => {
                                opf.metadata.title = Some(text)
                            }
                            "creator" => opf.metadata.authors.push(text),
                            "language" if opf.metadata.language.is_none() => {
                                opf.metadata.language = Some(text)
                            }
                            _ => {}
                        }
                    }
                    acc.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    opf
}
