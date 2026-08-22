use super::*;

#[test]
fn extracts_normalized_text_with_cjk() {
    let xml = r#"<html><head><title>skip me</title><style>p{}</style></head>
<body><h1>第一章　月光</h1>
<p>静かな　夜だった。</p><p>Line <em>two</em> here.</p>
<script>ignore();</script>
<p>&amp; escaped &lt;text&gt;</p></body></html>"#;
    let text = extract_text(xml).unwrap();
    // U+3000 ideographic spaces normalize to ASCII spaces with the rest.
    assert_eq!(
        text,
        "第一章 月光\n静かな 夜だった。\nLine two here.\n& escaped <text>"
    );
}

/// The aggregate corpus budget holds through the real extraction path: a
/// CJK resource whose text alone exceeds the (injected) budget comes back
/// `None` — dropped, import still succeeds upstream — even though it is
/// far under the per-entry decompression cap, while a resource within
/// the budget still extracts. Deterministic because each call names a
/// single resource, so rayon scheduling cannot reorder budget spending.
#[test]
fn aggregate_text_budget_drops_resources_beyond_it() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("OEBPS/small.xhtml", stored).unwrap();
    zip.write_all("<html><body><p>短い本文。</p></body></html>".as_bytes())
        .unwrap();
    zip.start_file("OEBPS/big.xhtml", stored).unwrap();
    let big_body = "満月の夜、風が語る。".repeat(50); // ~1.5 KB of CJK text
    zip.write_all(format!("<html><body><p>{big_body}</p></body></html>").as_bytes())
        .unwrap();
    zip.finish().unwrap();

    let budget = 256;
    let small = extract_spine_text_budgeted(&path, &["OEBPS/small.xhtml".into()], budget);
    assert_eq!(small[0].as_deref(), Some("短い本文。"));

    let big = extract_spine_text_budgeted(&path, &["OEBPS/big.xhtml".into()], budget);
    assert!(big[0].is_none(), "over-budget resource must be dropped");
}

/// Two imports of one publication must produce one search corpus. The
/// budget used to be charged from the rayon workers, so *which* resources
/// survived was decided by whichever threads happened to reach the
/// counter first — and because the counter was never credited back for a
/// text it rejected, the pre-read check stayed poisoned above the budget
/// afterwards. Here 64 equal-sized CJK resources meet a budget that fits
/// five: the answer must be the first five in spine order, every time.
#[test]
fn budget_truncation_is_a_deterministic_prefix() {
    use std::io::Write;

    const RESOURCES: usize = 64;
    let body = |i: usize| format!("第{i:02}章の本文。");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deterministic.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..RESOURCES {
        zip.start_file(format!("OEBPS/ch{i:02}.xhtml"), stored)
            .unwrap();
        zip.write_all(format!("<html><body><p>{}</p></body></html>", body(i)).as_bytes())
            .unwrap();
    }
    zip.finish().unwrap();

    // Every body is the same length (the index is always two ASCII
    // digits), so the budget lands on an exact resource boundary.
    let each = body(0).len();
    let fits = 5;
    let budget = each * fits;
    let spine: Vec<String> = (0..RESOURCES)
        .map(|i| format!("OEBPS/ch{i:02}.xhtml"))
        .collect();
    let expected: Vec<Option<String>> = (0..RESOURCES)
        .map(|i| (i < fits).then(|| body(i)))
        .collect();

    // Repeated so a scheduling-dependent answer cannot slip through by
    // getting lucky once.
    for _ in 0..32 {
        let texts = extract_spine_text_budgeted(&path, &spine, budget);
        let texts: Vec<Option<String>> = texts
            .iter()
            .map(|t| t.as_deref().map(str::to_owned))
            .collect();
        assert_eq!(texts, expected);
    }
}

