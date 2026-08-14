import UIKit

/// Color tokens ported from the Inkuna design system (`tokens/colors.css`).
///
/// Every semantic color is a dynamic provider that resolves Paper (day)
/// versus Night from the trait collection. The app drives night mode by
/// setting `overrideUserInterfaceStyle` on the window (see `AppSettings`),
/// so these tokens follow the in-app theme rather than the system theme.
enum InkColor {
    // MARK: Backgrounds

    /// `--bg-app`
    static let bgApp = dynamic(day: 0xF6F1E8, night: 0x191713)
    /// `--bg-surface`
    static let bgSurface = dynamic(day: 0xFBF8F2, night: 0x211E18)
    /// `--bg-raised`
    static let bgRaised = dynamic(day: 0xFFFEFA, night: 0x2A261F)
    /// `--bg-reading`
    static let bgReading = dynamic(day: 0xF4EEE1, night: 0x1D1B16)
    /// `--bg-recessed`
    static let bgRecessed = dynamic(day: 0xEFE7D9, night: 0x14120E)

    // MARK: Text

    /// `--text-display`
    static let textDisplay = dynamic(day: 0x241F17, night: 0xEEE6D6)
    /// `--text-body`
    static let textBody = dynamic(day: 0x2C261D, night: 0xE2D9C6)
    /// `--text-secondary`
    static let textSecondary = dynamic(day: 0x6B6255, night: 0xA89D8C)
    /// `--text-tertiary`
    static let textTertiary = dynamic(day: 0x9A9083, night: 0x776F62)

    // MARK: Accent

    /// `--accent` (amber)
    static let accent = dynamic(day: 0xB4863B, night: 0xD9AE63)
    /// `--amber-deep`; pressed/hover accent
    static let accentDeep = dynamic(day: 0x8F6829, night: 0xE7C489)
    /// `--accent-ink`; foreground on accent fills
    static let accentInk = dynamic(day: 0xFBF8F2, night: 0x241F17)
    /// `--accent-soft`; selected-state washes
    static let accentSoft = dynamic(day: 0xB4863B, dayAlpha: 0.12, night: 0xD9AE63, nightAlpha: 0.14)

    // MARK: Signals

    /// `--moon`
    static let moon = dynamic(day: 0x8E97A6, night: 0xA9B2C0)
    /// `--positive`
    static let positive = dynamic(day: 0x557153, night: 0x7E9B7B)
    /// `--danger`
    static let danger = dynamic(day: 0x9C4A38, night: 0xC07660)

    // MARK: Lines & overlays

    /// `--border-hairline`
    static let borderHairline = dynamic(day: 0x241F17, dayAlpha: 0.09, night: 0xEEE6D6, nightAlpha: 0.09)
    /// `--scrim`
    static let scrim = dynamic(day: 0x18140E, dayAlpha: 0.40, night: 0x0A0907, nightAlpha: 0.55)

    // MARK: Helpers

    private static func dynamic(
        day: UInt32,
        dayAlpha: CGFloat = 1,
        night: UInt32,
        nightAlpha: CGFloat = 1
    ) -> UIColor {
        UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? UIColor(ink: night, alpha: nightAlpha)
                : UIColor(ink: day, alpha: dayAlpha)
        }
    }
}

extension UIColor {
    /// Builds a color from a 24-bit `0xRRGGBB` value.
    convenience init(ink hex: UInt32, alpha: CGFloat = 1) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: alpha
        )
    }
}
