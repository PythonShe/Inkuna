import UIKit

/// The current page strip. Coordinates are engine layout points at 1×.
struct PageScene {
    var spineIdx: UInt32
    var pageCount: UInt32
    var rtl: Bool
    var innerOffset: CGFloat
    var outerDisplacement: CGFloat
    var neighborEdge: (spineIdx: UInt32, pageIdx: UInt32, toRight: Bool)?
}

/// Mounts the small visible window of engine pages and positions its strip.
@MainActor
final class EnginePageCanvas: UIView {
    private struct PageKey: Hashable {
        let spineIdx: UInt32
        let pageIdx: UInt32
    }

    private struct MountedPage {
        let view: PageView
        var generation: UInt64?
        var use: UInt64
    }

    private let session: ReaderSession
    private let imageProvider: PageImageProvider
    private var mounted: [PageKey: MountedPage] = [:]
    private var scene: PageScene?
    private var latestGeneration: UInt64?
    private var useCounter: UInt64 = 0
    private let unreadableLabel = InkLabel()
    private var truncationNotice: UIView?

    var theme: ReadingTheme {
        didSet {
            mounted.values.forEach { $0.view.theme = theme }
            unreadableLabel.textColor = theme.foreground
            backgroundColor = theme.background
        }
    }

    var onTap: ((CGPoint) -> Void)?
    var onPageDrawn: ((UInt32, UInt32) -> Void)?
    /// A page's accessibility link activation, in page-local layout points.
    var onLinkActivated: ((UInt32, UInt32, CGFloat, CGFloat) -> Void)?
    var selectionCopyHandler: (() -> Void)?
    weak var selectionController: ReaderSelectionController?
    var canCopySelection = false
    var isLaidOut: Bool { bounds.width > 0 && bounds.height > 0 }

