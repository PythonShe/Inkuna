// The UniFFI-generated bindings (Generated/InkunaCore.swift) are compiled
// directly into this target, so core types need no import.
import Foundation

/// Thin app-side wrapper around the Rust core's `Library`.
/// Owns the database location; all logic stays in the core.
final class LibraryStore: Sendable {
    static let shared = LibraryStore()

    let library: Bookshelf

    private init() {
        let directory = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        )[0]
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        do {
            // The core owns everything under this directory: inkuna.db,
            // books/, and covers/. A pre-existing inkuna.db from the old
            // dbPath constructor is adopted by the core's v2 migration.
            library = try Bookshelf.open(dataDir: directory.path)
        } catch {
            fatalError("cannot open library database: \(error)")
        }
    }
}
