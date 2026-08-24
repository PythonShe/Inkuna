//! Publisher font extraction: at session open, the publication's
//! embedded faces are read out of the archive, deobfuscated
//! (`META-INF/encryption.xml`), decompressed (WOFF/WOFF2 → sfnt via the
//! pure-Rust `wuff` decoder), validated, and written to a per-book
//! cache dir the registry can memory-map — content-hash-named, so a
//! re-open reuses the extracted files byte-for-byte and a changed book
//! never collides with a stale cache entry.
//!
//! Face discovery is deterministic, a function of the publication
//! alone: `@font-face` rules from the manifest's CSS resources in
//! manifest × source order first (declared family/style/weight), then
//! manifest items with font media types no rule referenced (family and
//! slant/weight introspected from the font's own tables). Every failure
//! — unreadable, undeobfuscatable, unparseable — skips that face with a
//! warning log; extraction never fails a session open.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use inkuna_content::{
    deobfuscate, read_obfuscations, resolve_relative, split_fragment, EpubPackage,
    ObfuscationScheme, ResourceReader,
};
use read_fonts::types::NameId;
use read_fonts::{FontRef, TableProvider};

use crate::style::{parse_sheet, FontStyle};

use super::face::name_value;
use super::publisher::PublisherFaceSpec;

/// Upper bound on registered publisher faces per publication. Real
/// books embed a handful; a crafted manifest can declare thousands,
/// each costing an archive read and an mmap.
const MAX_PUBLISHER_FACES: usize = 32;

/// Aggregate budget for extracted (post-decompression) font bytes per
/// publication — WOFF2 can inflate well past its archive size, so the
/// per-resource read cap alone does not bound the cache dir.
const MAX_TOTAL_FONT_BYTES: usize = 64 * 1024 * 1024;

/// Per-face cap enforced BEFORE decompression, on both the compressed
/// container size and the WOFF header's declared `totalSfntSize` —
/// WOFF2 allows ~100× expansion, so a small archive entry could
/// otherwise allocate gigabytes before the post-decompression budget
/// ever sees it. Real embedded faces are a few MiB at most.
const MAX_FACE_BYTES: usize = 32 * 1024 * 1024;

/// Discovers, deobfuscates, decompresses, validates, and extracts the
/// publication's embedded faces into `cache_dir`, returning the specs
/// to register. Empty when the book embeds nothing usable.
pub fn extract_publisher_fonts(
    epub_path: &Path,
    package: &EpubPackage,
    cache_dir: &Path,
) -> Vec<PublisherFaceSpec> {
    let mut reader = match ResourceReader::open(epub_path) {
        Ok(reader) => reader,
        Err(e) => {
            log::warn!("publisher fonts: {} unreadable ({e})", epub_path.display());
            return Vec::new();
        }
    };
    let obfuscations = read_obfuscations(epub_path);
    let unique_identifier = package.metadata.unique_identifier.as_deref();
    // A crash between tmp write and rename in an earlier session leaves
    // `<hash>.tmp` behind forever otherwise (the orphan sweep removes
    // whole dirs, not stray files, and the budget never counts them).
    // Safe here: the FFI holds one live session per book, so nothing is
    // concurrently mid-write in this book's cache dir.
    sweep_stale_tmp(cache_dir);

    let mut extractor = Extractor {
        reader: &mut reader,
        obfuscations,
        unique_identifier,
        cache_dir,
        extracted: Vec::new(),
        total_bytes: 0,
    };

    let mut specs: Vec<PublisherFaceSpec> = Vec::new();
    let mut rule_hrefs: HashSet<String> = HashSet::new();

    // Pass 1: @font-face rules from the manifest's stylesheets, in
    // manifest × rule × source order. The first loadable source wins.
    for item in &package.manifest {
        let is_css = item
            .media_type
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("text/css"));
        if !is_css || specs.len() >= MAX_PUBLISHER_FACES {
            continue;
        }
        let css = match extractor.reader.read(&item.href) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                log::warn!("publisher fonts: stylesheet {} skipped ({e})", item.href);
                continue;
            }
        };
        for rule in parse_sheet(&css).font_faces() {
            if specs.len() >= MAX_PUBLISHER_FACES {
                break;
            }
            for source in &rule.sources {
                let href = resolve_source(&item.href, source);
                if let Some(file_path) = extractor.extract(&href) {
                    // EVERY href of the consumed rule is claimed — the
                    // losing alternates (`url(x.woff2), url(x.ttf)`)
                    // must not resurface in pass 2 as duplicate faces
                    // burning the face cap.
                    for claimed in &rule.sources {
                        rule_hrefs.insert(resolve_source(&item.href, claimed));
                    }
                    specs.push(PublisherFaceSpec {
                        file_path,
                        family: rule.family.clone(),
                        italic: rule.style == FontStyle::Italic,
                        weight: rule.weight,
                        unicode_ranges: rule.unicode_ranges.clone(),
                    });
                    break;
                }
            }
        }
    }

    // Pass 2: manifest font items no rule referenced — registered under
    // the font's own family name so name-matching stacks still find
    // them.
    for item in &package.manifest {
        if !item.is_font() || rule_hrefs.contains(&item.href) {
            continue;
        }
        if specs.len() >= MAX_PUBLISHER_FACES {
            log::warn!(
                "publisher fonts: face cap ({MAX_PUBLISHER_FACES}) reached; remaining fonts skipped"
            );
            break;
        }
        let Some(file_path) = extractor.extract(&item.href) else {
            continue;
        };
        match introspect(&file_path) {
            Some((family, italic, weight)) => specs.push(PublisherFaceSpec {
                file_path,
                family,
                italic,
                weight: (weight, weight),
                // Manifest-only faces declare no unicode-range: they
                // claim every codepoint their cmap covers.
                unicode_ranges: None,
            }),
            None => log::warn!(
                "publisher fonts: {} has no usable family name; skipped",
                item.href
            ),
        }
    }
    specs
}

