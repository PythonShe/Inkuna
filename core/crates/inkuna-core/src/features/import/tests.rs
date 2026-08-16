use crate::test_support::{
    imported, write_cbz, write_epub, write_epub_parts, write_epub_with, CoverKind, TocKind,
};
use crate::{CoreError, ImportOutcome, Library, Shelf, Sort};

fn count(library: &Library, sql: &str, id: &str) -> i64 {
    library
        .readers
        .with(|conn| conn.query_row(sql, [id], |row| row.get(0)).map_err(Into::into))
        .unwrap()
}

fn table_counts(library: &Library) -> [i64; 4] {
    ["publications", "chapters", "resources", "resource_text"].map(|table| {
        library
            .readers
            .with(|conn| {
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                    .map_err(Into::into)
            })
            .unwrap()
    })
}


#[test]
fn import_extracts_spine_toc_cover_and_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    // Cover extracted as-is under covers/.
    assert_eq!(
        publication.cover_path.as_deref(),
        Some(format!("covers/{}.png", publication.id).as_str())
    );

    // Flattened TOC with CJK titles, fragments, and depth.
    let chapters = library.chapters(&publication.id).unwrap();
    let brief: Vec<(&str, &str, u32)> = chapters
        .iter()
        .map(|c| (c.title.as_str(), c.href.as_str(), c.depth))
        .collect();
    assert_eq!(
        brief,
        vec![
            ("第一章", "OEBPS/text/ch01.xhtml", 0),
            ("第一節", "OEBPS/text/ch01.xhtml#s1", 1),
            ("第二章", "OEBPS/text/ch02.xhtml", 0),
        ]
    );
    assert_eq!(chapters.iter().map(|c| c.idx).collect::<Vec<_>>(), vec![0, 1, 2]);

    // The spine landed in reading order with one text row per resource.
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        2
    );
    let first_body: String = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT rt.body FROM resource_text rt
                 JOIN resources r ON r.id = rt.resource_id
                 WHERE r.publication_id = ?1 ORDER BY r.spine_idx LIMIT 1",
                [&publication.id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(first_body.contains("月の光が窓辺に落ちていた。"));

    // remove() cascades the children.
    library.remove(&publication.id).unwrap();
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM chapters WHERE publication_id = ?1", &publication.id),
        0
    );
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        0
    );
}

#[test]
fn no_toc_epub_still_builds_a_complete_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(&epub, "無目次", "作者", "ja", TocKind::None, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    assert!(library.chapters(&publication.id).unwrap().is_empty());
    assert!(publication.cover_path.is_none());
    // The corpus keys off the spine, not the TOC.
    let text_rows = count(
        &library,
        "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
             (SELECT id FROM resources WHERE publication_id = ?1)",
        &publication.id,
    );
    assert_eq!(text_rows, 2);
}

/// A cover href the extension heuristic cannot make sense of degrades to
/// no cover, exactly like unreadable cover bytes do — it must never fail
/// the import. `img.old/cover` under an unknown media type derives the
/// "extension" `old/cover`, and writing `covers/<id>.old/cover` fails on
/// the missing intermediate directory.
#[test]
fn malformed_cover_href_still_imports_without_a_cover() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(
        &epub,
        "壊れた表紙",
        "作者",
        "ja",
        TocKind::Nav,
        CoverKind::MalformedHref,
    );

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    assert!(publication.cover_path.is_none());
    // The rest of the book imported normally.
    assert_eq!(publication.title, "壊れた表紙");
    assert_eq!(library.chapters(&publication.id).unwrap().len(), 3);
    // And nothing was left behind under covers/.
    assert_eq!(std::fs::read_dir(data_dir.join("covers")).unwrap().count(), 0);
}

#[test]
fn ncx_fallback_supplies_the_toc() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(&epub, "旧式目次", "作者", "ja", TocKind::Ncx, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    let chapters = library.chapters(&publication.id).unwrap();
    assert_eq!(chapters.len(), 3);
    assert_eq!(chapters[1].title, "第一節");
    assert_eq!(chapters[1].href, "OEBPS/text/ch01.xhtml#s1");
    assert_eq!(chapters[1].depth, 1);
}

