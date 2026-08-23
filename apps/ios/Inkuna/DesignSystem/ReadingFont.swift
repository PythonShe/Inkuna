import UIKit

/// The two bundled reading faces understood by the layout engine.
enum ReadingFont: String, CaseIterable {
    case notoSerif = "noto-serif"
    case notoSans = "noto-sans"

    static func normalize(_ stored: String) -> ReadingFont {
        switch stored.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case notoSans.rawValue, "system-sans": .notoSans
        case notoSerif.rawValue, "publisher", "system-serif": .notoSerif
        default: .notoSerif
        }
    }

    var displayName: String {
        switch self {
        case .notoSerif: String(localized: "font_noto_serif", defaultValue: "Noto Serif")
        case .notoSans: String(localized: "font_noto_sans", defaultValue: "Noto Sans")
        }
    }

    func previewFont(size: CGFloat, bold: Bool) -> UIFont {
        switch self {
        case .notoSerif: Self.notoFont(named: "NotoSerif-Regular", size: size, bold: bold)
        case .notoSans: Self.notoFont(named: "NotoSans-Regular", size: size, bold: bold)
        }
    }

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
