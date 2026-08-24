import CoreText
import UIKit

/// Resolves the platform reading faces — New York for `system-serif`,
/// San Francisco for `system-sans` — down to the font files the engine
/// can shape with.
///
/// Discovery is best-effort by design: a face whose file cannot be
/// resolved or read is silently skipped, and the engine substitutes the
/// bundled Noto for that role at selection time. Nothing here throws.
enum SystemReadingFonts {
    /// The regular/bold × upright/italic grid for both roles, deduplicated.
    /// Both weights of a variable face resolve to the same file; the core
    /// instances weights from the `wght` axis itself, so one entry per
    /// distinct face is enough.
    static func discover() -> [SystemFontFace] {
        var faces: [SystemFontFace] = []
        var seen: Set<FaceKey> = []
        // 400 before 700, so a variable file keeps its regular entry.
        for (weight, uiWeight) in [(UInt16(400), UIFont.Weight.regular), (UInt16(700), .bold)] {
            for italic in [false, true] {
                for (role, design) in [
                    (SystemFontRole.serif, UIFontDescriptor.SystemDesign.serif),
                    (SystemFontRole.sans, .default),
                ] {
                    guard let face = resolve(
                        role: role,
                        design: design,
                        weight: weight,
                        uiWeight: uiWeight,
                        italic: italic
                    ) else { continue }
                    let key = FaceKey(
                        role: role,
                        italic: italic,
                        filePath: face.filePath,
                        postScriptName: face.postScriptName
                    )
                    if seen.insert(key).inserted {
                        faces.append(face)
                    }
                }
            }
        }
        return faces
    }

    private struct FaceKey: Hashable {
        let role: SystemFontRole
        let italic: Bool
        let filePath: String
        let postScriptName: String?
    }

    private static func resolve(
        role: SystemFontRole,
        design: UIFontDescriptor.SystemDesign,
        weight: UInt16,
        uiWeight: UIFont.Weight,
        italic: Bool
    ) -> SystemFontFace? {
        var descriptor = UIFont.systemFont(ofSize: 17, weight: uiWeight).fontDescriptor
        if design != .default {
            // A missing design means the platform has no such face.
            guard let designed = descriptor.withDesign(design) else { return nil }
            descriptor = designed
        }
        if italic {
            guard let slanted = descriptor.withSymbolicTraits(.traitItalic) else { return nil }
            descriptor = slanted
        }

        let font = CTFontCreateWithFontDescriptor(descriptor as CTFontDescriptor, 17, nil)
        // An italic request the platform satisfied with the upright face
        // must not be registered as italic, or it would hijack emphasis.
        if italic, !CTFontGetSymbolicTraits(font).contains(.traitItalic) {
            return nil
        }
        guard
            let url = CTFontCopyAttribute(font, kCTFontURLAttribute) as? URL,
            url.isFileURL
        else { return nil }
        let path = url.path
        // System font files live under /System and are sandbox-readable;
        // anything the process cannot actually read is skipped up front so
        // the engine never records a face it cannot open.
        guard FileManager.default.isReadableFile(atPath: path) else { return nil }

        // Only a collection needs the PostScript name: the core scans the
        // .ttc for the matching face. For single-face files the name CTFont
        // reports can be an alias of the name table's, and a mismatch would
        // reject a perfectly good face — index 0 is already exact.
        let postScriptName: String? = url.pathExtension.lowercased() == "ttc"
            ? CTFontCopyPostScriptName(font) as String
            : nil

        return SystemFontFace(
            role: role,
            italic: italic,
            weight: weight,
            filePath: path,
            postScriptName: postScriptName,
            ttcHint: nil
        )
    }
}