/// The worst crafted-EPUB outcome is *persistent*: before the TOC cap, a
/// ~385 KB nav doc imported "successfully" into 2.5M `chapters` rows and
/// a 480 MB database. The TOC is optional, so the import must still
/// succeed — with the chapter rows truncated at the cap.
#[test]
fn crafted_toc_is_capped_at_max_entries_and_still_imports() {
    use crate::formats::epub::MAX_TOC_ENTRIES;

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("toc-bomb.epub");
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>目次爆弾</dc:title></metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let mut nav = String::from(
        r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>"#,
    );
    for _ in 0..MAX_TOC_ENTRIES + 500 {
        nav.push_str(r#"<li><a href="ch01.xhtml">章</a></li>"#);
    }
    nav.push_str("</ol></nav></body></html>");
    write_epub_parts(
        &epub,
        opf,
        &[
            ("nav.xhtml", nav.as_str()),
            ("ch01.xhtml", r#"<html><body><p>本文。</p></body></html>"#),
        ],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    assert_eq!(library.chapters(&publication.id).unwrap().len(), MAX_TOC_ENTRIES);
}

/// The round-5 amplifier, pinned at the database: one navPoint whose
/// 64 KiB label was re-cloned by each of thousands of sibling
/// `<content>` elements — a ~2 KB archive persisting 630 MB of
/// `chapters` rows while every per-entry cap saw in-bounds numbers. NCX
/// fixes a navPoint to exactly one `<content>` (`navLabel+, content,
/// navPoint*`), so only the first wins: one row.
#[test]
fn repeated_ncx_content_elements_yield_a_single_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("ncx-label-bomb.epub");
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>複製爆弾</dc:title></metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"#;
    let label = "月".repeat(64 * 1024 / 3); // ~64 KiB of CJK
    let mut ncx = format!(
        r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint><navLabel><text>{label}</text></navLabel>"#
    );
    for _ in 0..1000 {
        ncx.push_str(r#"<content src="ch01.xhtml"/>"#);
    }
    ncx.push_str("</navPoint></navMap></ncx>");
    write_epub_parts(
        &epub,
        opf,
        &[
            ("toc.ncx", ncx.as_str()),
            ("ch01.xhtml", r#"<html><body><p>本文。</p></body></html>"#),
        ],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    let chapters = library.chapters(&publication.id).unwrap();
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].href, "OEBPS/ch01.xhtml");
}

/// With one `<content>` per navPoint nothing amplifies, but many
/// navPoints with large labels still sum past any honest TOC. The
/// aggregate byte budget bounds what reaches the `chapters` table; the
/// import still succeeds with the honest prefix.
#[test]
fn crafted_ncx_toc_is_bounded_by_the_byte_budget() {
    use crate::formats::epub::MAX_TOC_TOTAL_BYTES;

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("ncx-budget-bomb.epub");
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>予算爆弾</dc:title></metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"#;
    let label = "書".repeat(32 * 1024 / 3); // ~32 KiB of CJK per label
    let n = 300; // ~9.6 MiB retained if uncapped — past the 8 MiB budget
    let mut ncx =
        String::from(r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>"#);
    for i in 0..n {
        ncx.push_str(&format!(
            r##"<navPoint><navLabel><text>{label}</text></navLabel><content src="ch01.xhtml#p{i}"/></navPoint>"##
        ));
    }
    ncx.push_str("</navMap></ncx>");
    write_epub_parts(
        &epub,
        opf,
        &[
            ("toc.ncx", ncx.as_str()),
            ("ch01.xhtml", r#"<html><body><p>本文。</p></body></html>"#),
        ],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    let chapters = library.chapters(&publication.id).unwrap();
    assert!(!chapters.is_empty());
    assert!(chapters.len() < n, "budget did not truncate: {} rows", chapters.len());
    let retained: usize = chapters.iter().map(|c| c.title.len() + c.href.len()).sum();
    assert!(retained <= MAX_TOC_TOTAL_BYTES);
}

/// MAX_HREF_BYTES is checked on the manifest href as written, but the
/// *resolved* href is what every spine row persists — and resolution
/// prepends the OPF's directory, which a crafted container.xml can push
/// toward zip's 65,535-byte name ceiling. Uncapped, each itemref
/// retained its own ~multi-KB copy in `resources`; oversized resolved
/// hrefs must degrade away instead.
#[test]
fn oversized_resolved_spine_hrefs_degrade_away() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("long-dir-bomb.epub");
    let long_dir = "d".repeat(8_000);
    let file = std::fs::File::create(&epub).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="{long_dir}/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#
        )
        .as_bytes(),
    )
    .unwrap();
    zip.start_file(format!("{long_dir}/content.opf"), deflated).unwrap();
    zip.write_all(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>深層書庫</dc:title></metadata>
  <manifest><item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/><itemref idref="c1"/><itemref idref="c1"/></spine>
</package>"#
            .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap();

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    // Every resolved spine href carries the ~8 KB directory: all skipped,
    // no `resources` row retains the amplified path.
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        0
    );
    assert_eq!(publication.title, "深層書庫");
}

/// The round-7 amplifier, pinned at the hot path's source: a crafted
/// multi-hundred-KB `<dc:title>` (measured at 60 MiB from a 62 KB
/// archive) used to persist whole and re-materialize on every `list()`
/// and every search keystroke. The cap holds at the push site, and the
/// cut must land on a `char` boundary — the fixture is CJK (3-byte
/// chars) and the cap is not a multiple of 3, so a naive byte-offset
/// slice would split a character.
#[test]
fn oversized_cjk_metadata_is_bounded_on_what_list_returns() {
    use crate::formats::epub::MAX_METADATA_VALUE_BYTES;

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("title-bomb.epub");
    let title = "書".repeat(200_000); // 600 KB, ~300x the cap
    let creator = "著".repeat(200_000);
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title><dc:creator>{creator}</dc:creator>
  </metadata>
  <manifest><item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
    );
    write_epub_parts(
        &epub,
        &opf,
        &[("ch01.xhtml", r#"<html><body><p>本文。</p></body></html>"#)],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    imported(library.import(epub.to_str().unwrap()).unwrap());

    // Assert on what the hot path actually returns, not on parser
    // internals.
    let all = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    let publication = &all[0];
    assert!(
        publication.title.len() <= MAX_METADATA_VALUE_BYTES,
        "title not bounded: {} bytes",
        publication.title.len()
    );
    // Only whole characters survive the cut: a byte-offset truncation
    // would leave a mangled trailing char (or fail UTF-8 on the way).
    assert!(!publication.title.is_empty());
    assert!(publication.title.chars().all(|c| c == '書'));
    assert_eq!(publication.authors.len(), 1);
    assert!(publication.authors[0].len() <= MAX_METADATA_VALUE_BYTES);
    assert!(publication.authors[0].chars().all(|c| c == '著'));
}

/// Guards the metadata cap from becoming the data-loss bug it prevents:
/// an honest book's CJK title, author, and language come back from
/// `list()` byte-for-byte verbatim.
#[test]
fn honest_cjk_metadata_survives_the_cap_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "吾輩は猫である", "夏目漱石", "ja");

    let library = Library::open(dir.path().join("library")).unwrap();
    imported(library.import(epub.to_str().unwrap()).unwrap());

    let all = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(all[0].title, "吾輩は猫である");
    assert_eq!(all[0].authors, vec!["夏目漱石".to_string()]);
    assert_eq!(all[0].language.as_deref(), Some("ja"));
}

/// The round-8 tuning: a spine repeating one resolved href must not
/// multiply `resources` (and `resource_text`) rows — EPUB 3 requires each
/// itemref to reference a unique resource, so an honest book never
/// repeats, while a crafted spine used to persist one ~4 KB href and one
/// 8 MiB text per repeat.
#[test]
fn duplicate_spine_hrefs_collapse_to_one_resource_row() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("repeat-spine.epub");
    let itemrefs = r#"<itemref idref="c1"/>"#.repeat(500);
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>反復爆弾</dc:title></metadata>
  <manifest><item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine>{itemrefs}</spine>
</package>"#
    );
    write_epub_parts(
        &epub,
        &opf,
        &[("ch01.xhtml", r#"<html><body><p>同じ章。</p></body></html>"#)],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        1
    );
    assert_eq!(
        count(
            &library,
            "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
                 (SELECT id FROM resources WHERE publication_id = ?1)",
            &publication.id,
        ),
        1
    );
}

/// The manifest is a mandatory part, so a crafted OPF listing an absurd
/// number of items is not degraded around — it fails the import cleanly
/// (before the cap: a 355 KB file parsed into a 616 MB resident set) and
/// sweeps the staged copy.
#[test]
fn manifest_bomb_fails_the_import_cleanly() {
    use crate::formats::epub::MAX_MANIFEST_ITEMS;

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("manifest-bomb.epub");
    let items = r#"<item id="i" href="h"/>"#.repeat(MAX_MANIFEST_ITEMS + 1);
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>目録爆弾</dc:title></metadata>
  <manifest>{items}</manifest>
  <spine></spine>
</package>"#
    );
    write_epub_parts(&epub, &opf, &[]);

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let err = library.import(epub.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, CoreError::InvalidPublication(_)), "got {err:?}");
    // Nothing persisted: no publication row, no staged file left behind.
    assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
    assert_eq!(std::fs::read_dir(data_dir.join("books")).unwrap().count(), 0);
}

/// The whole point of the persist budget: tripping it must leave *no
/// trace* — the transaction rolled back (every table count unchanged)
/// and the staged book and cover swept — instead of the silent-success
/// mangled-library outcome every previous amplification produced. Real
/// ceilings are unreachable by anything the parse caps pass, so the trip
/// path is exercised with tiny explicit ceilings through the same
/// `commit_import_budgeted` the production wrapper delegates to.
#[test]
fn tripped_budget_rolls_back_rows_and_sweeps_staged_files() {
    use super::budget::PersistBudget;
    use super::pipeline::Prepared;

    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existing.epub");
    write_epub(&existing, "既存の本", "先住作家", "ja");
    let bomb = dir.path().join("second.epub");
    write_epub(&bomb, "予算超過", "後発作家", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    imported(library.import(existing.to_str().unwrap()).unwrap());
    let baseline = table_counts(&library);

    let fresh = |path: &std::path::Path| match library
        .prepare_import(path.to_str().unwrap())
        .unwrap()
    {
        Prepared::Fresh(prepared) => *prepared,
        Prepared::Duplicate(p) => panic!("unexpected duplicate of {}", p.id),
    };

    // Row ceiling.
    let err = library
        .commit_import_budgeted(fresh(&bomb), PersistBudget::with_limits(2, u64::MAX))
        .unwrap_err();
    match &err {
        CoreError::InvalidPublication(msg) => {
            assert!(msg.contains("2 rows"), "limit not named: {msg}")
        }
        other => panic!("expected InvalidPublication, got {other:?}"),
    }
    // Byte ceiling.
    let err = library
        .commit_import_budgeted(fresh(&bomb), PersistBudget::with_limits(u64::MAX, 64))
        .unwrap_err();
    match &err {
        CoreError::InvalidPublication(msg) => {
            assert!(msg.contains("64 bytes"), "limit not named: {msg}")
        }
        other => panic!("expected InvalidPublication, got {other:?}"),
    }

    // No trace: every table exactly at baseline, no orphan files.
    assert_eq!(table_counts(&library), baseline);
    assert_eq!(std::fs::read_dir(data_dir.join("books")).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(data_dir.join("covers")).unwrap().count(), 1);
}

/// Guard against the breaker becoming the data-loss bug it prevents: an
/// honest book — CJK, because multi-byte titles and text are where naive
/// byte accounting would misjudge an honest book first — imports through
/// the production ceilings completely unaffected.
#[test]
fn honest_cjk_book_imports_unaffected_by_the_budget() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "吾輩は猫である", "夏目漱石", "ja");

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    assert_eq!(publication.title, "吾輩は猫である");
    assert_eq!(library.chapters(&publication.id).unwrap().len(), 3);
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        2
    );
    assert_eq!(
        count(
            &library,
            "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
                 (SELECT id FROM resources WHERE publication_id = ?1)",
            &publication.id,
        ),
        2
    );
}

