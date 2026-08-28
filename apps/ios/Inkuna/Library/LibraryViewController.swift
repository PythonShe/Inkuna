import UIKit

/// The Library tab: search, the Reading/Finished/Wishlist segments, and
/// the books — all of it rendered from the Rust core, as either a list or
/// a cover grid (the user's persisted choice).
///
/// Shelf membership is the core's decision, never this screen's: each
/// segment asks for its own `Shelf` rather than fetching everything and
/// re-deriving "reading" or "finished" in Swift, which would duplicate core
/// logic and drift from it.
///
/// The screen is one compositional collection view: the header (title,
/// search, segments) is a full-width section that scrolls away naturally,
/// and the books section switches between a list layout and a grid whose
/// column count is recomputed from the container width on every layout
/// pass — rotation and window resizes reflow for free.
///
/// TODO(core): the Wishlist segment has no core shelf yet (file-less
/// publications are deferred to their own spec), so it stays visible and
/// empty.
final class LibraryViewController: UIViewController {
    private enum Segment: Int, CaseIterable {
        case reading
        case finished
        case wishlist

        var title: String {
            switch self {
            case .reading: String(localized: "library_seg_reading", defaultValue: "Reading")
            case .finished: String(localized: "library_seg_finished", defaultValue: "Finished")
            case .wishlist: String(localized: "library_seg_wishlist", defaultValue: "Wishlist")
            }
        }
    }

    private enum Section: Int, CaseIterable {
        case header
        case books
    }

    private enum Item: Hashable {
        case header
        case book(String)
        case emptyShelf(String)
        case emptyLibrary
    }

    /// Target grid cell width, from which the column count is derived:
    /// the Tonight-shelf tile on phones, an airier tile on tablets.
    private static let gridCellTargetPhone: CGFloat = 104
    private static let gridCellTargetTablet: CGFloat = 250

    /// Picks the tile target from the device's smaller screen dimension —
    /// the same sw600dp convention Android uses — so an iPad is airy in
    /// both orientations while an iPhone Max stays dense in landscape
    /// (which trait size-classes would get wrong: it is regular-H there).
    private func gridCellTarget() -> CGFloat {
        let screen = view.window?.windowScene?.screen.bounds.size ?? view.bounds.size
        return min(screen.width, screen.height) >= 600
            ? Self.gridCellTargetTablet
            : Self.gridCellTargetPhone
    }

    private var collectionView: UICollectionView!
    private var dataSource: UICollectionViewDiffableDataSource<Section, Item>!
    /// The header content, built once and hosted by whichever cell shows
    /// it, so the search field keeps its text and focus across reuse.
    private let headerStack = UIStackView()
    private var viewToggle: InkIconButton!

    private var segment: Segment = .reading
    private var query = ""

    /// Whether the books section currently lays out as a grid. Search
    /// results always render as list rows regardless of the stored mode.
    private var gridActive = false

    /// Rows currently on screen, in the order the core returned them.
    private var publications: [Publication] = []
    private var publicationsByID: [String: Publication] = [:]
    /// The in-flight fetch. Held so a fast typist's keystrokes cancel their
    /// predecessors instead of racing each other onto the list.
    private var reloadTask: Task<Void, Never>?
    /// `nonisolated(unsafe)` so the nonisolated `deinit` can unregister it.
    /// Only ever touched on the main actor: assigned in `viewDidLoad`, read
    /// once at deinit, when no other reference to this screen survives.
    nonisolated(unsafe) private var libraryDidChangeObserver: NSObjectProtocol?

    deinit {
        if let libraryDidChangeObserver {
            NotificationCenter.default.removeObserver(libraryDidChangeObserver)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        buildHeader()
        buildCollectionView()

        // An import from anywhere — this screen's picker, "Open with
        // Inkuna", the share sheet — refreshes the list.
        libraryDidChangeObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reloadList() }
        }

