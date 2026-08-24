use super::{deobfuscate, parse_encryption_xml, ObfuscationScheme};

const UUID_ID: &str = "urn:uuid:12345678-90ab-cdef-1234-567890abcdef";

fn sample_font(len: usize) -> Vec<u8> {
    (0..len).map(|b| (b % 251) as u8).collect()
}

#[test]
fn parses_both_schemes_and_flags_unknown_algorithms() {
    let xml = r#"<?xml version="1.0"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
            xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData>
    <enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
    <enc:CipherData><enc:CipherReference URI="OEBPS/fonts/serif%20face.ttf"/></enc:CipherData>
  </enc:EncryptedData>
  <enc:EncryptedData>
    <enc:EncryptionMethod Algorithm="http://ns.adobe.com/pdf/enc#RC"/>
    <enc:CipherData><enc:CipherReference URI="OEBPS/fonts/sans.otf"/></enc:CipherData>
  </enc:EncryptedData>
  <enc:EncryptedData>
    <enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>
    <enc:CipherData><enc:CipherReference URI="OEBPS/chapter1.xhtml"/></enc:CipherData>
  </enc:EncryptedData>
</encryption>"#;
    let entries = parse_encryption_xml(xml);
    assert_eq!(entries.len(), 3);
    // Percent-encoding decodes like every other href.
    assert_eq!(entries[0].href, "OEBPS/fonts/serif face.ttf");
    assert_eq!(entries[0].scheme, ObfuscationScheme::Idpf);
    assert_eq!(entries[1].href, "OEBPS/fonts/sans.otf");
    assert_eq!(entries[1].scheme, ObfuscationScheme::Adobe);
    assert_eq!(entries[2].scheme, ObfuscationScheme::Unsupported);
}

#[test]
fn malformed_declaration_degrades_to_empty() {
    assert!(parse_encryption_xml("not xml at all <<<").is_empty());
    // A method with no reference contributes nothing.
    let xml = r#"<encryption><EncryptedData>
        <EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
        </EncryptedData></encryption>"#;
    assert!(parse_encryption_xml(xml).is_empty());
}

/// XOR round trip: obfuscating (the same operation) then deobfuscating
/// restores the original, and only the scheme's prefix is touched.
#[test]
fn idpf_round_trip_touches_exactly_1040_bytes() {
    let original = sample_font(2000);
    let mut mangled = original.clone();
    assert!(deobfuscate(
        &mut mangled,
        ObfuscationScheme::Idpf,
        Some(UUID_ID)
    ));
    assert_ne!(mangled[..1040], original[..1040]);
    assert_eq!(mangled[1040..], original[1040..]);
    assert!(deobfuscate(
        &mut mangled,
        ObfuscationScheme::Idpf,
        Some(UUID_ID)
    ));
    assert_eq!(mangled, original);
}

#[test]
fn adobe_round_trip_touches_exactly_1024_bytes() {
    let original = sample_font(2000);
    let mut mangled = original.clone();
    assert!(deobfuscate(
        &mut mangled,
        ObfuscationScheme::Adobe,
        Some(UUID_ID)
    ));
    assert_ne!(mangled[..1024], original[..1024]);
    assert_eq!(mangled[1024..], original[1024..]);
    assert!(deobfuscate(
        &mut mangled,
        ObfuscationScheme::Adobe,
        Some(UUID_ID)
    ));
    assert_eq!(mangled, original);
}

/// The IDPF key is SHA-1 of the whitespace-stripped identifier: an
/// identifier written with embedded whitespace must derive the same key.
#[test]
fn idpf_key_strips_whitespace() {
    let mut a = sample_font(64);
    let mut b = sample_font(64);
    assert!(deobfuscate(&mut a, ObfuscationScheme::Idpf, Some("id-42")));
    assert!(deobfuscate(
        &mut b,
        ObfuscationScheme::Idpf,
        Some(" id\t-\n42\r ")
    ));
    assert_eq!(a, b);
}

/// The Adobe key ignores the urn prefix, hyphens, and colons — the same
/// UUID in different spellings derives one key.
#[test]
fn adobe_key_normalizes_uuid_spellings() {
    let mut a = sample_font(64);
    let mut b = sample_font(64);
    assert!(deobfuscate(&mut a, ObfuscationScheme::Adobe, Some(UUID_ID)));
    assert!(deobfuscate(
        &mut b,
        ObfuscationScheme::Adobe,
        Some("1234567890abcdef1234567890abcdef")
    ));
    assert_eq!(a, b);
}

/// Adobe's key needs a UUID; the first XOR'd byte must be the first key
/// byte (0x12) against a zero input — pinning byte order end to end.
#[test]
fn adobe_key_bytes_are_uuid_digits_in_order() {
    let mut zeroes = vec![0u8; 16];
    assert!(deobfuscate(
        &mut zeroes,
        ObfuscationScheme::Adobe,
        Some(UUID_ID)
    ));
    assert_eq!(
        zeroes,
        vec![
            0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
            0xcd, 0xef
        ]
    );
}

#[test]
fn missing_or_unusable_keys_refuse() {
    let mut bytes = sample_font(64);
    let untouched = bytes.clone();
    assert!(!deobfuscate(&mut bytes, ObfuscationScheme::Idpf, None));
    assert!(!deobfuscate(
        &mut bytes,
        ObfuscationScheme::Adobe,
        Some("not-a-uuid")
    ));
    assert!(!deobfuscate(
        &mut bytes,
        ObfuscationScheme::Unsupported,
        Some(UUID_ID)
    ));
    assert_eq!(bytes, untouched, "a refused deobfuscation must not write");
}
