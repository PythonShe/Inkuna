// The UniFFI-generated bindings (Generated/InkunaCore.swift) are compiled
// directly into this target, so core types need no import.
import Foundation
import os

/// Thin app-side wrapper around the Rust core's `Library`.
/// Owns the database location; all logic stays in the core.
///
/// An actor, because opening the library is not cheap: the core runs its
/// schema migrations and then sweeps files no row references, both
/// synchronously. Isolating them here keeps that work off the main thread
/// no matter how early a screen asks for the library. The core also
/// requires exactly one `Bookshelf` per data directory for the process
/// lifetime — a second one would sweep the first one's in-flight import —
/// which the cached instance is what enforces.
actor LibraryStore {
    static let shared = LibraryStore()

    private var opened: Bookshelf?

    /// The background cover-normalization pass, held on the actor so it
    /// has a defined cancellation path and a second open can never queue
    /// a second pass.
    private var coverOptimization: Task<Void, Never>?

    private let logger = Logger(subsystem: "app.inkuna.ios", category: "library")

    /// The core library, opened on first use.
    ///
    /// Opening can fail — a database damaged by a device-full write, a
    /// container the app cannot create — and that is thrown to the caller
    /// rather than trapped: a reader whose library will not open needs a
    /// screen they can retry or reset from, not an app that cannot launch.
    /// Nothing is cached on failure, so a later call retries.
    func library() throws -> Bookshelf {
        if let opened {
            return opened
        }
        guard let directory = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first else {
            throw LibraryStoreError.noApplicationSupportDirectory
        }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        // The core owns everything under this directory: inkuna.db,
        // books/, and covers/. A pre-existing inkuna.db from the old
        // dbPath constructor is adopted by the core's v2 migration.
        // fontDir is a placeholder until the reader engine's bundled fonts
        // land (plan 02 wires the real assets/fonts/ bundle directory).
        let bookshelf = try Bookshelf.open(dataDir: directory.path, fontDir: Bundle.main.bundlePath)
        opened = bookshelf
        // Covers imported by older cores are full-resolution originals;
        // normalize them into the core's bounded WebP form off the
        // critical path. Idempotent and cheap when there is nothing to
        // do; failing only means covers stay big until the next open —
        // but a failure is still worth a trace in the log.
        coverOptimization = Task(priority: .utility) { [logger] in
            do {
                _ = try await bookshelf.importer().optimizeCovers()
            } catch {
                logger.warning("Cover optimization failed: \(error)")
            }
        }
        return bookshelf
    }
}

/// Failures that belong to the shell's side of opening the library; the
/// core's own failures arrive as `InkunaError`.
enum LibraryStoreError: Error {
    /// The system reported no Application Support directory for the app.
    case noApplicationSupportDirectory
}