        reloadList()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // Progress moves while the reader is open; the rows behind it are
        // stale by the time the reader is popped.
        reloadList()
    }

    // MARK: Header

    private func buildHeader() {
        let title = InkLabel()
        title.text = String(localized: "library_title", defaultValue: "Library")
        title.font = InkFont.display
        title.textColor = InkColor.textDisplay
        title.numberOfLines = 0

        // The view toggle and import affordance ride beside the title:
        // this screen hides the navigation bar, so there is no bar button
        // item to put them in.
        viewToggle = InkIconButton(symbol: "square.grid.2x2", accessibilityLabel: "") { [weak self] in
            self?.toggleViewMode()
        }
        applyToggleAppearance()
        let addButton = InkIconButton(symbol: "plus", accessibilityLabel: String(localized: "import_add_books", defaultValue: "Add books")) { [weak self] in
            guard let self else { return }
            ImportFlow.presentPicker(from: self)
        }
        let titleRow = UIStackView(arrangedSubviews: [title, UIView(), viewToggle, addButton])
        titleRow.axis = .horizontal
        titleRow.alignment = .center
        titleRow.spacing = InkSpacing.space1

        let searchField = InkSearchField(placeholder: String(localized: "library_search_placeholder", defaultValue: "Search your library"))
        searchField.onTextChange = { [weak self] text in
            self?.query = text
            self?.reloadList()
        }

        let segments = Segment.allCases
        let segmented = InkSegmentedControl(
            options: segments.map(\.title),
            selectedIndex: segment.rawValue
        )
        segmented.onSelectIndex = { [weak self] index in
            guard let self, let chosen = Segment(rawValue: index) else { return }
            self.segment = chosen
            self.reloadList()
        }
        let segmentRow = UIStackView(arrangedSubviews: [segmented, UIView()])
        segmentRow.axis = .horizontal

        headerStack.axis = .vertical
        headerStack.addArrangedSubview(titleRow)
        headerStack.setCustomSpacing(InkSpacing.space5, after: titleRow)
        headerStack.addArrangedSubview(searchField)
        headerStack.setCustomSpacing(InkSpacing.space4, after: searchField)
        headerStack.addArrangedSubview(segmentRow)
    }

    /// Flips the persisted mode and re-renders in place. The stored choice
    /// applies to every segment; an active search stays a list either way.
    private func toggleViewMode() {
        AppSettings.shared.libraryGridEnabled.toggle()
        applyToggleAppearance()
        applySnapshot(recomputingMode: true)
    }

    private func applyToggleAppearance() {
        let grid = AppSettings.shared.libraryGridEnabled
        // The button shows the mode a tap switches to.
        viewToggle.symbol = grid ? "list.bullet" : "square.grid.2x2"
        viewToggle.accessibilityLabel = grid
            ? String(localized: "library_view_list", defaultValue: "Show as list")
            : String(localized: "library_view_grid", defaultValue: "Show as grid")
    }

    // MARK: Collection view

    private func buildCollectionView() {
        collectionView = UICollectionView(frame: .zero, collectionViewLayout: makeLayout())
        collectionView.backgroundColor = .clear
        collectionView.alwaysBounceVertical = true
        collectionView.showsVerticalScrollIndicator = false
        collectionView.keyboardDismissMode = .interactive
        collectionView.delegate = self
        collectionView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(collectionView)
        NSLayoutConstraint.activate([
            collectionView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            collectionView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            collectionView.topAnchor.constraint(equalTo: view.topAnchor),
            collectionView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        // A tap on quiet space puts the keyboard away; the delegate keeps
        // taps on the text field itself from bouncing the keyboard.
        let tap = UITapGestureRecognizer(target: self, action: #selector(dismissKeyboard))
        tap.cancelsTouchesInView = false
        tap.delegate = self
        collectionView.addGestureRecognizer(tap)

        let headerRegistration = UICollectionView.CellRegistration<HostCell, Item> { [weak self] cell, _, _ in
            guard let self else { return }
            cell.host(self.headerStack)
        }
        let listRegistration = UICollectionView.CellRegistration<BookListCell, Item> { [weak self] cell, _, item in
            guard let self, case .book(let id) = item, let publication = self.publicationsByID[id] else { return }
            cell.configure(publication: publication)
        }
        let gridRegistration = UICollectionView.CellRegistration<BookGridCell, Item> { [weak self] cell, _, item in
            guard let self, case .book(let id) = item, let publication = self.publicationsByID[id] else { return }
            cell.configure(publication: publication)
        }
        let emptyRegistration = UICollectionView.CellRegistration<HostCell, Item> { [weak self] cell, _, item in
            guard let self else { return }
            switch item {
            case .emptyShelf(let message):
                cell.host(self.paddedEmptyState(message))
            case .emptyLibrary:
                let invitation = EmptyLibraryView.inviting(self)
                cell.host(invitation)
                invitation.appear()
            default:
                break
            }
        }

        dataSource = UICollectionViewDiffableDataSource<Section, Item>(collectionView: collectionView) { [weak self] collectionView, indexPath, item in
            switch item {
            case .header:
                collectionView.dequeueConfiguredReusableCell(using: headerRegistration, for: indexPath, item: item)
            case .book:
                self?.gridActive == true
                    ? collectionView.dequeueConfiguredReusableCell(using: gridRegistration, for: indexPath, item: item)
                    : collectionView.dequeueConfiguredReusableCell(using: listRegistration, for: indexPath, item: item)
            case .emptyShelf, .emptyLibrary:
                collectionView.dequeueConfiguredReusableCell(using: emptyRegistration, for: indexPath, item: item)
            }
        }
    }

    /// The section provider re-runs whenever the container's size changes,
    /// so the grid's column count follows rotation and window resizes with
    /// no further bookkeeping.
    private func makeLayout() -> UICollectionViewLayout {
        UICollectionViewCompositionalLayout { [weak self] sectionIndex, environment in
            guard let self, let section = Section(rawValue: sectionIndex) else { return nil }
            switch section {
            case .header:
                return Self.headerSection()
            case .books:
                return self.gridActive
                    ? Self.gridSection(environment: environment, cellTarget: self.gridCellTarget())
                    : Self.listSection()
            }
        }
    }

    private static func headerSection() -> NSCollectionLayoutSection {
        let size = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .estimated(160)
        )
        let item = NSCollectionLayoutItem(layoutSize: size)
        let group = NSCollectionLayoutGroup.vertical(layoutSize: size, subitems: [item])
        let section = NSCollectionLayoutSection(group: group)
        section.contentInsets = NSDirectionalEdgeInsets(
            top: InkSpacing.space6,
            leading: InkSpacing.pageMargin,
            bottom: InkSpacing.space2,
            trailing: InkSpacing.pageMargin
        )
        return section
    }

    private static func listSection() -> NSCollectionLayoutSection {
        let size = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .estimated(84)
        )
        let item = NSCollectionLayoutItem(layoutSize: size)
        let group = NSCollectionLayoutGroup.vertical(layoutSize: size, subitems: [item])
        let section = NSCollectionLayoutSection(group: group)
        section.contentInsets = NSDirectionalEdgeInsets(
            top: 0,
            leading: InkSpacing.pageMargin,
            bottom: InkSpacing.space8,
            trailing: InkSpacing.pageMargin
        )
        return section
    }

    private static func gridSection(environment: NSCollectionLayoutEnvironment, cellTarget: CGFloat) -> NSCollectionLayoutSection {
        let gap = InkSpacing.stackGap
        let available = environment.container.effectiveContentSize.width - 2 * InkSpacing.pageMargin
        // As many target-width tiles as fit, but never fewer than two columns.
        let columns = max(2, Int((available + gap) / (cellTarget + gap)))
        let itemSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1 / CGFloat(columns)),
            heightDimension: .estimated(210)
        )
        let item = NSCollectionLayoutItem(layoutSize: itemSize)
        let groupSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .estimated(210)
        )
        let group = NSCollectionLayoutGroup.horizontal(layoutSize: groupSize, repeatingSubitem: item, count: columns)
        group.interItemSpacing = .fixed(gap)
        let section = NSCollectionLayoutSection(group: group)
        section.interGroupSpacing = InkSpacing.space5
        // No top inset: the header section's bottom inset already provides
        // the 8pt gap, matching list mode and Android exactly.
        section.contentInsets = NSDirectionalEdgeInsets(
            top: 0,
            leading: InkSpacing.pageMargin,
            bottom: InkSpacing.space8,
            trailing: InkSpacing.pageMargin
        )
        return section
    }

    // MARK: Data

    /// Fetches the current segment from the core and rebuilds the list.
    ///
    /// Every core call is awaited, so the main thread is never blocked on
    /// SQLite — the list simply keeps its previous rows until the new ones
    /// arrive, which reads as instant for any realistic library.
    private func reloadList() {
        reloadTask?.cancel()

        // TODO(core): no Wishlist shelf exists yet — file-less publications
        // are deferred to their own spec, so the segment stays empty.
        guard segment != .wishlist else {
            publications = []
            render(rows: [], emptiness: .shelf(String(localized: "library_empty_wishlist", defaultValue: "Nothing on the nightstand yet.")))
            return
        }

        // Unfinished, not Reading: a book must be listed the moment it is
        // imported, and Reading deliberately means "opened at least once".
        let shelf: Shelf = segment == .finished ? .finished : .unfinished
        let trimmed = query.trimmingCharacters(in: .whitespaces)

        reloadTask = Task { [weak self] in
            do {
                // Typing is not a query. A keystroke's reload waits out the
                // burst before touching the core; the next keystroke cancels
                // this sleep, so a fast typist costs one search, not eight.
                // Every reload with a query present waits — only an empty
                // field (appearing, switching segments, library changes
                // included) repaints at once.
                if !trimmed.isEmpty {
                    try await Task.sleep(for: .milliseconds(200))
                }
                let bookshelf = try await LibraryStore.shared.library()
                let rows: [Publication]
                if trimmed.isEmpty {
                    rows = try await bookshelf.library().list(shelf: shelf, sort: .recentlyOpened)
                } else {
                    // Metadata search is the core's, CJK-aware segmentation
                    // and all. The shelf still filters the result, so a
                    // search inside Finished stays inside Finished.
                    let matches = try await bookshelf.library().searchLibrary(query: trimmed)
                    let shelved = Set(
                        try await bookshelf.library().list(shelf: shelf, sort: .recentlyOpened).map(\.id)
                    )
                    rows = matches.filter { shelved.contains($0.id) }
                }
                guard !Task.isCancelled else { return }

                // Distinguish "this shelf is empty" from "there are no
                // books at all" — only the latter earns the invitation to
                // import, and it costs a second query only when empty.
                var emptiness: Emptiness = .shelf(
                    shelf == .finished
                        ? String(localized: "library_empty_finished", defaultValue: "Nothing finished yet. No hurry.")
                        : String(localized: "library_empty_reading", defaultValue: "Nothing here yet.")
                )
                if rows.isEmpty {
                    if !trimmed.isEmpty {
                        emptiness = .shelf(String(localized: "library_empty_query", defaultValue: "Nothing found in the stacks."))
                    } else if try await bookshelf.library().list(shelf: .all, sort: .recentlyAdded).isEmpty {
                        emptiness = .wholeLibrary
                    }
                }
                guard !Task.isCancelled else { return }
                self?.render(rows: rows, emptiness: emptiness, searching: !trimmed.isEmpty)
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                // A library that will not open is worth saying plainly
                // rather than showing as an empty shelf.
                self?.render(rows: [], emptiness: .shelf(String(localized: "library_unopenable", defaultValue: "The library couldn't be opened.")))
            }
        }
    }

    /// What to show when there is nothing to list.
    private enum Emptiness {
        /// This shelf or search has no rows, but the library has books.
        case shelf(String)
        /// The library holds no books at all.
        case wholeLibrary
    }

    private var emptiness: Emptiness = .shelf("")

    private func render(rows: [Publication], emptiness: Emptiness, searching: Bool = false) {
        publications = rows
        publicationsByID = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
        self.emptiness = emptiness
        // Search results always read as list rows: scan-friendly, author
        // and progress visible — mode applies to browsing, not finding.
        gridActive = AppSettings.shared.libraryGridEnabled && !searching && !rows.isEmpty
        applySnapshot(recomputingMode: false)
    }

    /// The `gridActive` value the last-applied snapshot was rendered with.
    /// Whenever a new apply disagrees, every carried book cell must be
    /// re-dequeued into the other cell class — no matter which caller
    /// triggered the apply (the toggle, or a search flipping the mode).
    private var appliedGridActive: Bool?

    /// Rebuilds the snapshot from the current state. `recomputingMode`
    /// re-derives `gridActive` from the persisted setting first (the
    /// toggle's path; `render` derives it itself).
    ///
    /// Book cells carried over from the previous snapshot are re-dequeued
    /// (`reloadItems`) whenever the grid/list mode differs from the one
    /// the last snapshot was applied with — required when the same items
    /// must move between list and grid cell classes.
    private func applySnapshot(recomputingMode: Bool) {
        if recomputingMode {
            gridActive = AppSettings.shared.libraryGridEnabled
                && query.trimmingCharacters(in: .whitespaces).isEmpty
                && !publications.isEmpty
        }
        var snapshot = NSDiffableDataSourceSnapshot<Section, Item>()
        snapshot.appendSections(Section.allCases)
        snapshot.appendItems([.header], toSection: .header)
        if publications.isEmpty {
            switch emptiness {
            case .shelf(let message):
                snapshot.appendItems([.emptyShelf(message)], toSection: .books)
            case .wholeLibrary:
                snapshot.appendItems([.emptyLibrary], toSection: .books)
            }
        } else {
            snapshot.appendItems(publications.map { .book($0.id) }, toSection: .books)
        }

        let modeChanged = appliedGridActive != gridActive
        let existing = Set(dataSource.snapshot().itemIdentifiers)
        let carried = snapshot.itemIdentifiers(inSection: .books).filter { existing.contains($0) }
        if modeChanged {
            snapshot.reloadItems(carried)
        } else {
            // Reconfigure in place: the cells keep their cover views — and
            // their decoded art — so the routine viewWillAppear refresh
            // updates text and progress without covers blinking back in.
            snapshot.reconfigureItems(carried)
        }
        dataSource.apply(snapshot, animatingDifferences: false)
        if modeChanged {
            // The section layout must re-prepare for the other mode even
            // when the diff itself is empty (every visible book carried).
            collectionView.collectionViewLayout.invalidateLayout()
        }
        appliedGridActive = gridActive
    }

    @objc private func dismissKeyboard() {
        view.endEditing(true)
    }

    // MARK: Shared building blocks (mirrors ScrollScreenViewController)

    /// Empty-state message with vertical breathing room, for list bodies.
    private func paddedEmptyState(_ text: String) -> UIView {
        let label = InkLabel()
        label.text = text
        label.font = InkFont.reading()
        label.textColor = InkColor.textTertiary
        label.textAlignment = .center
        label.numberOfLines = 0
        let wrapper = UIStackView(arrangedSubviews: [label])
        wrapper.axis = .vertical
        wrapper.isLayoutMarginsRelativeArrangement = true
        wrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space12, left: 0, bottom: InkSpacing.space12, right: 0)
        return wrapper
    }

    /// Full-page destinations slide the native tab bar away.
    private func openBook(_ publication: Publication) {
        let detail = BookDetailViewController(publication: publication)
        detail.hidesBottomBarWhenPushed = true
        navigationController?.pushViewController(detail, animated: true)
    }
}

