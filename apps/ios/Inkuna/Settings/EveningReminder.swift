import UserNotifications

/// Schedules and cancels the daily evening reading reminder — one repeating
/// local notification at the reading hour. The on/off choice lives in the
/// core's settings record; this owns only the platform scheduling.
enum EveningReminder {
    private static let requestIdentifier = "app.inkuna.ios.evening-reminder"

    // TODO(settings): make the reading hour configurable; 21:00 is the
    // fixed "evening" for now.
    private static let hour = 21

    /// Asks for notification permission if needed and schedules the
    /// repeating reminder. Returns false when permission is declined or
    /// scheduling fails — the caller should snap its switch back off.
    static func enable() async -> Bool {
        let center = UNUserNotificationCenter.current()
        let granted = (try? await center.requestAuthorization(options: [.alert])) ?? false
        guard granted else { return false }

        let content = UNMutableNotificationContent()
        content.title = String(localized: "settings_reminder_notif_title", defaultValue: "Evening reading")
        content.body = String(localized: "settings_reminder_notif_body", defaultValue: "Your book is waiting. A quiet page or two?")
        // No sound: a quiet nudge, per the design's promise.

        var at = DateComponents()
        at.hour = hour
        let trigger = UNCalendarNotificationTrigger(dateMatching: at, repeats: true)
        let request = UNNotificationRequest(identifier: requestIdentifier, content: content, trigger: trigger)
        do {
            try await center.add(request)
            return true
        } catch {
            return false
        }
    }

    static func disable() {
        UNUserNotificationCenter.current()
            .removePendingNotificationRequests(withIdentifiers: [requestIdentifier])
    }
}
