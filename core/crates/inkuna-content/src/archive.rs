//! Archive helpers: every read out of the zip goes through here so the
//! per-entry decompression budgets are impossible to bypass.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::ContentError;

/// Per-entry decompression budget for the XML parts import cannot do
/// without: `container.xml`, the OPF, the nav doc, the NCX. A zip entry's
/// declared uncompressed size is attacker-controlled and the deflate
/// stream itself is unbounded, so the cap is enforced on the read:
/// without it a few-hundred-KB crafted entry inflates to gigabytes and
/// gets the app jetsam-killed on device. The budget bounds only the
/// bytes, NOT the transient: what the parsers *build* from those bytes —
/// manifest items, spine idrefs, TOC entries — runs to roughly 10x the
/// decompressed size, so each parser enforces its own item caps at the
/// push sites (`MAX_MANIFEST_ITEMS`, `MAX_SPINE_ITEMS`,
/// `MAX_TOC_ENTRIES`). A big book's OPF or NCX can legitimately run to
/// megabytes, which is why this byte budget stays generous.
pub(crate) const MAX_XML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Per-entry budget for spine content documents, far tighter than the
/// mandatory parts: several readers (layout worker, corpus extraction)
/// may hold decompressed chapters at once, and the aggregate corpus
/// budget only bounds what is *retained*, not the in-flight transient.
/// A whole large novel's text is a few MB, so 8 MiB for one chapter
/// keeps orders of magnitude of headroom over any honest content.
pub const MAX_SPINE_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
/// Aggregate budget for the text retained for one publication. The
/// per-entry decompression cap bounds a single resource, never the corpus:
/// a spine may reference resources without limit, and the sum is what the
/// device actually pays — every retained byte becomes a persistent
/// `resource_text` row. A very large real novel's full text is a few MB,
/// and even omnibus "complete works" editions stay under ~20 MB, so
/// 32 MiB keeps real headroom over any honest book. The previous 128 MiB
/// was ~25x a huge novel, and measured, it let an 18 KB crafted archive
/// import into a 364 MB data directory; a crafted spine now stops at a
/// quarter of that. Consumed by the import pipeline's corpus extraction
/// and by the TXT converter's decoded-text caps.
pub const MAX_TOTAL_TEXT_BYTES: usize = 32 * 1024 * 1024;
/// Same idea for binary resources — cover art and the engine's images —
/// which never legitimately approach it.
const MAX_RESOURCE_BYTES: u64 = 16 * 1024 * 1024;

/// Reads a mandatory XML part under [`MAX_XML_ENTRY_BYTES`].
pub(crate) fn read_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<String, ContentError> {
    read_entry_capped(archive, name, MAX_XML_ENTRY_BYTES)
}

/// Reads a spine content document under the much tighter
/// [`MAX_SPINE_ENTRY_BYTES`]. Callers degrade on the error (skip the
/// resource's text and keep importing) — a content document is optional
/// as far as import is concerned.
pub(crate) fn read_spine_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<String, ContentError> {
    read_entry_capped(archive, name, MAX_SPINE_ENTRY_BYTES)
}

fn read_entry_capped(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
    cap: u64,
) -> Result<String, ContentError> {
    let entry = open_entry(archive, name)?;
    let mut buf = Vec::new();
    entry.take(cap + 1).read_to_end(&mut buf)?;
    // Cap first, decode second. The capped prefix routinely ends inside a
    // multi-byte character — for CJK content that is the normal case, not
    // an exotic one — and that must surface as the cap rejection it is,
    // not as invalid UTF-8, because the shells route on the variant.
    check_cap(buf.len(), cap, name)?;
    String::from_utf8(buf)
        .map_err(|e| ContentError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

/// Reads one package-root-relative binary entry — cover art at import,
/// any resource (an image, a stylesheet) for the reader engine — under
/// [`MAX_RESOURCE_BYTES`]. The single enforcement point for bounded
/// binary reads: it does not decode, and an entry inflating past the
/// budget is a cap breach (`ContentError::InvalidPublication`, like
/// every other capped reader here), never materialized in full. Callers
/// degrade on the error where the resource is optional (import keeps
/// going without a cover).
///
/// Cost contract: each call opens the file and re-parses the zip central
/// directory. That is fine for the occasional read (a cover at import);
/// a hot path reading many resources — the engine's per-page asset loads
/// — must not loop over this, and the engine session owns its own
/// long-lived `ZipArchive` handle for exactly that reason.
pub fn read_resource(epub_path: &Path, href: &str) -> Result<Vec<u8>, ContentError> {
    let mut archive = zip::ZipArchive::new(File::open(epub_path)?)?;
    let entry = open_entry(&mut archive, href)?;
    let mut buf = Vec::new();
    entry.take(MAX_RESOURCE_BYTES + 1).read_to_end(&mut buf)?;
    check_cap(buf.len(), MAX_RESOURCE_BYTES, href)?;
    Ok(buf)
}

/// Opens one archive entry, preserving *why* it could not be opened.
/// Only a genuinely absent entry is an invalid publication; an unreadable
/// file (a truncated download, a device error mid-import) is an I/O
/// failure the shells must be able to tell apart from a broken book, and
/// an unsupported compression method or a corrupt central directory is an
/// archive-level fault. Collapsing all three into "missing {name}" made
/// every one of them read as a malformed EPUB.
fn open_entry<'a>(
    archive: &'a mut zip::ZipArchive<File>,
    name: &str,
) -> Result<zip::read::ZipFile<'a, File>, ContentError> {
    archive.by_name(name).map_err(|e| match e {
        zip::result::ZipError::FileNotFound => {
            ContentError::InvalidPublication(format!("missing {name}"))
        }
        zip::result::ZipError::Io(e) => ContentError::Io(e),
        other => ContentError::Archive(format!("cannot read {name}: {other}")),
    })
}

/// Every reader above takes `cap + 1` bytes: a read that stops exactly at
/// the cap is indistinguishable from a silently truncated entry, so the
/// extra byte is what makes the overflow detectable here.
fn check_cap(read: usize, cap: u64, name: &str) -> Result<(), ContentError> {
    if read as u64 > cap {
        return Err(ContentError::InvalidPublication(format!(
            "{name} exceeds the {cap}-byte decompression limit"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
