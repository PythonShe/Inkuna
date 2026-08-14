import UIKit

/// App-side user settings.
///
/// TODO(core): persistence moves into the Rust core's settings store once
/// it lands; UserDefaults is a stand-in so the UI flow works end to end.
@MainActor
final class AppSettings {
    static let shared = AppSettings()

    private let defaults = UserDefaults.standard

    private enum Key {
        static let onboarded = "inkuna.onboarded"
        static let readingTheme = "inkuna.readingTheme"
        static let textSize = "inkuna.textSize"
        static let brightness = "inkuna.brightness"
    }

    private init() {}

    var hasCompletedOnboarding: Bool {
        get { defaults.bool(forKey: Key.onboarded) }
        set { defaults.set(newValue, forKey: Key.onboarded) }
    }

    /// The reader's page theme; also decides whether the whole app is in
    /// night mode (`ReadingTheme.isNight`).
    var readingTheme: ReadingTheme {
        get { defaults.string(forKey: Key.readingTheme).flatMap(ReadingTheme.init) ?? .paper }
        set {
            defaults.set(newValue.rawValue, forKey: Key.readingTheme)
            applyAppearance()
        }
    }

    var textSize: ReadingTextSize {
        get { defaults.string(forKey: Key.textSize).flatMap(ReadingTextSize.init) ?? .medium }
        set { defaults.set(newValue.rawValue, forKey: Key.textSize) }
    }

    /// Reader page brightness, 0…1. Below the default the reader dims the
    /// page with an ink veil rather than touching system brightness.
    var brightness: Double {
        get { defaults.object(forKey: Key.brightness) as? Double ?? 0.78 }
        set { defaults.set(newValue, forKey: Key.brightness) }
    }

    /// Cross-fades every window of the app into the theme's day/night side.
    func applyAppearance() {
        let style: UIUserInterfaceStyle = readingTheme.isNight ? .dark : .light
        for scene in UIApplication.shared.connectedScenes {
            guard let windowScene = scene as? UIWindowScene else { continue }
            for window in windowScene.windows where window.overrideUserInterfaceStyle != style {
                UIView.transition(with: window, duration: InkMotion.medium, options: .transitionCrossDissolve) {
                    window.overrideUserInterfaceStyle = style
                }
            }
        }
    }
}