    init(session: ReaderSession, theme: ReadingTheme) {
        self.session = session
        self.theme = theme
        imageProvider = PageImageProvider(session: session)
        super.init(frame: .zero)
        backgroundColor = theme.background
        isOpaque = true

        unreadableLabel.text = String(
            localized: "reader_chapter_unreadable",
            defaultValue: "This chapter can’t be displayed."
        )
        unreadableLabel.font = InkFont.reading()
        unreadableLabel.textAlignment = .center
        unreadableLabel.numberOfLines = 0
        unreadableLabel.textColor = theme.foreground
        unreadableLabel.isHidden = true
        addSubview(unreadableLabel)

        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(didTap(_:))))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func setScene(_ scene: PageScene) {
        self.scene = scene
        updatePages()
    }

    /// Removes stale display lists, then re-queries the visible window.
    func invalidate(generation: UInt64) {
        latestGeneration = generation
        let stale = mounted.compactMap { key, page in
            page.generation == generation ? nil : key
        }
        for key in stale {
            guard let page = mounted[key] else { continue }
            page.view.removeFromSuperview()
            mounted[key] = nil
        }
        updatePages()
    }

    /// Relayout has no synchronous generation in the current bindings.
    func invalidateAll() {
        // No display list is admissible until the next engine event latches
        // its generation through `invalidate(generation:)`.
        latestGeneration = nil
        mounted.values.forEach { $0.view.removeFromSuperview() }
        mounted.removeAll()
        updatePages()
    }

    func showUnreadablePlaceholder(_ visible: Bool) {
        unreadableLabel.isHidden = !visible
        if visible {
            mounted.values.forEach { $0.view.isHidden = true }
        } else {
            updatePages()
        }
    }

    /// A completed chapter can retain its laid-out prefix while reporting a
    /// budget truncation. This stays up until the reader explicitly closes
    /// it, but its caller only asks once for each spine slot.
    func showTruncationNotice() {
        let notice = truncationNotice ?? makeTruncationNotice()
        notice.isHidden = false
        bringSubviewToFront(notice)
    }

    /// The selection controller owns the shared page-local rect renderer so
    /// transient search highlights retain the exact horizontal/vertical
    /// geometry conversion used by native selection.
    func showSearchHighlight(_ rects: [SelectionRect]) {
        selectionController?.showSearchHighlight(rects)
    }

    func pagePoint(spineIdx: UInt32, pageIdx: UInt32, from point: CGPoint) -> CGPoint? {
        let key = PageKey(spineIdx: spineIdx, pageIdx: pageIdx)
        guard let page = mounted[key], !page.view.isHidden else { return nil }
        return CGPoint(x: point.x - page.view.frame.minX, y: point.y - page.view.frame.minY)
    }

    override var canBecomeFirstResponder: Bool { true }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(copy(_:)) { return canCopySelection }
        return super.canPerformAction(action, withSender: sender)
    }

    override func copy(_ sender: Any?) {
        selectionCopyHandler?()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        unreadableLabel.frame = bounds.insetBy(dx: 32, dy: 32)
        if let truncationNotice {
            truncationNotice.layer.cornerRadius = truncationNotice.bounds.height / 2
        }
        updatePages()
    }

    @objc private func didTap(_ recognizer: UITapGestureRecognizer) {
        onTap?(recognizer.location(in: self))
    }

    private func updatePages() {
        guard let scene, isLaidOut else { return }

        let width = bounds.width
        let height = bounds.height
        var targets: [(PageKey, CGRect)] = []
        if scene.pageCount > 0 {
            let contentOffset = scene.innerOffset - scene.outerDisplacement
            let firstSlot = max(0, Int(floor(contentOffset / width)))
            let lastSlot = min(
                Int(scene.pageCount) - 1,
                Int(floor((contentOffset + width - .leastNonzeroMagnitude) / width))
            )
            if firstSlot <= lastSlot {
                for slot in firstSlot ... lastSlot {
                    let pageIdx = PageSlot.pageIdx(
                        for: UInt32(slot),
                        pageCount: scene.pageCount,
                        rtl: scene.rtl
                    )
                    let frame = CGRect(
                        x: CGFloat(slot) * width - scene.innerOffset + scene.outerDisplacement,
                        y: 0,
                        width: width,
                        height: height
                    )
                    if frame.intersects(bounds) {
                        targets.append((PageKey(spineIdx: scene.spineIdx, pageIdx: pageIdx), frame))
                    }
                }
            }
        }

        if let edge = scene.neighborEdge {
            let frame = CGRect(
                x: (edge.toRight ? width : -width) + scene.outerDisplacement,
                y: 0,
                width: width,
                height: height
            )
            if frame.intersects(bounds) {
                targets.append((PageKey(spineIdx: edge.spineIdx, pageIdx: edge.pageIdx), frame))
            }
        }

        let targetKeys = Set(targets.map { $0.0 })
        for (key, page) in mounted where !targetKeys.contains(key) {
            page.view.isHidden = true
        }

        for (key, frame) in targets {
            let page = mount(key)
            page.view.frame = frame
            page.view.isHidden = unreadableLabel.isHidden == false
        }
        if let truncationNotice { bringSubviewToFront(truncationNotice) }
    }

    private func mount(_ key: PageKey) -> MountedPage {
        useCounter &+= 1
        if var page = mounted[key] {
            page.use = useCounter
            if page.generation == nil, let list = displayList(for: key) {
                page.generation = list.generation
                page.view.present(list, spineIdx: key.spineIdx, pageIdx: key.pageIdx, session: session)
            }
            mounted[key] = page
            return page
        }

        let view: PageView
        if mounted.count >= 6, let victim = mounted.min(by: { $0.value.use < $1.value.use }) {
            view = victim.value.view
            mounted.removeValue(forKey: victim.key)
        } else {
            view = PageView(frame: .zero)
            view.imageProvider = imageProvider
            view.onDidDraw = { [weak self] spineIdx, pageIdx in
                self?.onPageDrawn?(spineIdx, pageIdx)
            }
            view.onLinkActivated = { [weak self] spineIdx, pageIdx, x, y in
                self?.onLinkActivated?(spineIdx, pageIdx, x, y)
            }
            addSubview(view)
        }

        view.theme = theme
        let list = displayList(for: key)
        view.present(list, spineIdx: key.spineIdx, pageIdx: key.pageIdx, session: session)
        let page = MountedPage(view: view, generation: list?.generation, use: useCounter)
        mounted[key] = page
        return page
    }

    private func displayList(for key: PageKey) -> PageDisplayList? {
        guard let latestGeneration else { return nil }
        guard let list = try? session.page(spineIdx: key.spineIdx, pageIdx: key.pageIdx) else {
            return nil
        }
        guard list.generation == latestGeneration else { return nil }
        return list
    }

    private func makeTruncationNotice() -> UIView {
        let notice = UIView()
        notice.backgroundColor = UIColor(ink: 0x241F17, alpha: 0.92)
        notice.clipsToBounds = true
        notice.translatesAutoresizingMaskIntoConstraints = false

        let label = InkLabel()
        label.text = String(
            localized: "reader_chapter_truncated",
            defaultValue: "This chapter was too large to display completely."
        )
        label.font = InkFont.label
        label.textColor = UIColor(ink: 0xF2EBDD)
        label.numberOfLines = 1
        label.lineBreakMode = .byTruncatingTail

        var configuration = UIButton.Configuration.plain()
        configuration.baseForegroundColor = UIColor(ink: 0xF2EBDD)
        configuration.image = UIImage(
            systemName: "xmark",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 12, weight: .semibold)
        )
        let close = UIButton(configuration: configuration)
        close.accessibilityLabel = String(localized: "a11y_close", defaultValue: "Close")
        close.addAction(UIAction { [weak notice] _ in notice?.isHidden = true }, for: .primaryActionTriggered)

        let stack = UIStackView(arrangedSubviews: [label, close])
        stack.axis = .horizontal
        stack.spacing = 10
        stack.alignment = .center
        stack.translatesAutoresizingMaskIntoConstraints = false
        notice.addSubview(stack)
        addSubview(notice)
        NSLayoutConstraint.activate([
            close.widthAnchor.constraint(equalToConstant: 32),
            close.heightAnchor.constraint(equalToConstant: 32),
            stack.leadingAnchor.constraint(equalTo: notice.leadingAnchor, constant: 18),
            stack.trailingAnchor.constraint(equalTo: notice.trailingAnchor, constant: -10),
            stack.topAnchor.constraint(equalTo: notice.topAnchor, constant: 10),
            stack.bottomAnchor.constraint(equalTo: notice.bottomAnchor, constant: -10),
            notice.centerXAnchor.constraint(equalTo: centerXAnchor),
            notice.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            notice.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 16),
            notice.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -16),
        ])
        truncationNotice = notice
        return notice
    }
}
