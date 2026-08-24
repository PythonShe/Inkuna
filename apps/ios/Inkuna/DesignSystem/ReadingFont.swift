import UIKit

/// The reader's font roster (the Font list in the Customize panel).
///
/// Stored by the core as an opaque id. `publisher` keeps the book's own
/// embedded faces; the `system-*` ids select the platform faces registered
/// with the engine at startup; the `noto-*` ids pin the bundled Latin
/// variable cuts. CJK glyphs always fall through to the bundled CJK Notos,
/// which is a product requirement, never an omission.
enum ReadingFont: String, CaseIterable {
    /// The publication's own faces: the engine honors the book's
    /// @font-face rules and falls back to Noto Serif.
    case publisher
    case systemSerif = "system-serif"
    case systemSans = "system-sans"
    case notoSerif = "noto-serif"
    case notoSans = "noto-sans"

    /// The fresh-install face — mirrors the core DB default.
    static let standard: ReadingFont = .publisher

    /// Known ids map to themselves; anything else folds to `.notoSerif`,
    /// exactly as the engine folds unknown ids, so the shell's readout and
    /// the laid-out page can never disagree.
    static func normalize(_ stored: String) -> ReadingFont {
        let id = stored.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return ReadingFont(rawValue: id) ?? .notoSerif
    }

    var displayName: String {
        switch self {
        case .publisher: String(localized: "font_publisher", defaultValue: "Publisher default")
        case .systemSerif: String(localized: "font_system_serif", defaultValue: "System Serif")
        case .systemSans: String(localized: "font_system_sans", defaultValue: "System Sans")
        case .notoSerif: String(localized: "font_noto_serif", defaultValue: "Noto Serif")
        case .notoSans: String(localized: "font_noto_sans", defaultValue: "Noto Sans")
        }
    }

    /// Native face for the preview card's specimen. UI-only: the reading
    /// surface gets its faces from the engine's registry. `.publisher` has
    /// no knowable face here and stands in with the serif reading face.
    func previewFont(size: CGFloat, bold: Bool) -> UIFont {
        // The reading surface maps the bold toggle to weight 600.
        let weight: UIFont.Weight = bold ? .semibold : .regular
        switch self {
        case .publisher, .systemSerif:
            let base = UIFont.systemFont(ofSize: size, weight: weight)
            let serif = base.fontDescriptor.withDesign(.serif) ?? base.fontDescriptor
            return UIFont(descriptor: serif, size: size)
        case .systemSans:
            return UIFont.systemFont(ofSize: size, weight: weight)
        case .notoSerif:
            return Self.notoFont(named: "NotoSerif-Regular", size: size, bold: bold)
        case .notoSans:
            return Self.notoFont(named: "NotoSans-Regular", size: size, bold: bold)
        }
    }

    /// A Noto variable face at the requested weight via the 'wght' axis —
    /// name lookups only reach the default instance of a variable font.
    /// Falls back to the system face if the bundle lost the file.
    private static func notoFont(named postScriptName: String, size: CGFloat, bold: Bool) -> UIFont {
        let variation = UIFontDescriptor.AttributeName(rawValue: kCTFontVariationAttribute as String)
        let descriptor = UIFontDescriptor(fontAttributes: [
            .name: postScriptName,
            variation: [0x77676874: bold ? 600 : 400], // 'wght'
        ])
        let font = UIFont(descriptor: descriptor, size: size)
        guard font.fontName.hasPrefix("Noto") else {
            return .systemFont(ofSize: size, weight: bold ? .semibold : .regular)
        }
        return font
    }
}
