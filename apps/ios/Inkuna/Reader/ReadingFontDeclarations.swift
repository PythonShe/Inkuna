import Foundation
import ReadiumNavigator
import ReadiumShared

/// The transport that puts the bundled Noto files in front of WebKit.
///
/// Readium's font declarations are used purely as serving plumbing: they
/// emit the `@font-face` rules and host the files on the web-view server's
/// CORS-enabled assets origin. Which family the page actually *uses* is
/// decided by our own injected stylesheet (`ReaderUserStyle`), never by
/// `EPUBPreferences.fontFamily` — so when the custom renderer replaces the
/// navigator, only this file dies; the family names and the styling
/// contract survive.
extension ReadingFont {
    static var fontFamilyDeclarations: [AnyHTMLFontFamilyDeclaration] {
        [
            declaration(family: "Inkuna Noto Serif", regular: "NotoSerif", italic: "NotoSerif-Italic"),
            declaration(family: "Inkuna Noto Sans", regular: "NotoSans", italic: "NotoSans-Italic"),
        ].compactMap { $0 }
    }

    /// One family from its two variable cuts. A missing bundle file drops
    /// the declaration (the CSS stack then falls to its generic) instead of
    /// crashing the open.
    private static func declaration(
        family: String,
        regular: String,
        italic: String
    ) -> AnyHTMLFontFamilyDeclaration? {
        guard
            let regularFile = bundledFont(named: regular),
            let italicFile = bundledFont(named: italic)
        else { return nil }
        return CSSFontFamilyDeclaration(
            fontFamily: FontFamily(rawValue: family),
            fontFaces: [
                CSSFontFace(file: regularFile, style: .normal, weight: .variable(100 ... 900)),
                CSSFontFace(file: italicFile, style: .italic, weight: .variable(100 ... 900)),
            ]
        ).eraseToAnyHTMLFontFamilyDeclaration()
    }

    private static func bundledFont(named name: String) -> FileURL? {
        Bundle.main.url(forResource: name, withExtension: "ttf").flatMap { FileURL(url: $0) }
    }
}