/// The meter is per-publication, never shared: three books of ~60,003
/// rows each (fine individually, 180,009 combined — past the 150,000
/// ceiling if one meter survived across commits) all import through
/// `import_batch`, which funnels into the same `commit_import` path.
#[test]
fn budget_is_per_publication_across_batch_imports() {
    use crate::BatchImportOutcome;

    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<String> = (0..3)
        .map(|i| {
            let epub = dir.path().join(format!("volume-{i}.epub"));
            let opf = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>大全集 第{i}巻</dc:title></metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
            );
            let mut nav = String::from(
                r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>"#,
            );
            for _ in 0..60_000 {
                nav.push_str(r#"<li><a href="ch01.xhtml">章</a></li>"#);
            }
            nav.push_str("</ol></nav></body></html>");
            write_epub_parts(
                &epub,
                &opf,
                &[
                    ("nav.xhtml", nav.as_str()),
                    ("ch01.xhtml", r#"<html><body><p>本文。</p></body></html>"#),
                ],
            );
            epub.to_str().unwrap().to_string()
        })
        .collect();

    let library = Library::open(dir.path().join("library")).unwrap();
    let outcomes = library.import_batch(&paths);
    for outcome in &outcomes {
        let publication = match outcome {
            BatchImportOutcome::Imported(p) => p,
            other => panic!("expected Imported, got {other:?}"),
        };
        assert_eq!(library.chapters(&publication.id).unwrap().len(), 60_000);
    }
}

#[test]
fn rejects_non_epub_naming_the_format() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("鬼滅の刃 第1巻.cbz");
    write_cbz(&cbz);

    let library = Library::open(dir.path().join("library")).unwrap();
    let err = library.import(cbz.to_str().unwrap()).unwrap_err();
    match err {
        CoreError::UnsupportedFormat(Some(format)) => assert_eq!(format, "cbz"),
        other => panic!("expected UnsupportedFormat with name, got {other:?}"),
    }
    assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
}

