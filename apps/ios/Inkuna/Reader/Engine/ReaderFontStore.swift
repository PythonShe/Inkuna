import CoreText
import Foundation
import OSLog

/// Rebuilds the engine's font registry as Core Text descriptors and fonts.
@MainActor
final class ReaderFontStore {
    static let shared: ReaderFontStore = ReaderFontStore()

    private struct FontKey: Hashable {
        let id: UInt32
        let size: CGFloat
    }

    private let logger = Logger(subsystem: "app.inkuna.ios", category: "reader")
    private var descriptors: [UInt32: CTFontDescriptor] = [:]
    private var fonts: [FontKey: CTFont] = [:]
    private var loggedUnavailableEntries: Set<FontEntry> = []

    private init() {}

    func prime(_ registry: [FontEntry]) {
        descriptors.removeAll(keepingCapacity: true)
        fonts.removeAll(keepingCapacity: true)

        for entry in registry {
            guard FileManager.default.fileExists(atPath: entry.filePath) else {
                markUnavailable(entry, reason: "font file is missing")
                continue
            }

            guard
                let availableDescriptors = CTFontManagerCreateFontDescriptorsFromURL(
                    URL(fileURLWithPath: entry.filePath) as CFURL
                ) as? [CTFontDescriptor],
                let index = Int(exactly: entry.collectionIndex),
                availableDescriptors.indices.contains(index)
            else {
                markUnavailable(entry, reason: "font descriptor is unavailable")
                continue
            }

            var descriptor = availableDescriptors[index]
            let variations: [NSNumber: NSNumber] = entry.axes.reduce(into: [:]) { result, axis in
                guard let tag = fourCharCode(for: axis.tag) else { return }
                result[NSNumber(value: tag)] = NSNumber(value: axis.value)
            }

            if !variations.isEmpty {
                let attributes: [CFString: Any] = [kCTFontVariationAttribute: variations]
                descriptor = CTFontDescriptorCreateCopyWithAttributes(descriptor, attributes as CFDictionary)
            }

            descriptors[entry.id] = descriptor
        }
    }

    func font(id: UInt32, size: CGFloat) -> CTFont? {
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

    private func markUnavailable(_ entry: FontEntry, reason: String) {
        guard loggedUnavailableEntries.insert(entry).inserted else { return }
        logger.error("Unable to load reader font \(entry.id, privacy: .public): \(reason, privacy: .public)")
    }

    private func fourCharCode(for tag: String) -> UInt32? {
        let bytes = Array(tag.utf8)
        guard bytes.count == 4, bytes.allSatisfy({ $0 < 128 }) else { return nil }

        return bytes.reduce(0) { ($0 << 8) | UInt32($1) }
    }
}
