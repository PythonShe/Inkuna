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
