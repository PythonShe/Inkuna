use super::*;

/// Spine idrefs and creators are bounded *while* the OPF is walked — the
/// caps hold at the push site, so the full lists are never materialized —
/// and the seen-counts still report what the file listed so the caller
/// can log the truncation.
#[test]
fn spine_idrefs_and_creators_are_bounded_at_the_push_site() {
    let itemrefs = r#"<itemref idref="c1"/>"#.repeat(MAX_SPINE_ITEMS + 3);
    let creators = "<dc:creator>著者</dc:creator>".repeat(MAX_AUTHORS + 2);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>限界試験</dc:title>{creators}
  </metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine>{itemrefs}</spine>
</package>"#
    );
    let opf = parse_opf(&xml).unwrap();
    assert_eq!(opf.spine_idrefs.len(), MAX_SPINE_ITEMS);
    assert_eq!(opf.spine_itemrefs_seen, MAX_SPINE_ITEMS + 3);
    assert_eq!(opf.metadata.authors.len(), MAX_AUTHORS);
    assert_eq!(opf.creators_seen, MAX_AUTHORS + 2);
    assert_eq!(opf.metadata.title.as_deref(), Some("限界試験"));
}

/// An item whose href is absurdly long is dropped at the push site: one
/// such href referenced from every spine slot would be copied per
/// reference during resolution, so it must never enter the manifest.
#[test]
fn manifest_item_with_oversized_href_is_skipped() {
    let long_href = "a".repeat(MAX_HREF_BYTES + 1);
    let xml = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="bomb" href="{long_href}" media-type="application/xhtml+xml"/>
    <item id="c1" href="第一章.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="bomb"/><itemref idref="c1"/></spine>
</package>"#
    );
    let opf = parse_opf(&xml).unwrap();
    assert_eq!(opf.items.len(), 1);
    assert_eq!(opf.items[0].id, "c1");
    assert_eq!(opf.oversized_href_items, 1);
}

/// Spine items carry the media type the manifest declares for the item
/// backing each itemref — joined at `read_package`, `None` when the item
/// declares none — and the full manifest rides along, hrefs normalized.
#[test]
fn spine_items_carry_media_types() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("media-types.epub");
    crate::test_support::write_epub_parts(
        &path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>型</dc:title></metadata>
  <manifest>
    <item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch02.xhtml"/>
    <item id="css" href="style.css" media-type="text/css"/>
  </manifest>
  <spine><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"#,
        &[
            ("text/ch01.xhtml", "<html><body><p>一</p></body></html>"),
            ("text/ch02.xhtml", "<html><body><p>二</p></body></html>"),
        ],
    );

    let package = crate::read_package(&path).unwrap();
    assert_eq!(
        package.spine,
        [
            crate::SpineItem {
                href: "OEBPS/text/ch01.xhtml".into(),
                media_type: Some("application/xhtml+xml".into()),
            },
            crate::SpineItem {
                href: "OEBPS/text/ch02.xhtml".into(),
                media_type: None,
            },
        ]
    );
    assert_eq!(
        package.manifest,
        [
            crate::ManifestItem {
                href: "OEBPS/text/ch01.xhtml".into(),
                media_type: Some("application/xhtml+xml".into()),
            },
            crate::ManifestItem {
                href: "OEBPS/text/ch02.xhtml".into(),
                media_type: None,
            },
            crate::ManifestItem {
                href: "OEBPS/style.css".into(),
                media_type: Some("text/css".into()),
            },
        ]
    );
    assert_eq!(package.rendition_layout, RenditionLayout::Reflowable);
    assert!(!package.page_progression_rtl);
}

#[test]
fn rendition_prepaginated_detected() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout">pre-paginated</meta></metadata>
  <manifest/><spine/>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.rendition_layout, RenditionLayout::PrePaginated);
}

/// A malformed or unknown `rendition:layout` value is never an error —
/// anything other than `pre-paginated` takes the reflowable default.
#[test]
fn unknown_rendition_value_is_reflowable() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout">scrolled-continuous</meta></metadata>
  <manifest/><spine/>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.rendition_layout, RenditionLayout::Reflowable);
}

#[test]
fn page_progression_rtl_detected() {
    let rtl = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest/><spine page-progression-direction="rtl"/>
</package>"#,
    )
    .unwrap();
    assert!(rtl.page_progression_rtl);

    let absent = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest/><spine/>
</package>"#,
    )
    .unwrap();
    assert!(!absent.page_progression_rtl);

    // `ltr` (or anything else) is not rtl.
    let ltr = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest/><spine page-progression-direction="ltr"/>
</package>"#,
    )
    .unwrap();
    assert!(!ltr.page_progression_rtl);
}
