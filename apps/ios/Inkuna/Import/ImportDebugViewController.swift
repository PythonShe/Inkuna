#if DEBUG
import UIKit

/// Deep-launch route for the import harness:
/// `xcrun simctl launch booted app.inkuna.ios -inkuna.onboarded YES -inkuna.debugScreen import`
///
/// It presents rather than pushes, so it works whether the app came up on
/// the tab bar or on onboarding.
@MainActor
enum ImportDebugLaunch {
    private static var routed = false

    static func routeIfRequested(in scene: UIScene) {
        guard !routed,
              UserDefaults.standard.string(forKey: "inkuna.debugScreen") == "import" else { return }
        routed = true
        Task { @MainActor in
            // One turn of the run loop, so the root view controller is on
            // screen before anything is presented over it.
            await Task.yield()
            guard let presenter = ImportFlow.presenter(in: scene) else { return }
            let harness = ImportDebugViewController()
            harness.modalPresentationStyle = .fullScreen
            presenter.present(harness, animated: false)
        }
    }
}

/// Development harness for the import flow, reachable with
/// `xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen import`.
///
/// It exists because the document picker cannot be driven from a script:
/// this screen runs the *same* `ImportFlow` entry point over files dropped
/// into the app container, so a scripted run exercises staging, the core's
/// batch pipeline, dedupe, and every feedback path for real — while the
/// picker itself is verified by eye.
///
/// Drop fixtures in with:
/// ```
/// container=$(xcrun simctl get_app_container booted app.inkuna.ios data)
/// mkdir -p "$container/Documents/ImportFixtures" && cp *.epub "$_"
/// ```
final class ImportDebugViewController: ScrollScreenViewController {
    private static let fixtureFolder = "ImportFixtures"

    private let listStack = UIStackView()
    private var emptyState: EmptyLibraryView?
    // `nonisolated(unsafe)`: deinit must remove the observer, and a
    // main-actor view controller's deinit is nonisolated in Swift 6. The
    // token is written once on the main actor and read once at deinit.
    private nonisolated(unsafe) var libraryObserver: NSObjectProtocol?

    deinit {
        if let libraryObserver {
            NotificationCenter.default.removeObserver(libraryObserver)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        let title = displayTitle("Import")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space5, after: title)

        let add = InkButton("Pick", variant: .primary, size: .small, symbol: "plus") { [weak self] in
            guard let self else { return }
            ImportFlow.presentPicker(from: self)
        }
        let fixtures = InkButton("Fixtures", variant: .secondary, size: .small, symbol: "shippingbox") { [weak self] in
            self?.importFixtures()
        }
        let reset = InkButton("Reset", variant: .ghost, size: .small, symbol: "trash") { [weak self] in
            self?.resetLibrary()
        }
        let actions = UIStackView(arrangedSubviews: [add, fixtures, reset, UIView()])
        actions.axis = .horizontal
        actions.spacing = InkSpacing.space2
        actions.alignment = .center
        contentStack.addArrangedSubview(actions)
        contentStack.setCustomSpacing(InkSpacing.space5, after: actions)

        listStack.axis = .vertical
        contentStack.addArrangedSubview(listStack)

        libraryObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }
        reload()

        // `-inkuna.debugImportFixtures YES` runs the fixture import without
        // a tap, so a scripted run can screenshot every state. Any other
        // value is a file-name filter, which is how the single-file
        // feedback paths (toast, alert) get exercised from a script.
        // `-inkuna.debugPresentPicker YES` opens the document picker on
        // appear, so a screenshot run can see it without a tap.
        if UserDefaults.standard.bool(forKey: "inkuna.debugPresentPicker") {
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(400))
                ImportFlow.presentPicker(from: self)
            }
        }

        if let request = UserDefaults.standard.string(forKey: "inkuna.debugImportFixtures") {
            let filter = ["YES", "1", "true"].contains(request) ? nil : request
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(600))
                self.importFixtures(matching: filter)
            }
        }
    }

    // MARK: Actions

    private func importFixtures(matching filter: String? = nil) {
        guard let documents = try? FileManager.default.url(
            for: .documentDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) else { return }
        let folder = documents.appending(path: Self.fixtureFolder, directoryHint: .isDirectory)
        let files = ((try? FileManager.default.contentsOfDirectory(
            at: folder,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )) ?? []).filter { filter.map($0.lastPathComponent.contains) ?? true }
        guard !files.isEmpty else {
            InkToastView.show(
                symbol: "shippingbox",
                text: "No fixtures in Documents/\(Self.fixtureFolder)",
                in: view,
                topInset: view.safeAreaInsets.top + InkSpacing.space3
            )
            return
        }
        ImportFlow.importFiles(files.sorted { $0.lastPathComponent < $1.lastPathComponent }, from: self)
    }

    private func resetLibrary() {
        Task {
            guard let shelf = try? await LibraryStore.shared.library() else { return }
            let books = (try? await shelf.list(shelf: .all, sort: .recentlyAdded)) ?? []
            for book in books {
                try? await shelf.remove(id: book.id)
            }
            reload()
        }
    }

    // MARK: List

    private func reload() {
        Task {
            let books: [Publication]
            do {
                books = try await LibraryStore.shared.library().list(shelf: .all, sort: .recentlyAdded)
            } catch {
                books = []
            }
            render(books)
        }
    }

    private func render(_ books: [Publication]) {
        listStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        emptyState = nil

        guard !books.isEmpty else {
            let empty = EmptyLibraryView.inviting(self)
            listStack.addArrangedSubview(empty)
            emptyState = empty
            empty.appear()
            return
        }

        let eyebrow = eyebrowLabel("\(books.count) in the library")
        listStack.addArrangedSubview(eyebrow)
        listStack.setCustomSpacing(InkSpacing.space2, after: eyebrow)

        for book in books {
            let row = BookListRowView()
            row.configure(
                title: book.title,
                author: book.authors.joined(separator: ", "),
                progress: CGFloat(book.progression),
                seed: BookCoverView.coverSeed(for: book.id),
                coverPath: book.coverPath,
                downloaded: true
            )
            listStack.addArrangedSubview(row)

            let detail = InkLabel()
            detail.font = InkFont.caption
            detail.textColor = InkColor.textTertiary
            detail.numberOfLines = 0
            detail.text = "\(book.format) · \(book.language ?? "—") · cover: \(book.coverPath == nil ? "none" : "yes")"
            listStack.addArrangedSubview(detail)
            listStack.setCustomSpacing(InkSpacing.space4, after: detail)
        }
    }
}
#endif