/// Every retained copy counts: each `Some` becomes its own
/// `resource_text` row, so a spine repeating one within-budget chapter
/// must stop yielding copies once the aggregate budget is spent —
/// otherwise a tiny archive multiplies one extraction into gigabytes of
/// persistent database while every per-entry cap holds.
#[test]
fn repeated_spine_entries_charge_the_budget_per_retained_copy() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repeat-budget.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("OEBPS/ch.xhtml", stored).unwrap();
    zip.write_all("<html><body><p>満月の夜、風が語る。</p></body></html>".as_bytes())
        .unwrap();
    zip.finish().unwrap();

    let spine: Vec<String> = vec!["OEBPS/ch.xhtml".into(); 5];
    let budget = 80; // fits two 30-byte CJK copies, not five
    let texts = extract_spine_text_budgeted(&path, &spine, budget);

    let len = texts[0]
        .as_deref()
        .expect("first copy fits the budget")
        .len();
    assert_eq!(len, "満月の夜、風が語る。".len());
    assert_eq!(
        texts.iter().filter(|t| t.is_some()).count(),
        budget / len,
        "copies past the budget must be dropped"
    );
    assert!(texts[4].is_none());
}

/// A spine may name the same resource any number of times. The parse
/// must stay bounded (spine cap), the package must collapse the repeats
/// to one spine entry per distinct resolved href (reading order
/// surviving), and — defense in depth, should a repeating spine ever
/// reach it — `extract_spine_text` must still read and extract each
/// distinct resource exactly once, the repeats sharing one `Arc`.
#[test]
fn repeated_spine_entries_are_extracted_once() {
    use std::io::Write;

    use super::super::opf::MAX_SPINE_ITEMS;
    use super::super::package::read_package;

    let repeats = MAX_SPINE_ITEMS + 2_000;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repeated.epub");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", stored).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
    )
    .unwrap();

    // ch02 leads, then ch01 repeats: order has to survive, and the repeats
    // must collapse to one extraction.
    let mut spine = String::from(r#"<itemref idref="c2"/>"#);
    for _ in 0..repeats {
        spine.push_str(r#"<itemref idref="c1"/>"#);
    }
    zip.start_file("OEBPS/content.opf", stored).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>反復</dc:title></metadata>
  <manifest>
    <item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch02.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>{spine}</spine>
</package>"#
        )
        .as_bytes(),
    )
    .unwrap();
    zip.start_file("OEBPS/text/ch01.xhtml", stored).unwrap();
    zip.write_all(r#"<html><body><p>第一章の本文。</p></body></html>"#.as_bytes())
        .unwrap();
    zip.start_file("OEBPS/text/ch02.xhtml", stored).unwrap();
    zip.write_all(br#"<html><body><p>Second chapter text.</p></body></html>"#)
        .unwrap();
    zip.finish().unwrap();

    let package = read_package(&path).unwrap();
    // The 10k+ repeats of ch01 collapse to its first occurrence, after
    // ch02 — reading order survives.
    let spine_hrefs: Vec<&str> = package.spine.iter().map(|item| item.href.as_str()).collect();
    assert_eq!(
        spine_hrefs,
        ["OEBPS/text/ch02.xhtml", "OEBPS/text/ch01.xhtml"]
    );

    // Defense in depth below the package: hand the extractor a spine that
    // still repeats (as a crafted caller could) and every repeat must
    // alias one extraction.
    let spine: Vec<String> = std::iter::once("OEBPS/text/ch02.xhtml".to_string())
        .chain(std::iter::repeat_n(
            "OEBPS/text/ch01.xhtml".to_string(),
            MAX_SPINE_ITEMS - 1,
        ))
        .collect();
    let texts = extract_spine_text(&path, &spine);
    assert_eq!(texts.len(), MAX_SPINE_ITEMS);
    assert_eq!(texts[0].as_deref(), Some("Second chapter text."));

    let first_repeat = texts[1].clone().unwrap();
    assert_eq!(&*first_repeat, "第一章の本文。");
    for text in &texts[2..] {
        let repeat = text.clone().unwrap();
        // Same allocation, so the entry was read and extracted once.
        assert!(std::sync::Arc::ptr_eq(&first_repeat, &repeat));
    }
}
