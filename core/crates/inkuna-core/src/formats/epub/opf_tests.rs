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