// MARK: - Delegate

extension LibraryViewController: UICollectionViewDelegate {
    func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        collectionView.deselectItem(at: indexPath, animated: false)
        guard case .book(let id) = dataSource.itemIdentifier(for: indexPath),
              let publication = publicationsByID[id] else { return }
        openBook(publication)
    }

    func collectionView(_ collectionView: UICollectionView, shouldHighlightItemAt indexPath: IndexPath) -> Bool {
        if case .book = dataSource.itemIdentifier(for: indexPath) { return true }
        return false
    }
}

extension LibraryViewController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // Tapping into a text field must focus it, not fight the dismissal.
        var view = touch.view
        while let current = view {
            if current is UITextField { return false }
            view = current.superview
        }
        return true
    }
}

// MARK: - Cells

/// A cell that hosts an externally owned view — the header keeps its
/// search field (text, focus) alive across cell reuse because the view
/// itself is retained by the screen, not the cell.
private final class HostCell: UICollectionViewCell {
    private(set) var hosted: UIView?

    func host(_ view: UIView) {
        guard hosted !== view else { return }
        hosted?.removeFromSuperview()
        contentView.subviews.forEach { $0.removeFromSuperview() }
        hosted = view
        view.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            view.topAnchor.constraint(equalTo: contentView.topAnchor),
            view.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }
}

