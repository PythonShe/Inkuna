import UIKit
import UniformTypeIdentifiers

/// The import feature's front door.
///
/// One line puts the whole experience on a screen:
///
/// ```swift
/// ImportFlow.presentPicker(from: self)
/// ```
///
/// That covers picking files, the sandbox dance, progress, and every piece
/// of feedback. Files arriving from outside the app ("Open with Inkuna")
/// come in through ``handle(_:presentingFrom:)`` instead, and land in the
/// same pipeline.
///
/// Screens that show books observe ``Notification/Name/inkunaLibraryDidChange``
/// rather than wiring a callback, so a book added from anywhere refreshes
/// everywhere.
///
/// The flow object owns itself: it is retained for exactly as long as a
/// picker is up or a run is in flight (`UIDocumentPickerViewController`
/// holds its delegate weakly, which is the usual way this kind of flow
/// dies half-finished), and released the moment it reports back.
@MainActor
final class ImportFlow: NSObject {
    /// Flows currently alive. Never more than a couple: one waiting on a
    /// picker, one running.
    private static var active: [ImportFlow] = []

    /// Runs are serialized through this chain. Two imports at once would
    /// have the core's single writer queueing them anyway, and the reader
    /// would be watching two progress cards fight over the screen.
    private static var chain: Task<Void, Never>?

    private weak var presenter: UIViewController?
    private let consumesOriginals: Bool
    private let onFinish: (@MainActor (ImportReport) -> Void)?
    private var task: Task<ImportReport, Never>?

    private init(
        presenter: UIViewController,
        consumesOriginals: Bool,
        onFinish: (@MainActor (ImportReport) -> Void)?
    ) {
        self.presenter = presenter
        self.consumesOriginals = consumesOriginals
        self.onFinish = onFinish
    }

    // MARK: Entry points

    /// Presents the document picker and runs whatever the reader chooses.
    ///
    /// - Parameters:
    ///   - presenter: the screen the picker and its feedback belong to.
    ///   - onFinish: optional; most callers should observe
    ///     ``Notification/Name/inkunaLibraryDidChange`` instead.
    static func presentPicker(
        from presenter: UIViewController,
        onFinish: (@MainActor (ImportReport) -> Void)? = nil
    ) {
        let flow = ImportFlow(presenter: presenter, consumesOriginals: false, onFinish: onFinish)
        active.append(flow)
        flow.presentPicker()
    }

    /// Imports files the shell already has URLs for — an inbound document,
    /// a debug fixture — without showing a picker.
    ///
    /// - Parameter consumingOriginals: delete each source file once it has
    ///   been dealt with. True only for files iOS copied into our own
    ///   `Documents/Inbox`, which are ours to clean up; a file the reader
    ///   still owns is never touched.
    static func importFiles(
        _ urls: [URL],
        from presenter: UIViewController,
        consumingOriginals: Bool = false,
        onFinish: (@MainActor (ImportReport) -> Void)? = nil
    ) {
        guard !urls.isEmpty else { return }
        let flow = ImportFlow(
            presenter: presenter,
            consumesOriginals: consumingOriginals,
            onFinish: onFinish
        )
        active.append(flow)
        flow.run(urls: urls)
    }

    /// Handles "Open with Inkuna" — a book handed to us by Files, Mail,
    /// Safari, or a share sheet.
    ///
    /// Without `LSSupportsOpeningDocumentsInPlace`, iOS copies the file
    /// into `Documents/Inbox` first, so these URLs are inside our own
    /// container and are ours to delete afterwards. The security-scope
    /// call still runs for them and correctly reports "no extension
    /// needed"; see `ImportStaging`.
    static func handle(_ contexts: Set<UIOpenURLContext>, presentingFrom presenter: UIViewController?) {
        let urls = contexts.map(\.url).filter(\.isFileURL)
        guard !urls.isEmpty, let presenter else { return }
        importFiles(urls, from: presenter, consumingOriginals: true)
    }

    /// The topmost view controller a flow can present from, for entry
    /// points that have a scene rather than a screen.
    static func presenter(in scene: UIScene?) -> UIViewController? {
        let window = (scene as? UIWindowScene)?
            .windows
            .first(where: \.isKeyWindow)
            ?? (scene as? UIWindowScene)?.windows.first
        var controller = window?.rootViewController
        while let presented = controller?.presentedViewController {
            controller = presented
        }
        return controller
    }

