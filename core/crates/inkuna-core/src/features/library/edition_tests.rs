use super::{edition_key, title_key};

#[test]
fn a_urn_uuid_identifier_keys_canonically() {
    assert_eq!(
        edition_key("urn:uuid:0A1B2C3D-4E5F-4A6B-8C9D-0E1F2A3B4C5D").as_deref(),
        Some("uuid:0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d")
    );
    // Same identifier written three other ways, all one key.
    let canonical = edition_key("urn:uuid:0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d");
    assert_eq!(
        edition_key("  UUID:0A1B2C3D-4E5F-4A6B-8C9D-0E1F2A3B4C5D  "),
        canonical
    );
    assert_eq!(
        edition_key("0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d"),
        canonical
    );
    assert_eq!(
        edition_key("urn:uuid:{0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d}"),
        canonical
    );
}

#[test]
fn the_nil_and_max_uuids_are_not_identities() {
    // Both are "no identifier" written as one; real tools emit them.
    assert_eq!(
        edition_key("urn:uuid:00000000-0000-0000-0000-000000000000"),
        None
    );
    assert_eq!(
        edition_key("urn:uuid:ffffffff-ffff-ffff-ffff-ffffffffffff"),
        None
    );
}

#[test]
fn a_bare_hex_run_is_not_a_uuid() {
    // The `uuid` crate would parse the unhyphenated form; unprefixed it is
    // not distinctive enough to allowlist.
    assert_eq!(edition_key("0a1b2c3d4e5f4a6b8c9d0e1f2a3b4c5d"), None);
    // With the scheme named, it is.
    assert_eq!(
        edition_key("urn:uuid:0a1b2c3d4e5f4a6b8c9d0e1f2a3b4c5d").as_deref(),
        Some("uuid:0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d")
    );
}

#[test]
fn isbn10_and_isbn13_of_one_edition_share_a_key() {
    let ten = edition_key("urn:isbn:0306406152");
    let thirteen = edition_key("urn:isbn:9780306406157");
    assert_eq!(ten.as_deref(), Some("isbn:9780306406157"));
    assert_eq!(ten, thirteen);
    // Hyphenated and spaced printings are the same edition.
    assert_eq!(edition_key("ISBN: 0-306-40615-2"), ten);
    assert_eq!(edition_key("978 0 306 40615 7"), ten);
}

#[test]
fn an_isbn10_check_digit_of_x_is_accepted() {
    // 080442957X is a valid ISBN-10 whose check digit is 10.
    assert_eq!(
        edition_key("urn:isbn:080442957X").as_deref(),
        Some("isbn:9780804429573")
    );
}

#[test]
fn a_checksum_failing_digit_string_is_not_an_isbn() {
    // One digit off the valid 0306406152 / 9780306406157.
    assert_eq!(edition_key("urn:isbn:0306406153"), None);
    assert_eq!(edition_key("urn:isbn:9780306406158"), None);
    // And a bare 13-digit run that is merely a number.
    assert_eq!(edition_key("1234567890123"), None);
}

#[test]
fn a_doi_identifier_keys_lowercased() {
    assert_eq!(
        edition_key("doi:10.1000/XYZ123").as_deref(),
        Some("doi:10.1000/xyz123")
    );
    assert_eq!(
        edition_key("urn:doi:10.1000.10/sub").as_deref(),
        Some("doi:10.1000.10/sub")
    );
    // Not a DOI: no suffix, no registrant, or unprefixed.
    assert_eq!(edition_key("doi:10.1000/"), None);
    assert_eq!(edition_key("doi:10./xyz"), None);
    assert_eq!(edition_key("doi:11.1000/xyz"), None);
    assert_eq!(edition_key("10.1000/xyz"), None);
}

#[test]
fn junk_identifiers_yield_no_identity() {
    // Every one of these is a real-world `dc:identifier` value. A book
    // carrying one counts as itself.
    for junk in [
        "",
        "   ",
        "calibre_id",
        "test",
        "fixture",
        "1",
        "book",
        "月光書房",
        "http://example.com/book",
        "urn:",
        "uuid:",
        "isbn:",
        "urn:isbn:not-a-number",
    ] {
        assert_eq!(edition_key(junk), None, "{junk:?} must not be an identity");
    }
}

#[test]
fn title_key_folds_case_and_whitespace() {
    assert_eq!(
        title_key("  The Tale  of Genji "),
        title_key("the tale of genji")
    );
    assert_ne!(
        title_key("The Tale of Genji"),
        title_key("The Tale of Genjii")
    );
}

#[test]
fn title_key_merges_full_width_and_half_width_latin() {
    // NFKC maps full-width Latin and digits to half-width, so a title
    // typeset inside a CJK book matches the same title typeset plainly.
    assert_eq!(title_key("ＵＮＩＸ　１９７８"), title_key("unix 1978"));
}

#[test]
fn title_key_merges_half_width_and_full_width_katakana() {
    // ｶﾀｶﾅ and カタカナ are the same Japanese title, typeset differently.
    assert_eq!(title_key("ｶﾀｶﾅ"), title_key("カタカナ"));
    // Including the voiced marks, which are separate chars half-width.
    assert_eq!(title_key("ﾊﾟﾝﾀﾞ"), title_key("パンダ"));
}

#[test]
fn title_key_merges_decomposed_and_composed_hangul() {
    // A file provider can hand back decomposed Jamo; NFKC composes them.
    let decomposed = "\u{1112}\u{1161}\u{11AB}\u{1100}\u{116E}\u{11A8}"; // 한국
    assert_eq!(decomposed.chars().count(), 6);
    assert_eq!(title_key(decomposed), title_key("한국"));
}

#[test]
fn title_key_never_merges_simplified_and_traditional_chinese() {
    // Deliberate: 简体 and 繁體 are genuinely different editions.
    assert_ne!(title_key("红楼梦"), title_key("紅樓夢"));
}
