import os
import UIKit

/// Native selection UI over the engine's synchronous page geometry.
@MainActor
final class ReaderSelectionController: NSObject, @MainActor UIEditMenuInteractionDelegate {
    private struct ActiveSelection {
        let spineIdx: UInt32
        let pageIdx: UInt32
        let pageRange: CharRange
        var range: CharRange
    }

    private struct HandleDrag {
        let anchor: UInt64
    }

    private let session: ReaderSession
    private let canvas: EnginePageCanvas
    private let surface: EnginePagerSurface
    private weak var presenter: UIViewController?
    private let overlay = SelectionOverlayView()
    private var editMenuInteraction: UIEditMenuInteraction!
    private let selectionFeedback = UIImpactFeedbackGenerator(style: .light)
    private let logger = Logger(subsystem: "app.inkuna.ios", category: "reader-selection")
    private var searchHighlightAnimator: UIViewPropertyAnimator?
    private var searchHighlightToken = 0

    private var selection: ActiveSelection? {
        didSet {
            let active = selection != nil
            surface.selectionActive = active
            canvas.canCopySelection = active
        }
    }
    private var handleDrag: HandleDrag?

    var isActive: Bool { selection != nil }

    init(
        session: ReaderSession,
        canvas: EnginePageCanvas,
        surface: EnginePagerSurface,
        presenter: UIViewController
    ) {
        self.session = session
        self.canvas = canvas
        self.surface = surface
        self.presenter = presenter
        super.init()

        editMenuInteraction = UIEditMenuInteraction(delegate: self)
        canvas.addInteraction(editMenuInteraction)
        canvas.selectionCopyHandler = { [weak self] in self?.copySelection() }

        overlay.frame = canvas.bounds
        overlay.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        overlay.isHidden = true
        overlay.onHandlePan = { [weak self] handle, state, point in
            self?.handlePan(handle, state: state, at: point)
        }
        canvas.addSubview(overlay)
        canvas.selectionController = self

        let longPress = UILongPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:)))
        longPress.minimumPressDuration = 0.35
        longPress.cancelsTouchesInView = false
        canvas.addGestureRecognizer(longPress)
        selectionFeedback.prepare()
    }

    func clear() {
        cancelSearchHighlight()
        handleDrag = nil
        selection = nil
        overlay.clear()
        editMenuInteraction.dismissMenu()
        if canvas.isFirstResponder { canvas.resignFirstResponder() }
    }

    func containsSelection(at canvasPoint: CGPoint) -> Bool {
        guard isActive else { return false }
        return overlay.containsHighlight(canvasPoint)
    }

    func updateTheme() {
        overlay.updateAccentColor(selectionAccent)
    }

    func showSearchHighlight(_ rects: [SelectionRect]) {
        clear()
        guard !rects.isEmpty else { return }

        canvas.bringSubviewToFront(overlay)
        overlay.showSearchHighlight(rects: rects, accentColor: searchHighlightAccent)
        searchHighlightToken &+= 1
        let token = searchHighlightToken
        let animator = UIViewPropertyAnimator(duration: SelectionOverlayView.searchHighlightFade, curve: .easeInOut) { [weak overlay] in
            overlay?.alpha = 0
        }
        animator.addCompletion { [weak self] _ in
            guard self?.searchHighlightToken == token else { return }
            self?.overlay.alpha = 1
            self?.overlay.clear()
            self?.searchHighlightAnimator = nil
        }
        animator.startAnimation(afterDelay: SelectionOverlayView.searchHighlightHold)
        searchHighlightAnimator = animator
    }

    private func cancelSearchHighlight() {
        searchHighlightToken &+= 1
        searchHighlightAnimator?.stopAnimation(true)
        searchHighlightAnimator = nil
        overlay.alpha = 1
    }

    @objc private func handleLongPress(_ recognizer: UILongPressGestureRecognizer) {
        guard recognizer.state == .began, !isActive else { return }
        cancelSearchHighlight()
        overlay.clear()
        let canvasPoint = recognizer.location(in: canvas)
        let spineIdx = surface.spineIdx
        let pageIdx = surface.pageIdx
        guard let pagePoint = surface.pagePoint(fromCanvasPoint: canvasPoint) else { return }

        do {
            let pageRange = try session.pageCharRange(spineIdx: spineIdx, pageIdx: pageIdx)
            let hit = try session.hitTest(spineIdx: spineIdx, pageIdx: pageIdx, x: pagePoint.x, y: pagePoint.y)
            guard hit.coordinate.spineIdx == spineIdx else { return }
            let wordRange = clamp(try session.wordAt(coordinate: hit.coordinate), to: pageRange)
            guard wordRange.start < wordRange.end else { return }

            let rects = try session.selectionRects(spineIdx: spineIdx, range: wordRange)
            guard !rects.isEmpty else { return }
            selection = ActiveSelection(
                spineIdx: spineIdx,
                pageIdx: pageIdx,
                pageRange: pageRange,
                range: wordRange
            )
            canvas.bringSubviewToFront(overlay)
            overlay.show(rects: rects, accentColor: selectionAccent)
            selectionFeedback.impactOccurred()
            presentMenu()
        } catch {
            // Geometry calls are cache-only. A not-ready page is simply not selectable yet.
        }
    }

    private func handlePan(
        _ handle: SelectionOverlayView.Handle,
        state: UIGestureRecognizer.State,
        at canvasPoint: CGPoint
    ) {
        switch state {
        case .began:
            guard let selection else { return }
            handleDrag = HandleDrag(anchor: handle == .start ? selection.range.end : selection.range.start)
            editMenuInteraction.dismissMenu()
        case .changed:
            updateDraggedBoundary(at: canvasPoint)
        case .ended:
            updateDraggedBoundary(at: canvasPoint)
            handleDrag = nil
            presentMenu()
        case .cancelled, .failed:
            handleDrag = nil
        default:
            break
        }
    }

    private func updateDraggedBoundary(at canvasPoint: CGPoint) {
        guard let selection, let handleDrag,
              let pagePoint = surface.pagePoint(fromCanvasPoint: canvasPoint) else { return }

        do {
            let hit = try session.hitTest(
                spineIdx: selection.spineIdx,
                pageIdx: selection.pageIdx,
                x: pagePoint.x,
                y: pagePoint.y
            )
            guard hit.coordinate.spineIdx == selection.spineIdx else { return }
            let boundary = min(max(hit.coordinate.charOffset, selection.pageRange.start), selection.pageRange.end)
            guard boundary != handleDrag.anchor else { return }

            let range = CharRange(
                start: min(boundary, handleDrag.anchor),
                end: max(boundary, handleDrag.anchor)
            )
            let rects = try session.selectionRects(spineIdx: selection.spineIdx, range: range)
            guard !rects.isEmpty else { return }

            self.selection?.range = range
            overlay.show(rects: rects, accentColor: selectionAccent)
        } catch {
            // Keep the previous geometry visible while the new page snapshot is unavailable.
        }
    }

    private func presentMenu() {
        guard isActive, !overlay.selectionBounds.isNull else { return }
        canvas.becomeFirstResponder()
        let sourcePoint = CGPoint(x: overlay.selectionBounds.midX, y: overlay.selectionBounds.midY)
        editMenuInteraction.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: sourcePoint))
    }

    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        guard isActive else { return nil }

        let lookUp = UIAction(
            title: String(localized: "reader_look_up", defaultValue: "Look Up"),
            image: UIImage(systemName: "book"),
            identifier: UIAction.Identifier("reader_look_up")
        ) { [weak self] _ in
            self?.lookUpSelection()
        }
        let share = UIAction(
            title: String(localized: "reader_share_selection", defaultValue: "Share"),
            image: UIImage(systemName: "square.and.arrow.up"),
            identifier: UIAction.Identifier("reader_share_selection")
        ) { [weak self] _ in
            self?.shareSelection()
        }
        // `suggestedActions` carries Copy and nothing else — the canvas
        // validates only `copy:` (see `EnginePageCanvas`), so the entries
        // below are the reader's single Look Up and Share.
        return UIMenu(children: suggestedActions + [lookUp, share])
    }

    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        targetRectFor configuration: UIEditMenuConfiguration
    ) -> CGRect {
        overlay.selectionBounds
    }

    private func copySelection() {
        guard let text = selectedText() else { return }
        UIPasteboard.general.string = text
    }

    private func lookUpSelection() {
        guard let presenter, let text = selectedText() else { return }
        presenter.present(UIReferenceLibraryViewController(term: text), animated: true)
    }

    private func shareSelection() {
        guard let presenter, let text = selectedText() else { return }
        let activity = UIActivityViewController(activityItems: [text], applicationActivities: nil)
        activity.popoverPresentationController?.sourceView = canvas
        activity.popoverPresentationController?.sourceRect = overlay.selectionBounds
        presenter.present(activity, animated: true)
    }

    private func selectedText() -> String? {
        guard let selection else { return nil }
        do {
            return try session.textRange(spineIdx: selection.spineIdx, range: selection.range)
        } catch {
            logger.warning("Reading selected text failed: \(error)")
            return nil
        }
    }

    private func clamp(_ range: CharRange, to pageRange: CharRange) -> CharRange {
        CharRange(
            start: min(max(range.start, pageRange.start), pageRange.end),
            end: min(max(range.end, pageRange.start), pageRange.end)
        )
    }

    private var selectionAccent: UIColor {
        UIColor(ink: canvas.theme.isNight ? 0xD9AE63 : 0xB4863B)
    }

    private var searchHighlightAccent: UIColor {
        UIColor(ink: canvas.theme.isNight
            ? SelectionOverlayView.searchHighlightNightColor
            : SelectionOverlayView.searchHighlightDayColor)
    }
}
