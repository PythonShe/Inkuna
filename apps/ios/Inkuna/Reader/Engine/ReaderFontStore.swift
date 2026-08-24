import CoreText
import Foundation
import OSLog

/// Rebuilds the engine's font registry as Core Text descriptors and fonts.
///
/// The store is process-global but its cache is bound to the session it was
/// primed for: publisher ids sit in a per-session block after the shared
/// base blocks, so id 60 in one book's registry can be a different face in
/// another's — a still-mounted canvas for the previous book must resolve
/// nothing here, never the next book's face under a colliding id.
@MainActor
final class ReaderFontStore {
    static let shared: ReaderFontStore = ReaderFontStore()

    private struct FontKey: Hashable {
        let id: UInt32
        let size: CGFloat
    }

    /// One build's complete output, carried back from the detached build
    /// task. `CTFontDescriptor` is immutable, so moving references across
    /// isolation is safe.
    private struct Build: @unchecked Sendable {
        var descriptors: [UInt32: CTFontDescriptor] = [:]
        var byEntry: [FontEntry: CTFontDescriptor] = [:]
        var unavailable: [(entry: FontEntry, reason: String)] = []
        var malformedAxisTags: [(tag: String, entry: FontEntry)] = []
    }

    /// Immutable descriptor map handed into a build for cross-prime reuse.
    private struct ReusableDescriptors: @unchecked Sendable {
        let byEntry: [FontEntry: CTFontDescriptor]
    }

    private static let logger = Logger(subsystem: "app.inkuna.ios", category: "reader")
    private weak var owner: AnyObject?
    private var registry: [FontEntry] = []
    private var descriptors: [UInt32: CTFontDescriptor] = [:]
    private var descriptorsByEntry: [FontEntry: CTFontDescriptor] = [:]
    private var fonts: [FontKey: CTFont] = [:]
    private var loggedUnavailableEntries: Set<FontEntry> = []
    private var loggedMalformedAxisTags: Set<String> = []
    private var primeChain: Task<Void, Never>?

    private init() {}

    /// Replaces the whole registry at once and binds it to the session it
    /// was built for. The build is blocking disk I/O — one
    /// `CTFontManagerCreateFontDescriptorsFromURL` per distinct file, over
    /// ~40 MB CJK `.ttc` collections — so it runs off the main actor and
    /// must never sit inside the open-to-first-page budget synchronously;
    /// `openReader` awaits it before installing the canvas, which keeps the
    /// guarantee that no page draws before priming completes. An unchanged
    /// registry short-circuits the rebuild (re-keying only), and entries
    /// shared with the previous registry (the bundled + system blocks are
    /// stable across books) reuse their resolved descriptors without
    /// touching disk again.
    func prime(_ registry: [FontEntry], owner: AnyObject) async {
        let previous = primeChain
        let next = Task { @MainActor [weak self] in
            await previous?.value
            await self?.performPrime(registry, owner: owner)
        }
        primeChain = next
        await next.value
    }

    /// A face by display-list id, only for the session the map was built
    /// for — a stale canvas of the previous book resolves nothing.
    func font(id: UInt32, size: CGFloat, owner: AnyObject?) -> CTFont? {
        guard let owner, owner === self.owner else { return nil }

        let key = FontKey(id: id, size: size)
        if let cached = fonts[key] {
            return cached
        }

        guard let descriptor = descriptors[id] else {
            return nil
        }

        let font = CTFontCreateWithFontDescriptor(descriptor, size, nil)
        fonts[key] = font
        return font
    }

    private func performPrime(_ registry: [FontEntry], owner: AnyObject) async {
        if registry == self.registry {
            // Identical registry: re-key to the new session without
            // rebuilding (same-book reopen, or two books sharing every
            // block).
            self.owner = owner
            return
        }

        let reusable = ReusableDescriptors(byEntry: descriptorsByEntry)
        let built = await Task.detached(priority: .userInitiated) {
            Self.build(registry, reusing: reusable)
        }.value

        self.registry = registry
        descriptors = built.descriptors
        descriptorsByEntry = built.byEntry
        fonts.removeAll(keepingCapacity: true)
        self.owner = owner
        for failure in built.unavailable {
            markUnavailable(failure.entry, reason: failure.reason)
        }
        for malformed in built.malformedAxisTags {
            markMalformedAxisTag(malformed.tag, for: malformed.entry)
        }
    }

