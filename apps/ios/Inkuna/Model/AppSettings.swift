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
        brightness: 0.78,
        eveningReminder: false,
        reminderMinutes: 21 * 60,
        accountName: "",
        accountEmail: "",
        readingFont: ReadingFont.publisher.rawValue,
        readingBold: false,
        lineSpacing: 1.65,
        letterSpacing: 0,
        wordSpacing: 0,
        readingMargins: 26
    )

    /// Whether `record` reflects the stored settings. Stays false when the
    /// launch-time load fails, so the write path knows the untouched fields
    /// are defaults it must not persist over the user's stored values.
    private var loaded = false

    /// Tail of the serialized settings-write chain: whole-record writes
    /// must reach the core in the order the user made them.
    private var writeChain: Task<Void, Never>?

    /// Whether a queued write is still waiting to run. It persists the
    /// record as it stands when it executes, so every change made in the
    /// meantime — a brightness drag emits one per tick — rides along on it
    /// instead of queuing a core write of its own.
    private var writeQueued = false

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
            let current = try await bookshelf.settings()
            if let migrated = migratedLegacySettings(over: current) {
                // Settings written by builds that kept them in UserDefaults
                // move into the core once. The keys are removed only after
                // the core write lands — a failed write must leave them in
                // place for the next launch to migrate.
                try await bookshelf.setSettings(settings: migrated)
                removeLegacySettings()
                if adoptStored(migrated) { persist() }
            } else {
                if adoptStored(current) { persist() }
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

    /// The legacy UserDefaults settings overlaid on the core's current
    /// record, or nil when no legacy value exists (the store was born on
    /// the core).
    private func migratedLegacySettings(over current: Settings) -> Settings? {
        guard
            let bundleID = Bundle.main.bundleIdentifier,
            let stored = UserDefaults.standard.persistentDomain(forName: bundleID),
            LegacyKey.all.contains(where: { stored[$0] != nil })
        else { return nil }

        var migrated = current
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

    /// Every change made while `loaded` is false, in order. When a write
    /// finally reaches the core they are replayed over the *stored* record,
    /// so a load failure at launch cannot cost the user the settings they
    /// didn't touch.
    private var pendingRebase: [@MainActor (inout Settings) -> Void] = []

    /// Takes the stored record as the loaded truth, replaying every change
    /// queued while it was being read on top of it. Returns whether
    /// anything was replayed — those changes still owe the core a write.
    private func adoptStored(_ stored: Settings) -> Bool {
        var settings = stored
        let replayed = !pendingRebase.isEmpty
        for pending in pendingRebase { pending(&settings) }
        pendingRebase.removeAll()
        record = settings
        loaded = true
        return replayed
    }

    /// Applies one change to the in-memory record — the UI reads it back
    /// synchronously — and queues a whole-record write.
    private func mutate(_ transform: @escaping @MainActor (inout Settings) -> Void) {
        transform(&record)
        if !loaded { pendingRebase.append(transform) }
        persist()
    }

    /// Queues one whole-record write behind whatever write is in flight, so
    /// writes land in the order they were made. A write that has not started
    /// yet already covers this change — it reads the record when it runs.
    /// Failures are logged, never surfaced.
    private func persist() {
        guard !writeQueued else { return }
        writeQueued = true
        let previous = writeChain
        writeChain = Task {
            await previous?.value
            // From here on the record can still change under the awaits
            // below, so a later change must queue a write of its own.
            writeQueued = false
            do {
                let bookshelf = try await LibraryStore.shared.library()
                if !loaded {
                    // The launch-time load failed, so the record's untouched
                    // fields are defaults, not the user's stored values —
                    // writing it whole would erase them. Rebase instead:
                    // read the stored record and replay every change made
                    // since on top of it.
                    _ = adoptStored(try await bookshelf.settings())
                }
                try await bookshelf.setSettings(settings: record)
            } catch {
                logger.warning("Settings write failed: \(error)")
            }
        }
    }

    var hasCompletedOnboarding: Bool {
        get { record.onboarded }
        set { mutate { $0.onboarded = newValue } }
    }

    /// The reader's page theme; also decides whether the whole app is in
    /// night mode (`ReadingTheme.isNight`). The core stores the id as an
    /// opaque string, parsed case-insensitively (matching Android), so an
    /// id written by a newer build reads back as `.paper` here without
    /// being lost on the next write.
    var readingTheme: ReadingTheme {
        get { ReadingTheme(rawValue: record.readingTheme.lowercased()) ?? .paper }
        set {
            mutate { $0.readingTheme = newValue.rawValue }
            applyAppearance()
        }
    }

    var textSize: ReadingTextSize {
        get { ReadingTextSize(rawValue: Int(record.textSizeStep)) ?? .medium }
        set { mutate { $0.textSizeStep = UInt8(newValue.rawValue) } }
    }

    /// Reader page brightness, 0…1, clamped here as well as by the core so
    /// the in-memory record never diverges from what gets stored.
    /// Below the default the reader dims the page with an ink veil rather
    /// than touching system brightness.
    var brightness: Double {
        get { record.brightness }
        set {
            let clamped = min(max(newValue, 0), 1)
            mutate { $0.brightness = clamped }
        }
    }

    /// Whether the daily evening reading reminder is on. The core stores
    /// the choice; scheduling the local notification is the caller's job.
    var eveningReminder: Bool {
        get { record.eveningReminder }
        set { mutate { $0.eveningReminder = newValue } }
    }

    /// When the reminder fires, in minutes after local midnight. Clamped
    /// here as well as by the core so the in-memory record never diverges
    /// from what gets stored.
    var reminderMinutes: Int {
        get { Int(record.reminderMinutes) }
        set { mutate { $0.reminderMinutes = UInt16(min(max(newValue, 0), 23 * 60 + 59)) } }
    }

    /// The reader's body-text font. Stored as an opaque id like the theme:
    /// an unknown id reads as `.publisher` here but is not overwritten
    /// unless the user actually picks a font.
    var readingFont: ReadingFont {
        get { ReadingFont(rawValue: record.readingFont.lowercased()) ?? .publisher }
        set { mutate { $0.readingFont = newValue.rawValue } }
    }

    /// The stored font id verbatim, unparsed. A commit that is not itself a
    /// font pick writes this back, so an id this build does not know
    /// survives a bold toggle or a slider release instead of collapsing to
    /// `publisher`.
    var readingFontID: String {
        get { record.readingFont }
        set { mutate { $0.readingFont = newValue } }
    }

    /// Whether body text is forced to weight 600 on the reading surface.
    var readingBold: Bool {
        get { record.readingBold }
        set { mutate { $0.readingBold = newValue } }
    }

    /// Reading line-height multiplier. Clamped here as well as by the core
    /// so the in-memory record never diverges from what gets stored.
    var lineSpacing: Double {
        get { record.lineSpacing }
        set { mutate { $0.lineSpacing = min(max(newValue, 1.30), 2.10) } }
    }

    /// Extra letter spacing on the reading surface, in em.
    var letterSpacing: Double {
        get { record.letterSpacing }
        set { mutate { $0.letterSpacing = min(max(newValue, 0), 0.06) } }
    }

    /// Extra word spacing on the reading surface, in em.
    var wordSpacing: Double {
        get { record.wordSpacing }
        set { mutate { $0.wordSpacing = min(max(newValue, 0), 0.30) } }
    }

    /// Horizontal page margins, in CSS px inside the reading web view.
    var readingMargins: Int {
        get { Int(record.readingMargins) }
        set { mutate { $0.readingMargins = UInt16(min(max(newValue, 16), 48)) } }
    }

    /// The six Customize values back to their defaults in one record write.
    /// Text size, theme, and brightness deliberately survive — the design's
    /// reset covers only the Customize panel.
    func resetReadingCustomization() {
        mutate {
            $0.readingFont = ReadingFont.publisher.rawValue
            $0.readingBold = false
            $0.lineSpacing = 1.65
            $0.letterSpacing = 0
            $0.wordSpacing = 0
            $0.readingMargins = 26
        }
    }

    /// Display name of the purely local account; empty means "not set".
    var accountName: String {
        get { record.accountName }
        set { mutate { $0.accountName = newValue } }
    }

    /// Contact email of the purely local account; empty means "not set".
    var accountEmail: String {
        get { record.accountEmail }
        set { mutate { $0.accountEmail = newValue } }
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
