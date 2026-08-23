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
fn detects_bomless_utf16_cjk_in_both_byte_orders() {
    let text = "春夏秋冬 山中月夜".repeat(64);
    let little_endian: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let decoded = decode_text(&little_endian);
    assert_eq!(decoded.encoding, "UTF-16LE");
    assert_eq!(decoded.text, text);

    let big_endian: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let decoded = decode_text(&big_endian);
    assert_eq!(decoded.encoding, "UTF-16BE");
    assert_eq!(decoded.text, text);
}

#[test]
fn cjk_utf8_gbk_and_big5_samples_remain_non_utf16() {
    let utf8 = "春夏秋冬 山中月夜".as_bytes();
    assert!(bomless_utf16(utf8).is_none());
    assert_eq!(decode_text(utf8).text, "春夏秋冬 山中月夜");

    let gbk = encoded(encoding_rs::GBK, "第一章 春天");
    assert!(bomless_utf16(&gbk).is_none());
    let decoded = decode_text(&gbk);
    assert_eq!(decoded.encoding, "GBK");
    assert_eq!(decoded.text, "第一章 春天");

    let big5 = encoded(encoding_rs::BIG5, "第一章 春天");
    assert!(bomless_utf16(&big5).is_none());
    let decoded = decode_text(&big5);
    assert_eq!(decoded.encoding, "Big5");
    assert_eq!(decoded.text, "第一章 春天");
}

#[test]
fn normalizes_unicode_line_separators_and_lossy_decode_never_panics() {
    let decoded = decode_text(b"one\x85two\x81three");
    assert!(!decoded.text.is_empty());

    let decoded = decode_text("一\u{85}二\u{2028}三\u{2029}四".as_bytes());
    assert_eq!(decoded.text, "一\n二\n三\n四");
}

fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
    if little_endian {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        text.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }
}

fn assert_roundtrips(text: &str) {
    for (little_endian, expected) in [(true, "UTF-16LE"), (false, "UTF-16BE")] {
        let bytes = utf16(text, little_endian);
        assert_eq!(
            bomless_utf16(&bytes).map(|encoding| encoding.name()),
            Some(expected),
            "byte order misread for {text:?} (little_endian={little_endian})"
        );
        let decoded = decode_text(&bytes);
        assert_eq!(decoded.encoding, expected);
        assert_eq!(decoded.text, text);
    }
}

/// The regression the parity rule shipped: NULs in CJK UTF-16 come from the
/// *low* byte of U+xx00 characters — 一 U+4E00, 言 U+8A00, 退 U+9000 — which
/// is the opposite parity from the high-byte NULs of ASCII UTF-16. A
/// UTF-16LE sample of this text measures even=209/odd=0 and the parity rule
/// called it UTF-16BE, decoding 一片寂静 into "N䝲쉛妗…".
#[test]
fn cjk_with_low_byte_nul_characters_keeps_its_byte_order() {
    assert_roundtrips(&"一片寂静，月光洒在窗台上。".repeat(16));
    assert_roundtrips(&"他一个人走在长长的街上，一切都安静了。".repeat(16));
}

/// The same characters with no ASCII anywhere: one parity holds every NUL
/// and the other holds none, so parity has nothing at all to compare.
#[test]
fn cjk_without_any_ascii_keeps_its_byte_order() {
    let text = "一言退中文测试内容没有空格".repeat(16);
    assert!(!text.is_ascii());
    for little_endian in [true, false] {
        let bytes = utf16(&text, little_endian);
        let (even, odd) = bytes
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == 0)
            .fold((0usize, 0usize), |(even, odd), (index, _)| {
                if index % 2 == 0 {
                    (even + 1, odd)
                } else {
                    (even, odd + 1)
                }
            });
        assert!(even == 0 || odd == 0, "expected all NULs at one parity");
    }
    assert_roundtrips(&text);
}

#[test]
fn mixed_cjk_and_ascii_keeps_its_byte_order() {
    assert_roundtrips(&"Chapter 一 begins. 他说：一路平安！ Then 退回。".repeat(16));
    assert_roundtrips(&"第一章 Chapter 1\n山中 Body text\n".repeat(16));
}

#[test]
fn ascii_dominant_utf16_keeps_its_byte_order() {
    assert_roundtrips(&"Chapter 1\nBody text goes here.\n".repeat(16));
}

/// Japanese and Korean flip into plausible-looking ideographs more often
/// than Chinese does, so they are the tightest margin the scorer sees.
#[test]
fn japanese_and_korean_utf16_keep_their_byte_order() {
    assert_roundtrips(&"彼は一人で言った。退屈な日々が続く。".repeat(16));
    assert_roundtrips(&"그는 한 사람으로 걸었다. 달빛이 내렸다.".repeat(16));
}

/// Smart quotes and em dashes are near-universal in English prose. The
/// valid-UTF-8 bail-out rejected any UTF-16 sample carrying one, which
/// stopped the file being detected as TXT at all.
#[test]
fn english_utf16_with_smart_punctuation_is_still_utf16() {
    assert_roundtrips(&"It was the best of times — the ‘worst’ too. ".repeat(16));
    assert_roundtrips(&"“Well,” he said, “it’s done.” ".repeat(16));
}

/// A NUL byte is the whole gate: valid UTF-8, GBK, Big5 and Shift_JIS text
/// can never contain one, and a stray NUL alone must not promote a sample
/// whose flipped readings are equally garbage.
#[test]
fn nul_bearing_non_utf16_samples_are_rejected() {
    let mut utf8 = "It’s fine — really. ".repeat(64).into_bytes();
    utf8.push(0);
    assert!(bomless_utf16(&utf8).is_none());
    assert_eq!(decode_text(&utf8).encoding, "UTF-8");

    let mut cjk = "春夏秋冬 山中月夜".repeat(32).into_bytes();
    cjk.extend_from_slice(&[0, 0]);
    assert!(bomless_utf16(&cjk).is_none());

    // Short Latin text scores well as UTF-16 by luck — read big-endian,
    // `text\0then binary` is seven CJK ideographs and a `t`. The NUL floor,
    // not the scoring, is what rejects it.
    assert!(bomless_utf16(b"text\x00then binary").is_none());

    assert!(bomless_utf16(&[0, 1, 0, 2, 3, 0, 4, 0]).is_none());
    assert!(bomless_utf16(&[0u8; 512]).is_none());
    assert!(bomless_utf16(b"A\x00").is_none(), "too short to score");
    assert!(bomless_utf16(&[]).is_none());
}

/// The clamped sample is truncated to whole code units, so an odd-length
/// file cannot shift every pair by one byte and invert the reading.
#[test]
fn odd_length_samples_are_truncated_to_whole_code_units() {
    let mut bytes = utf16(&"春夏秋冬 山中月夜".repeat(16), true);
    bytes.push(b'!');
    assert_eq!(
        bomless_utf16(&bytes).map(|encoding| encoding.name()),
        Some("UTF-16LE")
    );
}
