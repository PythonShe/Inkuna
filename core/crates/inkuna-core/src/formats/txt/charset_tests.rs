use super::*;

fn encoded(encoding: &'static encoding_rs::Encoding, text: &str) -> Vec<u8> {
    let (bytes, _, had_errors) = encoding.encode(text);
    assert!(!had_errors);
    bytes.into_owned()
}

#[test]
fn decodes_gb18030_four_byte_sequences_as_gbk() {
    let mut bytes = encoded(encoding_rs::GBK, "第一章 春天\n");
    // U+1F600 uses a four-byte GB18030 sequence, which the WHATWG GBK
    // decoder intentionally accepts.
    bytes.extend_from_slice(&[0x94, 0x39, 0xfc, 0x36]);
    let decoded = decode_text(&bytes);
    assert_eq!(decoded.encoding, "GBK");
    assert!(decoded.text.contains("第一章 春天"));
    assert!(decoded.text.contains('😀'));
}

#[test]
fn decodes_big5_and_shift_jis() {
    let big5 = decode_text(&encoded(encoding_rs::BIG5, "第一章 春天"));
    assert_eq!(big5.encoding, "Big5");
    assert_eq!(big5.text, "第一章 春天");

    let shift_jis = decode_text(&encoded(encoding_rs::SHIFT_JIS, "第一章 春の日"));
    assert_eq!(shift_jis.encoding, "Shift_JIS");
    assert_eq!(shift_jis.text, "第一章 春の日");
}

#[test]
fn honors_utf8_bom_and_detects_bomless_utf8() {
    let with_bom = decode_text(b"\xef\xbb\xbf\xe7\xac\xac\xe4\xb8\x80\xe7\xab\xa0\r\n\xe6\x98\xa5");
    assert_eq!(with_bom.encoding, "UTF-8");
    assert_eq!(with_bom.text, "第一章\n春");

    let without_bom = decode_text("第一章\n春".as_bytes());
    assert_eq!(without_bom.encoding, "UTF-8");
    assert_eq!(without_bom.text, "第一章\n春");
}

#[test]
fn detects_utf16le_with_and_without_bom() {
    let mut with_bom = vec![0xff, 0xfe];
    with_bom.extend("第一章\r\n春".encode_utf16().flat_map(u16::to_le_bytes));
    let decoded = decode_text(&with_bom);
    assert_eq!(decoded.encoding, "UTF-16LE");
    assert_eq!(decoded.text, "第一章\n春");

    let without_bom: Vec<u8> = "Chapter 1\rBody text"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let decoded = decode_text(&without_bom);
    assert_eq!(decoded.encoding, "UTF-16LE");
    assert_eq!(decoded.text, "Chapter 1\nBody text");
}

#[test]
fn normalizes_unicode_line_separators_and_lossy_decode_never_panics() {
    let decoded = decode_text(b"one\x85two\x81three");
    assert!(!decoded.text.is_empty());

    let decoded = decode_text("一\u{85}二\u{2028}三\u{2029}四".as_bytes());
    assert_eq!(decoded.text, "一\n二\n三\n四");
}