/// A `src` url resolved against its declaring stylesheet, with any
/// fragment or query cut (`font.woff2#iefix`, `font.ttf?v=2`).
fn resolve_source(css_href: &str, source: &str) -> String {
    let (no_fragment, _) = split_fragment(source);
    let trimmed = no_fragment.split('?').next().unwrap_or(no_fragment);
    resolve_relative(css_href, trimmed)
}

/// The extraction worker: reads, deobfuscates, decompresses, validates,
/// and writes one face per distinct href, remembering outcomes so a
/// href referenced twice costs one pass and one cache file.
struct Extractor<'a> {
    reader: &'a mut ResourceReader,
    obfuscations: Vec<inkuna_content::ObfuscatedResource>,
    unique_identifier: Option<&'a str>,
    cache_dir: &'a Path,
    /// Per-href outcome memo: `None` = tried and failed.
    extracted: Vec<(String, Option<PathBuf>)>,
    total_bytes: usize,
}

impl Extractor<'_> {
    /// The extracted file for an archive href, or `None` (memoized) when
    /// the resource cannot become a usable font.
    fn extract(&mut self, href: &str) -> Option<PathBuf> {
        if let Some((_, outcome)) = self.extracted.iter().find(|(h, _)| h == href) {
            return outcome.clone();
        }
        let outcome = self.extract_new(href);
        self.extracted.push((href.to_string(), outcome.clone()));
        outcome
    }

    fn extract_new(&mut self, href: &str) -> Option<PathBuf> {
        let mut bytes = match self.reader.read(href) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("publisher fonts: {href} unreadable ({e})");
                return None;
            }
        };
        // Deobfuscate first: obfuscation is applied to the stored bytes,
        // whatever container they hold.
        if let Some(entry) = self.obfuscations.iter().find(|o| o.href == href) {
            if !deobfuscate(&mut bytes, entry.scheme, self.unique_identifier) {
                let why = match entry.scheme {
                    ObfuscationScheme::Unsupported => "unsupported encryption algorithm",
                    _ => "no usable unique identifier for the deobfuscation key",
                };
                log::warn!("publisher fonts: {href} skipped ({why})");
                return None;
            }
        }
        // WOFF containers decompress to sfnt; raw sfnt passes through.
        // Both bounds are checked BEFORE wuff allocates anything: the
        // compressed container size and the header's declared
        // `totalSfntSize` (offset 16, big-endian, in both WOFF formats)
        // — the post-decompression budget stays the final arbiter.
        let bytes = match &bytes[..bytes.len().min(4)] {
            woff @ (b"wOFF" | b"wOF2") => {
                if bytes.len() > MAX_FACE_BYTES {
                    log::warn!(
                        "publisher fonts: {href} skipped (compressed font exceeds the \
                         per-face cap of {MAX_FACE_BYTES} bytes)"
                    );
                    return None;
                }
                let declared = bytes
                    .get(16..20)
                    .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
                match declared {
                    Some(size) if size <= MAX_FACE_BYTES => {}
                    Some(_) => {
                        log::warn!(
                            "publisher fonts: {href} skipped (declared sfnt size exceeds \
                             the per-face cap of {MAX_FACE_BYTES} bytes)"
                        );
                        return None;
                    }
                    None => {
                        log::warn!("publisher fonts: {href} skipped (truncated WOFF header)");
                        return None;
                    }
                }
                let decompressed = if woff == b"wOFF" {
                    wuff::decompress_woff1(&bytes)
                } else {
                    wuff::decompress_woff2(&bytes)
                };
                match decompressed {
                    Ok(sfnt) => sfnt,
                    Err(e) => {
                        log::warn!("publisher fonts: {href} failed WOFF decompression ({e:?})");
                        return None;
                    }
                }
            }
            _ => bytes,
        };
        // Validate BEFORE writing: only parseable faces enter the cache.
        if let Err(e) = validate_font(&bytes) {
            log::warn!("publisher fonts: {href} is not a usable font ({e})");
            return None;
        }
        if self.total_bytes.saturating_add(bytes.len()) > MAX_TOTAL_FONT_BYTES {
            log::warn!(
                "publisher fonts: {href} skipped (per-book font budget of \
                 {MAX_TOTAL_FONT_BYTES} bytes exhausted)"
            );
            return None;
        }
        self.total_bytes += bytes.len();

        let extension = match bytes.get(..4) {
            Some(b"OTTO") => "otf",
            Some(b"ttcf") => "ttc",
            _ => "ttf",
        };
        let hash = blake3::hash(&bytes);
        let name = format!("{}.{extension}", &hash.to_hex()[..32]);
        let path = self.cache_dir.join(name);
        // Content-hash naming makes reuse safe: an existing file with
        // this name holds these bytes (or a corrupt one, replaced below
        // on a size mismatch).
        if path
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() == bytes.len() as u64)
        {
            return Some(path);
        }
        if let Err(e) = write_atomic(self.cache_dir, &path, &bytes) {
            log::warn!("publisher fonts: {href} could not be cached ({e})");
            return None;
        }
        Some(path)
    }
}

