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
                layout: RenditionLayout::Reflowable,
            },
            crate::SpineItem {
                href: "OEBPS/text/ch02.xhtml".into(),
                media_type: None,
                layout: RenditionLayout::Reflowable,
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

/// The publication-level layout vote happens after the spine has dropped
/// dangling and duplicate itemrefs, so only resources the engine can lay
/// out participate.
#[test]
fn filtered_spine_items_alone_vote_for_fixed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("filtered-spine-layout.epub");
    crate::test_support::write_epub_parts(
        &path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>選別</dc:title></metadata>
  <manifest>
    <item id="p1" href="p1.xhtml" media-type="application/xhtml+xml"/>
    <item id="p2" href="p2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="p1" properties="rendition:layout-pre-paginated"/>
    <itemref idref="missing"/>
    <itemref idref="p2" properties="rendition:layout-pre-paginated"/>
    <itemref idref="p1"/>
  </spine>
</package>"#,
        &[
            ("p1.xhtml", "<html><body><p>一</p></body></html>"),
            ("p2.xhtml", "<html><body><p>二</p></body></html>"),
        ],
    );

    let package = crate::read_package(&path).unwrap();
    assert_eq!(package.spine.len(), 2);
    assert!(package
        .spine
        .iter()
        .all(|item| item.layout == RenditionLayout::PrePaginated));
    assert_eq!(package.rendition_layout, RenditionLayout::PrePaginated);
}

/// The href cap holds on the *resolved* manifest href too: an item whose
/// as-written href fits under `MAX_HREF_BYTES` but whose resolution
/// against the OPF's directory pushes it over is dropped from both the
/// spine and the manifest — otherwise a crafted container.xml plus a
/// full manifest retains the oversized prefix once per item.
#[test]
fn oversized_resolved_manifest_hrefs_are_dropped() {
    // Under the cap as written; over it once "OEBPS/" is prepended.
    let long_href = format!("{}.xhtml", "a".repeat(MAX_HREF_BYTES - 6));
    assert!(long_href.len() <= MAX_HREF_BYTES);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resolved-bomb.epub");
    crate::test_support::write_epub_parts(
        &path,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>解決</dc:title></metadata>
  <manifest>
    <item id="bomb" href="{long_href}" media-type="application/xhtml+xml"/>
    <item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="bomb"/><itemref idref="c1"/></spine>
</package>"#
        ),
        &[("text/ch01.xhtml", "<html><body><p>一</p></body></html>")],
    );

    let package = crate::read_package(&path).unwrap();
    assert_eq!(
        package.manifest,
        [crate::ManifestItem {
            href: "OEBPS/text/ch01.xhtml".into(),
            media_type: Some("application/xhtml+xml".into()),
        }]
    );
    assert_eq!(package.spine.len(), 1);
    assert_eq!(package.spine[0].href, "OEBPS/text/ch01.xhtml");
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
    assert_eq!(opf.package_layout, Some(RenditionLayout::PrePaginated));
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
    assert_eq!(opf.package_layout, Some(RenditionLayout::Reflowable));
}

#[test]
fn empty_rendition_value_is_an_explicit_reflowable_default() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout"/></metadata>
  <manifest/><spine/>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.package_layout, Some(RenditionLayout::Reflowable));
}

/// `rendition:layout` is publication-level only when un-refined: a
/// `<meta refines="…">` overrides one itemref, so it must not set the
/// whole book's layout — even when it appears first — and the
/// publication-level meta after it still wins.
#[test]
fn refined_rendition_meta_does_not_set_book_layout() {
    let refined_only = parse_opf(
        r##"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta refines="#spread1" property="rendition:layout">pre-paginated</meta></metadata>
  <manifest/><spine/>
</package>"##,
    )
    .unwrap();
    assert_eq!(refined_only.package_layout, None);

    let refined_then_publication = parse_opf(
        r##"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata>
    <meta refines="#spread1" property="rendition:layout">reflowable</meta>
    <meta property="rendition:layout">pre-paginated</meta>
  </metadata>
  <manifest/><spine/>
</package>"##,
    )
    .unwrap();
    assert_eq!(
        refined_then_publication.package_layout,
        Some(RenditionLayout::PrePaginated)
    );
}

/// Itemref properties are not package declarations: without an unrefined
/// rendition meta, the package default remains absent and each retained
/// spine item resolves its own layout later.
#[test]
fn itemref_properties_do_not_create_a_package_layout_default() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">絵本</dc:title></metadata>
  <manifest>
    <item id="p1" href="p1.xhtml" media-type="application/xhtml+xml"/>
    <item id="p2" href="p2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="p1" properties="rendition:layout-pre-paginated"/>
    <itemref idref="p2" properties="page-spread-left rendition:layout-pre-paginated"/>
  </spine>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.package_layout, None);
}

/// A fixed itemref remains an item-level declaration when the package has
/// no rendition meta; the retained spine resolves the publication vote.
#[test]
fn one_fixed_itemref_does_not_create_a_package_layout_default() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="map" href="map.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="map" properties="rendition:layout-pre-paginated"/>
    <itemref idref="c2"/>
  </spine>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.package_layout, None);
}

/// A package-level declaration is publication-level: itemref properties
/// still resolve per resource, but cannot change the reader chosen at open.
#[test]
fn itemrefs_do_not_override_a_prepaginated_package_layout() {
    let all_reflowable = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout">pre-paginated</meta></metadata>
  <manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1" properties="rendition:layout-reflowable"/>
    <itemref idref="c2" properties="rendition:layout-reflowable"/>
  </spine>
</package>"#,
    )
    .unwrap();
    assert_eq!(
        all_reflowable.package_layout,
        Some(RenditionLayout::PrePaginated)
    );

    let some_inherit = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout">pre-paginated</meta></metadata>
  <manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1" properties="rendition:layout-reflowable"/>
    <itemref idref="c2"/>
  </spine>
</package>"#,
    )
    .unwrap();
    assert_eq!(
        some_inherit.package_layout,
        Some(RenditionLayout::PrePaginated)
    );
}

/// A package-level `pre-paginated` with itemrefs that declare nothing
/// still reads as fixed-layout — the pre-existing behavior the per-item
/// resolution must not regress.
#[test]
fn package_prepaginated_survives_plain_itemrefs() {
    let opf = parse_opf(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><meta property="rendition:layout">pre-paginated</meta></metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#,
    )
    .unwrap();
    assert_eq!(opf.package_layout, Some(RenditionLayout::PrePaginated));
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
