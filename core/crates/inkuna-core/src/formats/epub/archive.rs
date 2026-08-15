//! Archive helpers: every read out of the zip goes through here so the
//! per-entry decompression budgets are impossible to bypass.

use std::fs::File;
use std::io::Read;

use crate::CoreError;

/// Per-entry decompression budget for XML/XHTML entries (container, OPF,
/// nav, NCX, spine resources). A zip entry's declared uncompressed size is
/// attacker-controlled and the deflate stream itself is unbounded, so the
/// cap is enforced on the read: without it a few-hundred-KB crafted entry
/// inflates to gigabytes and gets the app jetsam-killed on device.
pub(super) const MAX_XML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Same budget for cover art, which never legitimately approaches it.
const MAX_COVER_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn read_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
) -> Result<String, CoreError> {
    let entry = open_entry(archive, name)?;
    let mut buf = String::new();
    entry.take(MAX_XML_ENTRY_BYTES + 1).read_to_string(&mut buf)?;
    check_cap(buf.len(), MAX_XML_ENTRY_BYTES, name)?;
    Ok(buf)
}

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

fn open_entry<'a>(
    archive: &'a mut zip::ZipArchive<File>,
    name: &str,
) -> Result<zip::read::ZipFile<'a, File>, CoreError> {
    archive
        .by_name(name)
        .map_err(|_| CoreError::InvalidPublication(format!("missing {name}")))
}

/// Both readers above take `cap + 1` bytes: a read that stops exactly at
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