/// Parses the (possibly collection) bytes and checks the metrics the
/// registry will need, so registration cannot fail after extraction.
fn validate_font(bytes: &[u8]) -> Result<(), String> {
    let font = FontRef::from_index(bytes, 0).map_err(|e| e.to_string())?;
    let upem = font.head().map_err(|e| e.to_string())?.units_per_em();
    if upem == 0 {
        return Err("units_per_em is 0".to_string());
    }
    font.hhea().map_err(|e| e.to_string())?;
    Ok(())
}

/// Removes stale `*.tmp` files a crashed earlier extraction left in
/// the book's cache dir. Missing dir or unremovable files are fine —
/// extraction proceeds regardless.
fn sweep_stale_tmp(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "tmp") {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Write-then-rename so a concurrent open never maps a half-written
/// file (the registry mmaps these).
fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// The registration identity of a manifest-only face, from its own
/// tables: typographic family name (id 16) falling back to the family
/// name (id 1); OS/2 slant and weight class with sane defaults.
fn introspect(path: &Path) -> Option<(String, bool, u16)> {
    let bytes = std::fs::read(path).ok()?;
    let font = FontRef::from_index(&bytes, 0).ok()?;
    let family = name_value(&font, NameId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name_value(&font, NameId::FAMILY_NAME))?;
    let (italic, weight) = match font.os2() {
        Ok(os2) => (
            os2.fs_selection()
                .contains(read_fonts::tables::os2::SelectionFlags::ITALIC),
            os2.us_weight_class().clamp(1, 1000),
        ),
        Err(_) => (false, 400),
    };
    Some((family, italic, weight))
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