#[test]
fn batch_import_reports_each_completed_file_with_increasing_counts() {
    use std::sync::Mutex;

    let dir = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..3 {
        let epub = dir.path().join(format!("book-{i}.epub"));
        write_epub(&epub, &format!("第{i}巻"), "著者", "ja");
        paths.push(epub.to_str().unwrap().to_string());
    }
    // A failure still counts as a completed file.
    let comic = dir.path().join("comic.cbz");
    write_cbz(&comic);
    paths.push(comic.to_str().unwrap().to_string());

    let library = Library::open(dir.path().join("library")).unwrap();
    let events: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());
    let outcomes = library.import_batch_with(&paths, &|done, path| {
        events.lock().unwrap().push((done, path.to_string()));
    });

    assert_eq!(outcomes.len(), paths.len());
    let events = events.into_inner().unwrap();
    // One event per file, counts strictly 1..=N even though rayon finishes
    // files in whatever order it likes.
    assert_eq!(
        events.iter().map(|(done, _)| *done).collect::<Vec<_>>(),
        (1..=paths.len()).collect::<Vec<_>>()
    );
    let mut reported: Vec<_> = events.into_iter().map(|(_, path)| path).collect();
    reported.sort();
    let mut expected = paths.clone();
    expected.sort();
    assert_eq!(reported, expected);
}