/// List mode: the existing design-system row, unchanged.
private final class BookListCell: UICollectionViewCell {
    private let row = BookListRowView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        row.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            row.topAnchor.constraint(equalTo: contentView.topAnchor),
            row.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func configure(publication: Publication) {
        row.configure(
            title: publication.title,
            author: publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author")),
            progress: publication.progression > 0 ? CGFloat(publication.progression) : nil,
            seed: BookCoverView.coverSeed(for: publication.id),
            coverPath: publication.coverPath,
            // The core owns every book's file, so a listed book is
            // always on disk — there is no cloud-only state to badge.
            downloaded: true
        )
    }

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            InkMotion.runQuiet(duration: InkMotion.fast) {
                self.contentView.alpha = pressed ? 0.7 : 1
            }
        }
    }
}

/// Grid mode: cover and title only — the clean bookshelf look. The cover
/// takes the cell's full width; `BookCoverView` enforces the 2:3 aspect
/// and picks its decode bucket from that width.
private final class BookGridCell: UICollectionViewCell {
    private let coverContainer = UIView()
    private let titleLabel = InkLabel()
    /// What the current cover view was built from, so reconfiguring with
    /// the same book keeps the decoded art in place (no placeholder flash).
    private var coverIdentity: (title: String, author: String, seed: Int, coverPath: String?)?

