import UIKit

/// The reader's font roster (the Font list in the Customize panel).
///
/// Stored by the core as an opaque id — like `ReadingTheme` — so fonts can
/// ship shell-first. Bundled faces are the Noto Latin variable cuts; CJK
/// glyphs always fall through to the system faces, which is a product
/// requirement, never an omission.
enum ReadingFont: String, CaseIterable {
    /// The publication's own faces: no font-family rule is emitted at all.
    case publisher
    case systemSerif = "system-serif"
    case systemSans = "system-sans"
    case notoSerif = "noto-serif"
    case notoSans = "noto-sans"

    /// The CSS font-family stack for the reading web view, or nil for
    /// `.publisher`. The bundled families are namespaced "Inkuna Noto …"
    /// because a publisher may embed a face literally named "Noto Serif";
    /// the trailing generic keeps per-glyph CJK fallback intact.
    var cssStack: String? {
        switch self {
        case .publisher: nil
        case .systemSerif: #"ui-serif, "Times New Roman", serif"#
        case .systemSans: "-apple-system, system-ui, sans-serif"
        case .notoSerif: #""Inkuna Noto Serif", serif"#
        case .notoSans: #""Inkuna Noto Sans", sans-serif"#
        }
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

    /// Native face for the preview card and the "Aa" specimens. The web
    /// view never sees app-registered fonts, so this is UI-only; the
    /// reading surface gets the same faces through @font-face. `.publisher`
    /// has no knowable face natively and stands in with the serif reading
    /// face.
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
        let wghtAxis = 0x77676874 // 'wght'
        let variation = UIFontDescriptor.AttributeName(rawValue: kCTFontVariationAttribute as String)
        let descriptor = UIFontDescriptor(fontAttributes: [
            .name: postScriptName,
            variation: [wghtAxis: bold ? 600 : 400],
        ])
        let font = UIFont(descriptor: descriptor, size: size)
        guard font.fontName.hasPrefix("Noto") else {
            return .systemFont(ofSize: size, weight: bold ? .semibold : .regular)
        }
        return font
    }
}
