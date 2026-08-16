import os
import UIKit

/// App-side user settings, persisted by the Rust core's settings store.
///
/// The core record is loaded once at launch (`load()`, awaited before the
/// first screen renders), held in memory for the synchronous reads every
/// screen makes, and written back whole — the core's contract — on a
/// serialized chain so writes land in the order they were made.
@MainActor
final class AppSettings {
    static let shared = AppSettings()

    /// The last-known core record: the in-memory truth between writes.
    /// Starts on the core's own defaults so a library that cannot open
    /// still leaves the app functional.
    private var record = Settings(
        onboarded: false,
        readingTheme: ReadingTheme.paper.rawValue,
        textSizeStep: 2,
        brightness: 0.78
    )

    /// Tail of the serialized settings-write chain: whole-record writes
    /// must reach the core in the order the user made them.
    private var writeChain: Task<Void, Never>?

    private let logger = Logger(subsystem: "app.inkuna.ios", category: "settings")

    private init() {}

    /// The keys settings lived under before the core owned them; read once
    /// to migrate, then removed.
    private enum LegacyKey {
        static let onboarded = "inkuna.onboarded"
        static let readingTheme = "inkuna.readingTheme"
        static let textSize = "inkuna.textSize"
        static let textSizeStep = "inkuna.textSizeStep"
        static let brightness = "inkuna.brightness"
        static let all = [onboarded, readingTheme, textSize, textSizeStep, brightness]
    }

    /// Loads the record from the core. A library that will not open leaves
    /// the defaults standing; the library screen owns that recovery path.
    func load() async {
        do {
            let bookshelf = try await LibraryStore.shared.library()
            if let migrated = migratedLegacySettings() {
                // Settings written by builds that kept them in UserDefaults
                // move into the core once, then the keys are gone.
                try await bookshelf.setSettings(settings: migrated)
                record = migrated
                removeLegacySettings()
            } else {
                record = try await bookshelf.settings()
            }
        } catch {
            logger.warning("Settings load failed: \(error)")
        }
        #if DEBUG
        // `-inkuna.readingTheme moon` deep-launches a theme for the
        // screenshot loop; launch arguments land in the volatile argument
        // domain, which the migration deliberately ignores.
        if let override = UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain)[LegacyKey.readingTheme] as? String {
            record.readingTheme = override
        }
        #endif
    }

    /// The legacy UserDefaults settings as a core record, or nil when no
    /// legacy value exists (the store was born on the core).
    private func migratedLegacySettings() -> Settings? {
        guard
            let bundleID = Bundle.main.bundleIdentifier,
            let stored = UserDefaults.standard.persistentDomain(forName: bundleID),
            LegacyKey.all.contains(where: { stored[$0] != nil })
        else { return nil }

        var migrated = record
        if let onboarded = stored[LegacyKey.onboarded] as? Bool {
            migrated.onboarded = onboarded
        }
        if let theme = stored[LegacyKey.readingTheme] as? String {
            migrated.readingTheme = theme
        }
        if let step = stored[LegacyKey.textSizeStep] as? Int, let size = ReadingTextSize(rawValue: step) {
            migrated.textSizeStep = UInt8(size.rawValue)
        } else if let legacySize = stored[LegacyKey.textSize] as? String {
            // The pre-stepper S/M/L scale, mapped onto the five steps.
            switch legacySize {
            case "S": migrated.textSizeStep = UInt8(ReadingTextSize.small.rawValue)
            case "L": migrated.textSizeStep = UInt8(ReadingTextSize.large.rawValue)
            default: migrated.textSizeStep = UInt8(ReadingTextSize.medium.rawValue)
            }
        }
        if let brightness = stored[LegacyKey.brightness] as? Double {
            migrated.brightness = brightness
        }
        return migrated
    }

    private func removeLegacySettings() {
        for key in LegacyKey.all {
            UserDefaults.standard.removeObject(forKey: key)
        }
    }

    /// Appends one whole-record write to the serialized chain. Failures are
    /// logged, never surfaced: the in-memory record already changed and the
    /// UI moved with it.
    private func persist() {
        let snapshot = record
        let previous = writeChain
        writeChain = Task { [logger] in
            await previous?.value
            do {
                let bookshelf = try await LibraryStore.shared.library()
                try await bookshelf.setSettings(settings: snapshot)
            } catch {
                logger.warning("Settings write failed: \(error)")
            }
        }
    }

    var hasCompletedOnboarding: Bool {
        get { record.onboarded }
        set {
            record.onboarded = newValue
            persist()
        }
    }

    /// The reader's page theme; also decides whether the whole app is in
    /// night mode (`ReadingTheme.isNight`). The core stores the id as an
    /// opaque string, so an id written by a newer build reads back as
    /// `.paper` here without being lost on the next write.
    var readingTheme: ReadingTheme {
        get { ReadingTheme(rawValue: record.readingTheme) ?? .paper }
        set {
            record.readingTheme = newValue.rawValue
            persist()
            applyAppearance()
        }
    }

    var textSize: ReadingTextSize {
        get { ReadingTextSize(rawValue: Int(record.textSizeStep)) ?? .medium }
        set {
            record.textSizeStep = UInt8(newValue.rawValue)
            persist()
        }
    }

    /// Reader page brightness, 0…1. Below the default the reader dims the
    /// page with an ink veil rather than touching system brightness.
    var brightness: Double {
        get { record.brightness }
        set {
            record.brightness = newValue
            persist()
        }
    }

    /// Cross-fades every window of the app into the theme's day/night side.
    func applyAppearance() {
        let style: UIUserInterfaceStyle = readingTheme.isNight ? .dark : .light
        for scene in UIApplication.shared.connectedScenes {
            guard let windowScene = scene as? UIWindowScene else { continue }
            for window in windowScene.windows where window.overrideUserInterfaceStyle != style {
                UIView.transition(with: window, duration: InkMotion.medium, options: [.transitionCrossDissolve, .allowUserInteraction]) {
                    window.overrideUserInterfaceStyle = style
                }
            }
        }
    }
}
