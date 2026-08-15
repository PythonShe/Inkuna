//! Archive helpers: every read out of the zip goes through here so the
//! per-entry decompression budgets are impossible to bypass.

use std::fs::File;
use std::io::Read;

use crate::CoreError;

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
pub(super) const MAX_XML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Per-entry budget for spine content documents, which are far tighter
/// than the mandatory parts because they are read *concurrently*: the
/// transient peak is `rayon threads × this`, so the 64 MiB above would
/// put a 6-core phone around 384 MB — inside jetsam range — no matter
/// what aggregate budget the corpus keeps, since many reads are already
/// in flight when it trips. A whole large novel's text is a few MB, so
/// 8 MiB for one chapter keeps orders of magnitude of headroom over any
/// honest content while bounding the peak to ~48 MB.
pub(super) const MAX_SPINE_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
/// Same idea for cover art, which never legitimately approaches it.
const MAX_COVER_BYTES: u64 = 16 * 1024 * 1024;

/// Reads a mandatory XML part under [`MAX_XML_ENTRY_BYTES`].
pub(super) fn read_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<String, CoreError> {
    read_entry_capped(archive, name, MAX_XML_ENTRY_BYTES)
}

/// Reads a spine content document under the much tighter
/// [`MAX_SPINE_ENTRY_BYTES`]. Callers degrade on the error (skip the
/// resource's text and keep importing) — a content document is optional
/// as far as import is concerned.
pub(super) fn read_spine_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<String, CoreError> {
    read_entry_capped(archive, name, MAX_SPINE_ENTRY_BYTES)
}

fn read_entry_capped(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
    cap: u64,
) -> Result<String, CoreError> {
    let entry = open_entry(archive, name)?;
    let mut buf = Vec::new();
    entry.take(cap + 1).read_to_end(&mut buf)?;
    // Cap first, decode second. The capped prefix routinely ends inside a
    // multi-byte character — for CJK content that is the normal case, not
    // an exotic one — and that must surface as the cap rejection it is,
    // not as invalid UTF-8, because the shells route on the variant.
    check_cap(buf.len(), cap, name)?;
    String::from_utf8(buf)
        .map_err(|e| CoreError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

/// Reads a binary entry — cover art, the only one today — under
/// [`MAX_COVER_BYTES`]. Unlike the text readers this one does not decode:
/// the bytes are written to `covers/` exactly as the archive stored them.
/// Callers degrade on the error (import keeps going without a cover).
pub(super) fn read_entry_bytes(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<Vec<u8>, CoreError> {
    let entry = open_entry(archive, name)?;
    let mut buf = Vec::new();
    entry.take(MAX_COVER_BYTES + 1).read_to_end(&mut buf)?;
    check_cap(buf.len(), MAX_COVER_BYTES, name)?;
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
) -> Result<zip::read::ZipFile<'a, File>, CoreError> {
    archive.by_name(name).map_err(|e| match e {
        zip::result::ZipError::FileNotFound => {
            CoreError::InvalidPublication(format!("missing {name}"))
        }
        zip::result::ZipError::Io(e) => CoreError::Io(e),
        other => CoreError::Archive(format!("cannot read {name}: {other}")),
    })
}

/// Every reader above takes `cap + 1` bytes: a read that stops exactly at
/// the cap is indistinguishable from a silently truncated entry, so the
/// extra byte is what makes the overflow detectable here.
fn check_cap(read: usize, cap: u64, name: &str) -> Result<(), CoreError> {
    if read as u64 > cap {
        return Err(CoreError::InvalidPublication(format!(
            "{name} exceeds the {cap}-byte decompression limit"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
