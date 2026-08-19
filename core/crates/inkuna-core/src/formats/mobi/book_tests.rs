use super::MobiBook;
use crate::test_support::MobiTestBuilder;
use crate::CoreError;

fn tiny_huff_records() -> Vec<Vec<u8>> {
    tiny_huff_records_with(b"HELLO", b'Y')
}

fn tiny_huff_records_with(first: &[u8], second: u8) -> Vec<Vec<u8>> {
    let mut huff = vec![0; 1304];
    huff[..4].copy_from_slice(b"HUFF");
    huff[4..8].copy_from_slice(&24u32.to_be_bytes());
    huff[8..12].copy_from_slice(&24u32.to_be_bytes());
    huff[12..16].copy_from_slice(&1048u32.to_be_bytes());
    for index in 0..256 {
        let offset = 24 + index * 4;
        huff[offset..offset + 4].copy_from_slice(&0x0000_ff88u32.to_be_bytes());
    }

    let first_end = 22 + first.len();
    let second_start = first_end;
    let mut cdic = vec![0; second_start + 3];
    cdic[..4].copy_from_slice(b"CDIC");
    cdic[4..8].copy_from_slice(&16u32.to_be_bytes());
    cdic[8..12].copy_from_slice(&2u32.to_be_bytes());
    cdic[12..16].copy_from_slice(&1u32.to_be_bytes());
    cdic[16..18].copy_from_slice(&4u16.to_be_bytes());
    cdic[18..20].copy_from_slice(&((second_start - 16) as u16).to_be_bytes());
    cdic[20..22].copy_from_slice(&(0x8000 | first.len() as u16).to_be_bytes());
    cdic[22..first_end].copy_from_slice(first);
    cdic[second_start..second_start + 2].copy_from_slice(&0x8001u16.to_be_bytes());
    cdic[second_start + 2] = second;
    vec![huff, cdic]
}

#[test]
fn reads_uncompressed_and_palmdoc_text_including_cjk() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.mobi");
    MobiTestBuilder::new(6)
        .text_records(vec![b"first ".to_vec(), "第一章".as_bytes().to_vec()])
        .write(&plain);
    assert_eq!(
        MobiBook::open(&plain).unwrap().text().unwrap(),
        "first 第一章".as_bytes()
    );

    let compressed = dir.path().join("compressed.mobi");
    MobiTestBuilder::new(6)
        .compression(2)
        .text_records(vec!["月の光与山河".as_bytes().to_vec()])
        .write(&compressed);
    assert_eq!(
        MobiBook::open(&compressed).unwrap().text().unwrap(),
        "月の光与山河".as_bytes()
    );
}

#[test]
fn strips_sized_entries_and_multibyte_overlap_before_decompression() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trailing.mobi");
    // bit 0: three-byte overlap (two bytes + count byte 0b10 => 3 total)
    // bit 1: four data bytes + one-byte backward varint size (0x85 => 5)
    let trailer = vec![0xe4, 0xb8, 0x02, b'T', b'A', b'I', b'L', 0x85];
    MobiTestBuilder::new(6)
        .compression(2)
        .text_records(vec![b"body".to_vec()])
        .trailing_data(0x03, vec![trailer])
        .write(&path);
    assert_eq!(MobiBook::open(&path).unwrap().text().unwrap(), b"body");
}

#[test]
fn reads_huff_cdic_compressed_text_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huff.mobi");
    MobiTestBuilder::new(6)
        .compression(17_480)
        .text_records(vec![vec![0xff, 0xfe]])
        .text_length(6)
        .huff_records(tiny_huff_records())
        .write(&path);
    assert_eq!(MobiBook::open(&path).unwrap().text().unwrap(), b"HELLOY");
}

#[test]
fn hostile_text_length_is_rejected_by_the_length_mismatch_check() {
    // The capacity bound itself (reserving from validated record sums, not
    // the declared length) is not observable from outside `text()`; this
    // pins the rejection that runs regardless.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hostile-length.mobi");
    MobiTestBuilder::new(6)
        .text_records(vec![b"tiny".to_vec(), b"records".to_vec()])
        .text_length(96 * 1024 * 1024)
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    let error = book.text().unwrap_err();
    assert!(error.to_string().contains("does not match"));
}

#[test]
fn extracts_exth_metadata_cover_images_and_language() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metadata.mobi");
    let png = b"\x89PNG\r\n\x1a\ncover";
    MobiTestBuilder::new(6)
        .fullname(b"Old title")
        .locale(0x0804)
        .exth(100, b"Ada")
        .exth(100, "鲁迅".as_bytes())
        .exth(503, "新标题".as_bytes())
        .exth(201, &0u32.to_be_bytes())
        .image(png)
        .image(b"RESC sentinel")
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    assert_eq!(book.title(), "新标题");
    assert_eq!(book.authors(), vec!["Ada", "鲁迅"]);
    assert_eq!(book.language().as_deref(), Some("zh"));
    assert_eq!(book.image(1).as_deref(), Some(png.as_slice()));
    assert_eq!(book.image(2), None);
    assert_eq!(book.image(0), None);
    assert_eq!(book.cover().as_deref(), Some(png.as_slice()));
    assert_eq!(book.encoding(), 65001);
}

