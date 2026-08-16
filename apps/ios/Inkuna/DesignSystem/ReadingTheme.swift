import UIKit

/// The reader's four page themes (the swatches in the Theme & type sheet).
///
/// Paper and Moon are the canonical day/night pair from the onboarding
/// ritual; Calm and Quiet sit between them. Picking a night-side theme
/// flips the whole app into night mode (see `AppSettings.readingTheme`).
enum ReadingTheme: String, CaseIterable {
    case paper
    case calm
    case quiet
    case moon

    /// Page background. Fixed (not trait-dynamic): the reading surface is
    /// the theme, and the app chrome follows it, never the other way round.
    var background: UIColor {
        switch self {
        case .paper: UIColor(ink: 0xF4EEE1)
        case .calm: UIColor(ink: 0xEDE3CE)
        case .quiet: UIColor(ink: 0x3B3833)
        case .moon: UIColor(ink: 0x1D1B16)
        }
    }

    /// Body-text ink on that page.
    var foreground: UIColor {
        switch self {
        case .paper: UIColor(ink: 0x2C261D)
        case .calm: UIColor(ink: 0x3A3226)
        case .quiet: UIColor(ink: 0xD8D2C6)
        case .moon: UIColor(ink: 0xE2D9C6)
        }
    }

    /// Dimmed ink for chapter eyebrows and marginalia (55% mix).
    var dimmedForeground: UIColor {
        foreground.withAlphaComponent(0.55)
    }

    /// Whether this theme pulls the surrounding app into night mode.
    var isNight: Bool {
        switch self {
        case .paper, .calm: false
        case .quiet, .moon: true
        }
    }

    var displayName: String {
        switch self {
        case .paper: String(localized: "theme_paper", defaultValue: "Paper")
        case .calm: String(localized: "theme_calm", defaultValue: "Calm")
        case .quiet: String(localized: "theme_quiet", defaultValue: "Quiet")
        case .moon: String(localized: "theme_moon", defaultValue: "Moon")
        }
    }

    var subtitle: String {
        switch self {
        case .paper: String(localized: "theme_paper_subtitle", defaultValue: "Warm daylight reading")
        case .calm: String(localized: "theme_calm_subtitle", defaultValue: "Soft parchment")
        case .quiet: String(localized: "theme_quiet_subtitle", defaultValue: "Low graphite")
        case .moon: String(localized: "theme_moon_subtitle", defaultValue: "Lamplight for late hours")
        }
    }
}
