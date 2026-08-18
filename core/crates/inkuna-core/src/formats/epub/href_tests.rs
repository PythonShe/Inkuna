use super::*;

#[test]
fn resolves_hrefs_against_base_dirs() {
    assert_eq!(resolve_href("OEBPS", "ch01.xhtml"), "OEBPS/ch01.xhtml");
    assert_eq!(resolve_href("", "ch01.xhtml"), "ch01.xhtml");
    assert_eq!(
        resolve_href("OEBPS/text", "../images/c.png"),
        "OEBPS/images/c.png"
    );
    assert_eq!(
        resolve_href("OEBPS", "./ch01.xhtml#s1"),
        "OEBPS/ch01.xhtml#s1"
    );
    assert_eq!(resolve_href("OEBPS", "ch%201.xhtml"), "OEBPS/ch 1.xhtml");
    assert_eq!(resolve_href("OEBPS", "/root.xhtml"), "root.xhtml");
    // Document-relative resolution: a fragment-only href targets the
    // referencing document itself.
    assert_eq!(
        resolve_relative("OEBPS/nav.xhtml", "#pt1"),
        "OEBPS/nav.xhtml#pt1"
    );
    assert_eq!(
        resolve_relative("OEBPS/nav.xhtml", "text/ch01.xhtml"),
        "OEBPS/text/ch01.xhtml"
    );
}