#[test]
fn falls_back_to_fullname_then_pdb_name_and_decodes_cp1252() {
    let dir = tempfile::tempdir().unwrap();
    let fullname = dir.path().join("fullname.mobi");
    MobiTestBuilder::new(6)
        .encoding(1252)
        .fullname(b"Caf\xe9")
        .write(&fullname);
    assert_eq!(MobiBook::open(&fullname).unwrap().title(), "Café");

    let pdb_name = dir.path().join("pdb-name.mobi");
    MobiTestBuilder::new(6)
        .name(b"Database title")
        .write(&pdb_name);
    assert_eq!(MobiBook::open(&pdb_name).unwrap().title(), "Database title");
}

#[test]
fn bounds_titles_and_authors_to_the_epub_metadata_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bounded-metadata.mobi");
    let title = "月".repeat(1_000);
    let mut builder = MobiTestBuilder::new(6);
    builder.exth(503, title.as_bytes());
    for index in 0..1_100 {
        builder.exth(
            100,
            format!("Author {index} {}", "x".repeat(3_000)).as_bytes(),
        );
    }
    builder.write(&path);

    let book = MobiBook::open(&path).unwrap();
    assert!(book.title().len() <= 2_048);
    assert!(book.title().is_char_boundary(book.title().len()));
    assert_eq!(book.authors().len(), 1_000);
    assert!(book.authors().iter().all(|author| author.len() <= 2_048));
}

#[test]
fn refuses_drm_with_an_explicit_error() {
    let dir = tempfile::tempdir().unwrap();
    for encryption in [1, 2, 7] {
        let path = dir.path().join(format!("drm-{encryption}.azw"));
        MobiTestBuilder::new(6).encryption(encryption).write(&path);
        let error = MobiBook::open(&path).err().unwrap();
        assert!(matches!(error, CoreError::InvalidPublication(_)));
        assert!(error.to_string().contains("DRM"));
    }
}

#[test]
fn selects_the_kf8_view_after_a_combo_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("combo.mobi");
    let old_cover = b"\x89PNG\r\n\x1a\nold";
    let new_cover = b"\x89PNG\r\n\x1a\nnew";
    let mut kf8 = MobiTestBuilder::new(8);
    kf8.fullname(b"KF8 title")
        .text_records(vec![b"new markup".to_vec()])
        .exth(201, &0u32.to_be_bytes())
        .image(new_cover);
    MobiTestBuilder::new(6)
        .fullname(b"MOBI6 title")
        .text_records(vec![b"old markup".to_vec()])
        .image(old_cover)
        .kf8(kf8)
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    assert!(book.is_kf8());
    assert!(book.kf8_boundary().is_some());
    assert_eq!(book.title(), "KF8 title");
    assert_eq!(book.text().unwrap(), b"new markup");
    assert_eq!(book.image(1).as_deref(), Some(new_cover.as_slice()));
    assert_eq!(book.cover().as_deref(), Some(new_cover.as_slice()));
}

#[test]
fn combo_huff_tables_are_relative_to_the_selected_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("combo-huff.mobi");
    let mut kf8 = MobiTestBuilder::new(8);
    kf8.compression(17_480)
        .text_records(vec![vec![0xff]])
        .text_length(3)
        .huff_records(tiny_huff_records_with(b"NEW", b'N'));
    MobiTestBuilder::new(6)
        .compression(17_480)
        .text_records(vec![vec![0xff]])
        .text_length(3)
        .huff_records(tiny_huff_records_with(b"OLD", b'O'))
        .kf8(kf8)
        .write(&path);

    assert_eq!(MobiBook::open(&path).unwrap().text().unwrap(), b"NEW");
}

#[test]
fn treats_the_exth_boundary_sentinel_as_no_kf8_part() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mobi6-with-sentinel.mobi");
    MobiTestBuilder::new(6)
        .exth(121, &u32::MAX.to_be_bytes())
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    assert!(!book.is_kf8());
    assert_eq!(book.kf8_boundary(), None);
}

#[test]
fn truncated_files_always_return_errors_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("whole.mobi");
    MobiTestBuilder::new(6)
        .fullname(b"A complete header")
        .text_records(vec![b"text".to_vec()])
        .write(&path);
    let bytes = std::fs::read(&path).unwrap();

    for length in [0, 1, 60, 77, 78, 85, 100, bytes.len() - 1] {
        let truncated = dir.path().join(format!("truncated-{length}.mobi"));
        std::fs::write(&truncated, &bytes[..length]).unwrap();
        let result =
            std::panic::catch_unwind(|| MobiBook::open(&truncated).and_then(|book| book.text()));
        assert!(result.is_ok(), "panicked at truncation length {length}");
        assert!(
            result.unwrap().is_err(),
            "accepted truncation length {length}"
        );
    }
}
