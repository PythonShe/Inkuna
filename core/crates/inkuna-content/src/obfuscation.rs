//! EPUB font obfuscation (OCF "font mangling"): reading which resources
//! `META-INF/encryption.xml` declares obfuscated, and undoing the two
//! standard schemes. Obfuscation is NOT encryption — both schemes XOR a
//! short prefix with a key derived from the publication's Unique
//! Identifier, and both are part of the open EPUB specs; genuinely
//! DRM-encrypted resources use other algorithms, which this module
//! reports as [`ObfuscationScheme::Unsupported`] so callers skip them.

use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::archive::read_resource;
use crate::href::resolve_href;
use crate::xml::attr_value;

/// The archive path of the encryption declaration, fixed by OCF.
const ENCRYPTION_XML: &str = "META-INF/encryption.xml";

/// Upper bound on retained `EncryptedData` entries. Real books obfuscate
/// a handful of fonts; a crafted declaration can list millions.
const MAX_ENCRYPTED_ENTRIES: usize = 1_000;

/// IDPF scheme: XOR this many leading bytes with the SHA-1 key.
const IDPF_PREFIX_LEN: usize = 1040;
/// Adobe scheme: XOR this many leading bytes with the UUID key.
const ADOBE_PREFIX_LEN: usize = 1024;

/// One algorithm named by `encryption.xml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObfuscationScheme {
    /// `http://www.idpf.org/2008/embedding` — XOR of the first 1040
    /// bytes with SHA-1 of the whitespace-stripped Unique Identifier.
    Idpf,
    /// `http://ns.adobe.com/pdf/enc#RC` — XOR of the first 1024 bytes
    /// with the 16-byte UUID digits of the Unique Identifier.
    Adobe,
    /// Anything else (real DRM among it): the resource cannot be read
    /// and must be skipped — never "passed through" as if plain.
    Unsupported,
}

/// One declared entry: which resource, which algorithm.
#[derive(Debug, Clone, PartialEq)]
pub struct ObfuscatedResource {
    /// Package-root-relative (= archive) path, normalized like every
    /// other href.
    pub href: String,
    pub scheme: ObfuscationScheme,
}

/// Reads the publication's obfuscation declarations. A publication with
/// no `META-INF/encryption.xml` — the overwhelmingly common case — is
/// simply empty; a malformed one degrades to the entries parsed before
/// the error, because a font we fail to deobfuscate is skipped later
/// anyway.
pub fn read_obfuscations(epub_path: &Path) -> Vec<ObfuscatedResource> {
    let Ok(bytes) = read_resource(epub_path, ENCRYPTION_XML) else {
        return Vec::new();
    };
    parse_encryption_xml(&String::from_utf8_lossy(&bytes))
}

/// The parser proper, split out for tests.
pub(crate) fn parse_encryption_xml(xml: &str) -> Vec<ObfuscatedResource> {
    let mut out = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    // The algorithm of the `EncryptedData` currently open; a
    // `CipherReference` pairs with the most recent one.
    let mut scheme: Option<ObfuscationScheme> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"EncryptedData" => scheme = None,
                b"EncryptionMethod" => {
                    scheme = attr_value(&e, b"Algorithm").map(|algorithm| {
                        match algorithm.as_str() {
                            "http://www.idpf.org/2008/embedding" => ObfuscationScheme::Idpf,
                            "http://ns.adobe.com/pdf/enc#RC" => ObfuscationScheme::Adobe,
                            _ => ObfuscationScheme::Unsupported,
                        }
                    });
                }
                b"CipherReference" => {
                    if let (Some(scheme), Some(uri)) = (scheme, attr_value(&e, b"URI")) {
                        if out.len() < MAX_ENCRYPTED_ENTRIES {
                            // URIs are container-root-relative, possibly
                            // percent-encoded; normalize like any href.
                            out.push(ObfuscatedResource {
                                href: resolve_href("", &uri),
                                scheme,
                            });
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Undoes one scheme in place over `bytes`. Returns `false` when no key
/// can be derived from `unique_identifier` (absent, or not a UUID for
/// the Adobe scheme) or the scheme is unsupported — the caller must then
/// skip the resource, because its bytes are still scrambled.
pub fn deobfuscate(
    bytes: &mut [u8],
    scheme: ObfuscationScheme,
    unique_identifier: Option<&str>,
) -> bool {
    let Some(identifier) = unique_identifier else {
        return false;
    };
    let (key, prefix_len) = match scheme {
        ObfuscationScheme::Idpf => (idpf_key(identifier), IDPF_PREFIX_LEN),
        ObfuscationScheme::Adobe => {
            let Some(key) = adobe_key(identifier) else {
                return false;
            };
            (key, ADOBE_PREFIX_LEN)
        }
        ObfuscationScheme::Unsupported => return false,
    };
    xor_prefix(bytes, &key, prefix_len);
    true
}

/// XOR is its own inverse, so obfuscating in tests reuses [`deobfuscate`].
fn xor_prefix(bytes: &mut [u8], key: &[u8], prefix_len: usize) {
    let end = prefix_len.min(bytes.len());
    for (at, byte) in bytes[..end].iter_mut().enumerate() {
        *byte ^= key[at % key.len()];
    }
}

/// The IDPF key: SHA-1 of the identifier with every whitespace character
/// removed (the OCF algorithm strips U+0020, U+0009, U+000D, U+000A).
fn idpf_key(identifier: &str) -> Vec<u8> {
    use sha1::{Digest, Sha1};
    let stripped: String = identifier
        .chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n'))
        .collect();
    Sha1::digest(stripped.as_bytes()).to_vec()
}

/// The Adobe key: the identifier stripped of its `urn:uuid:` prefix
/// (ASCII-case-insensitively — `URN:UUID:` is equally legal, RFC 8141
/// treats the scheme and NID as case-insensitive), hyphens, and colons
/// must leave exactly 32 hex digits — the UUID's 16 bytes. Anything
/// else yields no key.
fn adobe_key(identifier: &str) -> Option<Vec<u8>> {
    let trimmed = identifier.trim();
    let stripped = match trimmed.get(.."urn:uuid:".len()) {
        Some(prefix) if prefix.eq_ignore_ascii_case("urn:uuid:") => {
            &trimmed["urn:uuid:".len()..]
        }
        _ => trimmed,
    };
    let cleaned: String = stripped
        .chars()
        .filter(|c| !matches!(c, '-' | ':'))
        .collect();
    if cleaned.len() != 32 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = Vec::with_capacity(16);
    for at in (0..32).step_by(2) {
        // In-range by the hex check above.
        key.push(u8::from_str_radix(&cleaned[at..at + 2], 16).ok()?);
    }
    Some(key)
}

#[cfg(test)]
#[path = "obfuscation_tests.rs"]
mod tests;
