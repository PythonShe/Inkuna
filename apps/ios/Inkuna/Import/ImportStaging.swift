import Foundation

/// Copies picked files into a private staging directory before the core
/// ever sees them.
///
/// Why stage at all, when the core copies the file into its own `books/`
/// directory anyway:
///
/// - **Sandbox extensions are the shell's problem, not the core's.** A URL
///   from the document picker or from "Open with Inkuna" is readable only
///   between `startAccessingSecurityScopedResource()` and its stop, and
///   only by *this* process. Holding that extension open across a long
///   async import — while the user may background the app — is how imports
///   silently fail on device. Staging shrinks the window to one copy.
/// - **Cloud files are not files yet.** An iCloud Drive or Dropbox item can
///   be a placeholder until something coordinates a read of it.
///   `NSFileCoordinator` is what materializes it; the core's plain
///   `File::open` is not.
/// - **Providers revoke.** The picker's URL is valid for the picker's
///   lifetime, not ours.
///
/// The copy is cheap where it matters: on the same APFS volume
/// `copyItem(at:to:)` clones rather than duplicates blocks.
///
/// Nothing here touches the main thread; call it from a detached task.
enum ImportStaging {
    /// Staging root, under Caches: these files are reconstructible and the
    /// system may reclaim them, which is exactly right for a copy that is
    /// deleted the moment the core has taken its own.
    static var root: URL {
        URL.cachesDirectory.appending(path: "ImportStaging", directoryHint: .isDirectory)
    }

    /// Copies `url` into a fresh staging directory and returns the copy.
    ///
    /// - Throws: whatever coordination or the copy itself failed with; the
    ///   partial staging directory is removed first.
    static func stage(_ url: URL) throws -> URL {
        // `startAccessingSecurityScopedResource()` returns **false** for
        // URLs that need no extension — anything already inside our own
        // container, including the `Documents/Inbox` copy iOS makes for
        // "Open with Inkuna", and the temp copy an `asCopy: true` picker
        // hands back. That false is not a failure, and bailing on it is the
        // classic silent-import bug. Only a `true` is balanced with a stop,
        // because the call is reference counted and an unbalanced stop
        // tears down an extension some other part of the app still holds.
        let scoped = url.startAccessingSecurityScopedResource()
        defer {
            if scoped { url.stopAccessingSecurityScopedResource() }
        }

        let directory = root.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let destination = directory.appending(path: stagedName(for: url), directoryHint: .notDirectory)

        var copyError: Error?
        var coordinationError: NSError?
        // A coordinated read is what pulls a non-materialized cloud item
        // down before we read it; `.withoutChanges` because we only read.
        NSFileCoordinator().coordinate(
            readingItemAt: url,
            options: [.withoutChanges],
            error: &coordinationError
        ) { readable in
            do {
                try FileManager.default.copyItem(at: readable, to: destination)
            } catch {
                copyError = error
            }
        }
        if let error = coordinationError ?? copyError {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
        return destination
    }

    /// Deletes one staged file and the directory that held it. Called as
    /// soon as the core reports on that file — the core owns its own copy
    /// by then, whether the import succeeded or not.
    static func discard(_ staged: URL) {
        let directory = staged.deletingLastPathComponent()
        // Only ever inside our own staging root, never a user file.
        guard directory.deletingLastPathComponent().standardizedFileURL == root.standardizedFileURL else {
            return
        }
        try? FileManager.default.removeItem(at: directory)
    }

    /// Clears anything a crashed or killed run left behind. Safe only when
    /// no run is in flight, so `ImportService` calls it as a run begins.
    static func sweep() {
        try? FileManager.default.removeItem(at: root)
    }

    /// The name the staged copy carries.
    ///
    /// It matters: when an EPUB's metadata has no title the core falls back
    /// to the file stem, so this name can end up as the book's title. Kept
    /// byte-for-byte apart from Unicode normalization — file providers hand
    /// back decomposed forms on some volumes, and the core normalizes its
    /// fallback title to NFC, so the staged name is normalized the same way
    /// to keep it, the display name, and the eventual title in agreement.
    private static func stagedName(for url: URL) -> String {
        let name = url.lastPathComponent.precomposedStringWithCanonicalMapping
        // `lastPathComponent` cannot contain a separator, but it can be
        // empty or a relative marker for a malformed URL.
        guard !name.isEmpty, name != ".", name != ".." else { return "Book.epub" }
        return name
    }
}

extension URL {
    /// The file's name as the user knows it from Files, for status lines
    /// and failure messages. Normalized like the staged copy so the two
    /// never disagree for a CJK name.
    var importDisplayName: String {
        let name = lastPathComponent.precomposedStringWithCanonicalMapping
        return name.isEmpty ? path : name
    }
}
