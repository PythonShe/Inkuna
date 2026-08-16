import UIKit

/// Turns a finished import into something the reader can act on.
///
/// The rule is proportionality, and it is the whole design:
///
/// - Everything landed → a toast. Adding books should feel like putting
///   them on a shelf, not like completing a transaction.
/// - One book, and it was already there → a toast that *names it*. A
///   duplicate is information, not an error, and silently swallowing it is
///   how a reader ends up wondering whether the import worked at all.
/// - One file, and it failed → an alert. This is the only outcome the
///   reader may need to do something about.
/// - More than one file with anything to explain → the summary sheet,
///   which itemizes every file by name.
@MainActor
enum ImportFeedback {
    static func present(_ report: ImportReport, from presenter: UIViewController) {
        guard !report.isEmpty else { return }
        haptic(for: report)

        if report.needsSummary {
            presenter.present(ImportSummaryViewController(report: report), animated: true)
            return
        }

        if report.isCleanSuccess, report.items.count > 1 {
            toast(symbol: "checkmark.circle", text: ImportCopy.addedMany(report.imported.count), in: presenter)
            return
        }

        switch report.items.first {
        case .imported(let publication, _):
            toast(
                symbol: "checkmark.circle",
                text: ImportCopy.addedOne(ImportCopy.shortened(publication.title)),
                in: presenter
            )
        case .duplicate(let publication, _):
            toast(
                symbol: "books.vertical",
                text: ImportCopy.alreadyInLibrary(ImportCopy.shortened(publication.title)),
                in: presenter
            )
        case .failed(let failure):
            alert(failure, in: presenter)
        case nil:
            break
        }
    }

    // MARK: Vehicles

    /// Floats a design-system toast, bounded to the screen's gutters. The
    /// width bound is the point: a CJK title is dense enough that an
    /// unbounded capsule runs off both edges.
    private static func toast(symbol: String, text: String, in presenter: UIViewController) {
        let host: UIView = presenter.view
        for case let stale as InkToastView in host.subviews {
            stale.removeFromSuperview()
        }

        let toast = InkToastView(symbol: symbol, text: text)
        toast.alpha = 0
        toast.isAccessibilityElement = true
        toast.accessibilityLabel = text
        toast.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(toast)
        NSLayoutConstraint.activate([
            toast.centerXAnchor.constraint(equalTo: host.centerXAnchor),
            toast.topAnchor.constraint(
                equalTo: host.safeAreaLayoutGuide.topAnchor,
                constant: InkSpacing.space3
            ),
            toast.widthAnchor.constraint(
                lessThanOrEqualTo: host.widthAnchor,
                constant: -2 * InkSpacing.pageMargin
            ),
        ])

        InkMotion.runQuiet(duration: InkMotion.fast) { toast.alpha = 1 }
        UIAccessibility.post(notification: .announcement, argument: text)

        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            guard toast.superview != nil else { return }
            InkMotion.runQuiet(duration: InkMotion.medium) {
                toast.alpha = 0
            } completion: {
                toast.removeFromSuperview()
            }
        }
    }

    private static func alert(_ failure: ImportFailure, in presenter: UIViewController) {
        let alert = UIAlertController(
            title: ImportCopy.couldNotAdd(failure.fileName),
            message: ImportCopy.explanation(failure.reason),
            preferredStyle: .alert
        )
        alert.addAction(UIAlertAction(title: ImportCopy.dismiss, style: .default))
        presenter.present(alert, animated: true)
    }

    private static func haptic(for report: ImportReport) {
        let generator = UINotificationFeedbackGenerator()
        if !report.failures.isEmpty {
            generator.notificationOccurred(.error)
        } else if !report.duplicates.isEmpty {
            generator.notificationOccurred(.warning)
        } else if !report.imported.isEmpty {
            generator.notificationOccurred(.success)
        }
    }
}
