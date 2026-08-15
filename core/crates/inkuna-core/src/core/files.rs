//! Filesystem leaves shared by the import pipeline and the migrations that
//! adopt legacy rows into core-owned storage.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::CoreError;

/// Reads `src` once, hashing with BLAKE3 while writing the bytes to `dest`.
/// The destination is fsynced so the later rename lands durable content.
pub(crate) fn copy_and_hash(src: &Path, dest: &Path) -> Result<String, CoreError> {
    let mut reader = File::open(src)?;
    let mut writer = File::create(dest)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
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
