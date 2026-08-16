//! Filesystem leaves shared by the import pipeline and the migrations that
//! adopt legacy rows into core-owned storage.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::CoreError;

/// The most one imported file may weigh, enforced on every byte that
/// reaches core-owned storage.
///
/// EPUB itself specifies no ceiling, so this is a judgement about real
/// books rather than a spec value: an ordinary novel is under a megabyte,
/// an illustrated or manga volume tens to a few hundred, and the heaviest
/// publications anyone actually ships stay well under a gigabyte. 2 GiB is
/// an order of magnitude past that — no legitimate book is turned away —
/// while still closing the case this exists for: a stream whose length
/// nobody can know in advance (a SAF provider, a pipe) handing out bytes
/// forever and filling the device before anyone notices. 2 GiB is also the
/// last size that fits a signed 32-bit offset, which keeps the zip reader
/// and every seek below it clear of their own limits.
pub(crate) const MAX_IMPORT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Reads `src` once, hashing with BLAKE3 while writing the bytes to `dest`.
/// The destination is fsynced so the later rename lands durable content.
/// Fails with [`CoreError::FileTooLarge`] past [`MAX_IMPORT_BYTES`].
pub(crate) fn copy_and_hash(src: &Path, dest: &Path) -> Result<String, CoreError> {
    stream_and_hash(&mut File::open(src)?, dest)
}

/// [`copy_and_hash`] over an already-open stream — a shell-owned file
/// descriptor — which need not be seekable: one sequential pass is all
/// that happens here. The ceiling matters most on this route: a stream
/// carries no length, so the size a provider *claims* is not something to
/// trust.
pub(crate) fn stream_and_hash(reader: &mut dyn Read, dest: &Path) -> Result<String, CoreError> {
    stream_and_hash_capped(reader, dest, MAX_IMPORT_BYTES)
}

/// The body of [`stream_and_hash`] with the ceiling as a parameter. Every
/// caller outside tests passes [`MAX_IMPORT_BYTES`]; tests inject a small
/// ceiling so the refusal path is reachable without moving gigabytes.
///
/// The count is checked *before* each block is written, so what lands on
/// disk never exceeds the ceiling and an endless stream costs one buffer
/// past it. The partial `dest` is left in place: sweeping it belongs to
/// the caller that chose the path, which is what the import pipeline
/// already does for every other failure here.
fn stream_and_hash_capped(
    reader: &mut dyn Read,
    dest: &Path,
    max_bytes: u64,
) -> Result<String, CoreError> {
    let mut writer = File::create(dest)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 128 * 1024];
    let mut written: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > max_bytes {
            return Err(CoreError::FileTooLarge(max_bytes));
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
    }
    writer.sync_all()?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Flushes a directory's own entries to disk. Fsyncing a file only makes
/// its *contents* durable; the name pointing at them lives in the parent
/// directory and stays in cache until that directory is fsynced too, so a
/// rename is not durable without this.
pub(crate) fn sync_dir(dir: &Path) -> Result<(), CoreError> {
    File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
