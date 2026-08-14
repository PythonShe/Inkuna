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
        let databaseURL = directory.appendingPathComponent("inkuna.db")
        do {
            library = try Bookshelf.open(dbPath: databaseURL.path)
        } catch {
            fatalError("cannot open library database: \(error)")
        }
    }
}