#[test]
fn untitled_epub_falls_back_to_the_file_stem_normalized_to_nfc() {
    let dir = tempfile::tempdir().unwrap();
    // Decomposed Hangul (NFD), as APFS/HFS+ file providers hand names back:
    // renders as 밤의 서재 but is byte-for-byte different from the composed
    // form every other title in the library uses.
    let decomposed = "\u{1107}\u{1161}\u{11B7}\u{110B}\u{1174} \
                      \u{1109}\u{1165}\u{110C}\u{1162}";
    let epub = dir.path().join(format!("{decomposed}.epub"));
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:language>ko</dc:language></metadata>
  <manifest><item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    write_epub_parts(
        &epub,
        opf,
        &[("ch01.xhtml", r#"<html><body><p>본문.</p></body></html>"#)],
    );

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    assert_eq!(publication.title, "밤의 서재");
}

#[test]
fn import_is_idempotent_by_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let first = imported(library.import(epub.to_str().unwrap()).unwrap());

    // Same file again.
    match library.import(epub.to_str().unwrap()).unwrap() {
        ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
        other => panic!("expected duplicate, got {other:?}"),
    }

    // Byte-identical copy under another name.
    let copy = dir.path().join("renamed-copy.epub");
    std::fs::copy(&epub, &copy).unwrap();
    match library.import(copy.to_str().unwrap()).unwrap() {
        ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
        other => panic!("expected duplicate, got {other:?}"),
    }

    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
    // No stray files: exactly one book in storage.
    let books: Vec<_> = std::fs::read_dir(data_dir.join("books"))
        .unwrap()
        .collect();
    assert_eq!(books.len(), 1);
}


#[test]
fn batch_import_reports_per_item_outcomes_in_order() {
    use crate::BatchImportOutcome;

    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.epub");
    write_epub(&good, "月光書房", "紫式部", "ja");
    let comic = dir.path().join("comic.cbz");
    write_cbz(&comic);
    let copy = dir.path().join("copy.epub");
    std::fs::copy(&good, &copy).unwrap();

    let library = Library::open(dir.path().join("library")).unwrap();
    let outcomes = library.import_batch(&[
        good.to_str().unwrap().to_string(),
        comic.to_str().unwrap().to_string(),
        copy.to_str().unwrap().to_string(),
    ]);

    assert_eq!(outcomes.len(), 3);
    // Identical content in one batch: exactly one Imported, the copy a
    // Duplicate of it (whichever parallel branch wins the race).
    let (imported, duplicate) = match (&outcomes[0], &outcomes[2]) {
        (BatchImportOutcome::Imported(a), BatchImportOutcome::Duplicate(b))
        | (BatchImportOutcome::Duplicate(b), BatchImportOutcome::Imported(a)) => (a, b),
        other => panic!("expected one Imported + one Duplicate, got {other:?}"),
    };
    assert_eq!(imported.id, duplicate.id);
    match &outcomes[1] {
        BatchImportOutcome::Failed { path, error } => {
            assert_eq!(path, comic.to_str().unwrap());
            assert!(matches!(error, CoreError::UnsupportedFormat(Some(f)) if f == "cbz"));
        }
        other => panic!("expected Failed for the comic, got {other:?}"),
    }
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
}