    override init(frame: CGRect) {
        super.init(frame: frame)

        titleLabel.font = InkFont.serif(14, weight: .medium, style: .footnote)
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 2

        let stack = UIStackView(arrangedSubviews: [coverContainer, titleLabel])
        stack.axis = .vertical
        stack.alignment = .fill
        stack.spacing = 9
        stack.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            stack.topAnchor.constraint(equalTo: contentView.topAnchor),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: contentView.bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func configure(publication: Publication) {
        let author = publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author"))
        titleLabel.text = publication.title

        // Cover and title only on screen, but VoiceOver still hears the
        // author — the grid trims chrome, not information.
        isAccessibilityElement = true
        let bookFormat = NSLocalizedString("a11y_book_row", comment: "")
        accessibilityLabel = String.localizedStringWithFormat(bookFormat, publication.title, author)
        accessibilityTraits = .button

        let identity = (title: publication.title, author: author, seed: BookCoverView.coverSeed(for: publication.id), coverPath: publication.coverPath)
        if coverIdentity == nil || coverIdentity! != identity {
            coverIdentity = identity
            coverContainer.subviews.forEach { $0.removeFromSuperview() }
            let cover = BookCoverView(title: identity.title, author: identity.author, seed: identity.seed, coverPath: identity.coverPath)
            cover.isUserInteractionEnabled = false
            cover.translatesAutoresizingMaskIntoConstraints = false
            coverContainer.addSubview(cover)
            NSLayoutConstraint.activate([
                cover.leadingAnchor.constraint(equalTo: coverContainer.leadingAnchor),
                cover.trailingAnchor.constraint(equalTo: coverContainer.trailingAnchor),
                cover.topAnchor.constraint(equalTo: coverContainer.topAnchor),
                cover.bottomAnchor.constraint(equalTo: coverContainer.bottomAnchor),
            ])
        }
    }

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            InkMotion.runQuiet(duration: InkMotion.fast) {
                self.contentView.alpha = pressed ? 0.7 : 1
            }
        }
    }
}