    private nonisolated static func build(
        _ registry: [FontEntry],
        reusing reusable: ReusableDescriptors
    ) -> Build {
        var build = Build()
        // 57+ registry entries resolve from ~13 distinct files: load each
        // file's descriptor collection once per build, with a PostScript
        // name index so per-entry lookup does not rescan the collection.
        var filesByPath: [String: (descriptors: [CTFontDescriptor], byName: [String: CTFontDescriptor])] = [:]

        for entry in registry {
            // An entry unchanged since the previous registry keeps its
            // resolved descriptor; only genuinely new entries (the
            // per-book publisher block) touch disk.
            if let reused = reusable.byEntry[entry] {
                build.byEntry[entry] = reused
                build.descriptors[entry.id] = reused
                continue
            }

            let file: (descriptors: [CTFontDescriptor], byName: [String: CTFontDescriptor])
            if let loaded = filesByPath[entry.filePath] {
                file = loaded
            } else {
                guard FileManager.default.fileExists(atPath: entry.filePath) else {
                    build.unavailable.append((entry, "font file is missing"))
                    continue
                }
                guard let availableDescriptors = CTFontManagerCreateFontDescriptorsFromURL(
                    URL(fileURLWithPath: entry.filePath) as CFURL
                ) as? [CTFontDescriptor] else {
                    build.unavailable.append((entry, "font descriptor is unavailable"))
                    continue
                }
                var byName: [String: CTFontDescriptor] = [:]
                for candidate in availableDescriptors {
                    if let name = CTFontDescriptorCopyAttribute(candidate, kCTFontNameAttribute) as? String {
                        // First face wins on a duplicate name, matching
                        // the previous first-match scan.
                        if byName[name] == nil { byName[name] = candidate }
                    }
                }
                file = (availableDescriptors, byName)
                filesByPath[entry.filePath] = file
            }

            guard var descriptor = descriptor(for: entry, in: file, build: &build) else {
                continue
            }

            var variations: [NSNumber: NSNumber] = [:]
            for axis in entry.axes {
                guard let tag = fourCharCode(for: axis.tag) else {
                    build.malformedAxisTags.append((axis.tag, entry))
                    continue
                }
                variations[NSNumber(value: tag)] = NSNumber(value: axis.value)
            }

            if !variations.isEmpty {
                let attributes: [CFString: Any] = [kCTFontVariationAttribute: variations]
                descriptor = CTFontDescriptorCreateCopyWithAttributes(descriptor, attributes as CFDictionary)
            }

            build.byEntry[entry] = descriptor
            build.descriptors[entry.id] = descriptor
        }

        return build
    }

    private nonisolated static func descriptor(
        for entry: FontEntry,
        in file: (descriptors: [CTFontDescriptor], byName: [String: CTFontDescriptor]),
        build: inout Build
    ) -> CTFontDescriptor? {
        // The registry names every face, and the name is authoritative:
        // Core Text's descriptor order for a collection (or a variable
        // font's named instances) is not guaranteed to match the file's
        // collection order, so a system or publisher .ttc face is found by
        // its PostScript name first and only falls back to the index.
        if let match = file.byName[entry.postScriptName] {
            return match
        }

        guard
            let index = Int(exactly: entry.collectionIndex),
            file.descriptors.indices.contains(index)
        else {
            build.unavailable.append((entry, "font descriptor is unavailable"))
            return nil
        }

        let descriptor = file.descriptors[index]
        let fileName = URL(fileURLWithPath: entry.filePath).deletingPathExtension().lastPathComponent
        guard fileName.hasPrefix("NotoSerifCJK-") || fileName.hasPrefix("NotoSansCJK-") else {
            return descriptor
        }

        guard let expectedName = expectedCJKPostScriptName(for: entry) else {
            build.unavailable.append((entry, "TTC face identity is unverifiable"))
            return nil
        }

        let resolvedName = CTFontDescriptorCopyAttribute(descriptor, kCTFontNameAttribute) as? String
        guard resolvedName == expectedName else {
            build.unavailable.append((
                entry,
                "TTC face index \(entry.collectionIndex) resolved \(resolvedName ?? "<missing>") instead of \(expectedName)"
            ))
            return nil
        }

        return descriptor
    }

    private nonisolated static func expectedCJKPostScriptName(for entry: FontEntry) -> String? {
        let fileName = URL(fileURLWithPath: entry.filePath).deletingPathExtension().lastPathComponent
        guard let separator = fileName.lastIndex(of: "-") else { return nil }

        let family = String(fileName[..<separator])
        let style = String(fileName[fileName.index(after: separator)...])
        guard family == "NotoSerifCJK" || family == "NotoSansCJK" else { return nil }

        let regions = ["jp", "kr", "sc", "tc"]
        guard
            let index = Int(exactly: entry.collectionIndex),
            regions.indices.contains(index)
        else {
            return nil
        }

        return "\(family)\(regions[index])-\(style)"
    }

    private func markUnavailable(_ entry: FontEntry, reason: String) {
        guard loggedUnavailableEntries.insert(entry).inserted else { return }
        Self.logger.error("Unable to load reader font \(entry.id, privacy: .public): \(reason, privacy: .public)")
    }

    private func markMalformedAxisTag(_ tag: String, for entry: FontEntry) {
        guard loggedMalformedAxisTags.insert(tag).inserted else { return }
        Self.logger.error("Ignoring malformed variation-axis tag \(tag, privacy: .public) for reader font \(entry.id, privacy: .public)")
    }

    private nonisolated static func fourCharCode(for tag: String) -> UInt32? {
        let bytes = Array(tag.utf8)
        guard bytes.count == 4, bytes.allSatisfy({ $0 < 128 }) else { return nil }

        return bytes.reduce(0) { ($0 << 8) | UInt32($1) }
    }
}
