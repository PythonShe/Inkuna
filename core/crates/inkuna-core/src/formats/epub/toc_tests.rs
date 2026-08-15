use super::*;

#[test]
fn nav_parsing_flattens_with_depth() {
    let xml = r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="lot"><ol><li><a href="x.xhtml">not the toc</a></li></ol></nav>
<nav epub:type="toc"><ol>
  <li><a href="ch01.xhtml">第一章</a>
    <ol><li><a href="ch01.xhtml#s1">第一節</a></li></ol>
  </li>
  <li><a href="ch02.xhtml">第二章</a></li>
  <li><a href="#landmarks">付録</a></li>
</ol></nav>
</body></html>"##;
    let toc = parse_nav(xml, "OEBPS/nav.xhtml");
    assert_eq!(
        toc,
        vec![
            TocEntry { title: "第一章".into(), href: "OEBPS/ch01.xhtml".into(), depth: 0 },
            TocEntry { title: "第一節".into(), href: "OEBPS/ch01.xhtml#s1".into(), depth: 1 },
            TocEntry { title: "第二章".into(), href: "OEBPS/ch02.xhtml".into(), depth: 0 },
            // Fragment-only entries anchor inside the nav doc itself.
            TocEntry { title: "付録".into(), href: "OEBPS/nav.xhtml#landmarks".into(), depth: 0 },
        ]
    );
}

/// A crafted NCX listing far more navPoints than any real TOC stops at
/// the entry cap instead of materializing one `TocEntry` (and later one
/// DB row) per navPoint.
#[test]
fn ncx_truncates_at_the_entry_cap() {
    let mut xml = String::from(r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>"#);
    for _ in 0..MAX_TOC_ENTRIES + 5 {
        xml.push_str(
            r#"<navPoint><navLabel><text>章</text></navLabel><content src="ch.xhtml"/></navPoint>"#,
        );
    }
    xml.push_str("</navMap></ncx>");
    let toc = parse_ncx(&xml, "OEBPS/toc.ncx");
    assert_eq!(toc.len(), MAX_TOC_ENTRIES);
}

/// navPoints nested past the depth cap are skipped (bounding the one
/// label `String` the parser holds per open navPoint), and self-closed
/// `<navPoint/>`s open nothing — before the fix each one leaked a level
/// of depth because no matching End ever popped it.
#[test]
fn ncx_depth_is_capped_and_empty_navpoints_do_not_leak_it() {
    let mut xml = String::from(r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>"#);
    for i in 0..MAX_TOC_DEPTH + 5 {
        xml.push_str(&format!(
            r##"<navPoint><navLabel><text>第{i}層</text></navLabel><content src="ch.xhtml#f{i}"/>"##
        ));
    }
    for _ in 0..MAX_TOC_DEPTH + 5 {
        xml.push_str("</navPoint>");
    }
    xml.push_str("</navMap></ncx>");
    let toc = parse_ncx(&xml, "OEBPS/toc.ncx");
    // One entry per level up to the cap; the five deeper ones are gone.
    assert_eq!(toc.len(), MAX_TOC_DEPTH);
    assert_eq!(toc.last().unwrap().depth as usize, MAX_TOC_DEPTH - 1);

    // A flood of self-closed navPoints must not inflate the depth of the
    // real entry that follows them.
    let xml = r#"<ncx><navMap><navPoint/><navPoint/><navPoint/>
<navPoint><navLabel><text>第一章</text></navLabel><content src="ch01.xhtml"/></navPoint>
</navMap></ncx>"#;
    let toc = parse_ncx(xml, "OEBPS/toc.ncx");
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].depth, 0);
}

#[test]
fn ncx_parsing_flattens_with_depth() {
    let xml = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
<navPoint id="a"><navLabel><text>第一章</text></navLabel><content src="ch01.xhtml"/>
  <navPoint id="b"><navLabel><text>第一節</text></navLabel><content src="ch01.xhtml#s1"/></navPoint>
</navPoint>
<navPoint id="c"><navLabel><text>第二章</text></navLabel><content src="ch02.xhtml"/></navPoint>
</navMap></ncx>"#;
    let toc = parse_ncx(xml, "OEBPS/toc.ncx");
    assert_eq!(
        toc,
        vec![
            TocEntry { title: "第一章".into(), href: "OEBPS/ch01.xhtml".into(), depth: 0 },
            TocEntry { title: "第一節".into(), href: "OEBPS/ch01.xhtml#s1".into(), depth: 1 },
            TocEntry { title: "第二章".into(), href: "OEBPS/ch02.xhtml".into(), depth: 0 },
        ]
    );
}