    // MARK: Picking

    /// Content types the picker offers.
    ///
    /// Deliberately wider than what Inkuna imports today: a reader with a
    /// MOBI in Files is better served by picking it and being told "MOBI
    /// isn't supported yet" than by watching it sit greyed out with no
    /// explanation. The core detects the real format from the file's magic
    /// bytes and names it, which is what makes that promise possible.
    private static var pickableTypes: [UTType] {
        var types: [UTType] = [.epub, .pdf, .plainText, .zip]
        for tag in ["mobi", "azw3", "azw", "cbz", "cbr"] {
            if let type = UTType(filenameExtension: tag) {
                types.append(type)
            }
        }
        return types
    }

    private func presentPicker() {
        guard let presenter else { return finish(ImportReport()) }
        let picker = UIDocumentPickerViewController(
            forOpeningContentTypes: Self.pickableTypes,
            asCopy: false
        )
        picker.delegate = self
        picker.allowsMultipleSelection = true
        picker.shouldShowFileExtensions = true
        presenter.present(picker, animated: true)
    }

    // MARK: Running

    private func run(urls: [URL]) {
        Self.enqueue { [weak self] in
            await self?.perform(urls: urls)
        }
    }

    private func perform(urls: [URL]) async {
        guard let presenter else { return finish(ImportReport()) }
        await Self.waitUntilPresentable(presenter)

        let overlay = ImportProgressOverlay(total: urls.count) { [weak self] in
            self?.task?.cancel()
        }
        overlay.present(in: presenter.view)

        let task = Task { @MainActor in
            await ImportService.run(urls: urls) { progress in
                overlay.update(progress)
            }
        }
        self.task = task
        let report = await task.value
        await overlay.dismiss()

        if consumesOriginals {
            Self.consume(urls)
        }
        if report.didChangeLibrary {
            NotificationCenter.default.post(name: .inkunaLibraryDidChange, object: nil)
        }
        await Self.waitUntilPresentable(presenter)
        ImportFeedback.present(report, from: presenter)
        finish(report)
    }

    private func finish(_ report: ImportReport) {
        onFinish?(report)
        Self.active.removeAll { $0 === self }
    }

    // MARK: Plumbing

    private static func enqueue(_ operation: @escaping @MainActor () async -> Void) {
        let previous = chain
        chain = Task { @MainActor in
            await previous?.value
            await operation()
        }
    }

    /// Deletes inbound copies iOS made for us, and only those: anything
    /// outside our own container belongs to the reader.
    private static func consume(_ urls: [URL]) {
        guard let documents = try? FileManager.default.url(
            for: .documentDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: false
        ) else { return }
        let inbox = documents.appending(path: "Inbox", directoryHint: .isDirectory).standardizedFileURL
        for url in urls where url.standardizedFileURL.path.hasPrefix(inbox.path + "/") {
            try? FileManager.default.removeItem(at: url)
        }
    }

    /// Waits for the presenter to be on screen with nothing modal over it.
    ///
    /// Two cases need this: a picker that is still animating away as its
    /// delegate fires, and a scene that hands us a document before its
    /// window is up. Bounded, because a wait that never ends would swallow
    /// the import silently.
    private static func waitUntilPresentable(_ presenter: UIViewController) async {
        let deadline = ContinuousClock.now + .seconds(2)
        while ContinuousClock.now < deadline {
            let onScreen = presenter.viewIfLoaded?.window != nil
            if onScreen, presenter.presentedViewController == nil {
                return
            }
            try? await Task.sleep(for: .milliseconds(50))
        }
    }
}

// MARK: - Document picker

extension ImportFlow: UIDocumentPickerDelegate {
    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        run(urls: urls)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(ImportReport())
    }
}

// MARK: - Notifications

extension Notification.Name {
    /// Posted on the main actor after an import adds books to the library.
    /// Any screen showing books should reload on it.
    static let inkunaLibraryDidChange = Notification.Name("app.inkuna.libraryDidChange")
}
