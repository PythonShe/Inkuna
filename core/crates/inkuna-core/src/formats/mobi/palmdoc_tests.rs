use super::decompress;
use crate::test_support::palmdoc_compress;

#[test]
fn decompresses_literals_and_byte_pairs() {
    let encoded = b"\x00\x03abc\t\xC1";
    assert_eq!(decompress(encoded, 64).unwrap(), b"\0abc\t A");
}

#[test]
fn decompresses_overlapping_backreferences() {
    // "abc" followed by distance=3, length=6. Overlapping copies must
    // observe bytes appended earlier in the same backreference.
    let encoded = b"\x03abc\x80\x1b";
    assert_eq!(decompress(encoded, 64).unwrap(), b"abcabcabc");
}

#[test]
fn rejects_truncated_or_invalid_backreferences() {
    assert!(decompress(&[0x80], 64).is_err());
    assert!(decompress(&[0x80, 0x08], 64).is_err());
}

#[test]
fn enforces_the_per_record_output_budget() {
    let error = decompress(b"\x08abcdefgh", 7).unwrap_err();
    assert!(error.to_string().contains("decompression limit"));
}

#[test]
fn round_trips_the_synthetic_compressor_with_cjk_utf8() {
    let input = "第一章 月の光与山河".as_bytes();
    let compressed = palmdoc_compress(input);
    assert_eq!(decompress(&compressed, 1024).unwrap(), input);
}
