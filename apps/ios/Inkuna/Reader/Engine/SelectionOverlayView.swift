import UIKit

/// Draws core-provided selection geometry and routes only handle touches.
@MainActor
final class SelectionOverlayView: UIView {
    static let searchHighlightDayColor: UInt32 = 0xB4863B
    static let searchHighlightNightColor: UInt32 = 0xD9AE63
    static let searchHighlightAlpha: CGFloat = 0.35
    static let searchHighlightCornerRadius: CGFloat = 3
    static let searchHighlightHold: TimeInterval = 1.5
    static let searchHighlightFade: TimeInterval = 0.6

    enum Handle: Hashable {
        case start
        case end
    }

    private enum Presentation: Equatable {
        case selection
        case searchHighlight
    }

    var onHandlePan: ((Handle, UIGestureRecognizer.State, CGPoint) -> Void)?

    private let knobDiameter: CGFloat = 12
    private let stemWidth: CGFloat = 2
    private let handleHitInset: CGFloat = 16
    private var highlightRects: [CGRect] = []
    private var writingMode: WritingMode = .horizontalTb
    private var accentColor = UIColor(ink: 0xB4863B)
    private var activeHandle: Handle?
    private var presentation: Presentation = .selection

    var selectionBounds: CGRect {
        highlightRects.reduce(into: CGRect.null) { $0 = $0.union($1) }
    }

    init() {
        super.init(frame: .zero)
        backgroundColor = .clear
        isOpaque = false
        contentMode = .redraw

        let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
        addGestureRecognizer(pan)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func show(rects: [SelectionRect], accentColor: UIColor) {
        presentation = .selection
        present(rects: rects, accentColor: accentColor)
    }

    /// Search highlights deliberately share selection's page-local rect
    /// conversion and writing-mode bookkeeping; only their presentation is
    /// transient (no handles, a softer rounded fill).
    func showSearchHighlight(rects: [SelectionRect], accentColor: UIColor) {
        presentation = .searchHighlight
        present(rects: rects, accentColor: accentColor)
    }

    /// Core geometry is page coordinates in layout points at 1× with y
    /// growing downward — the same convention UIKit draws in here, since
    /// this view's context is never flipped (unlike `PageView.draw`, whose
    /// flipped CTM its own `pageRect(for:)` cancels). Rects therefore go
    /// straight through, exactly as the Android sibling scales them.
    private func present(rects: [SelectionRect], accentColor: UIColor) {
        highlightRects = rects.compactMap { selectionRect in
            let rect = selectionRect.rect
            guard rect.width > 0, rect.height > 0 else { return nil }
            let pageLocalRect = CGRect(
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height
            )
            let clippedRect = pageLocalRect.intersection(bounds)
            return clippedRect.isNull || clippedRect.isEmpty ? nil : clippedRect
        }
        writingMode = rects.first?.writingMode ?? .horizontalTb
        self.accentColor = accentColor
        isHidden = highlightRects.isEmpty
        setNeedsDisplay()
    }

    func updateAccentColor(_ color: UIColor) {
        accentColor = color
        setNeedsDisplay()
    }

    func clear() {
        activeHandle = nil
        presentation = .selection
        highlightRects.removeAll()
        isHidden = true
        setNeedsDisplay()
    }

    func containsHighlight(_ point: CGPoint) -> Bool {
        highlightRects.contains { $0.contains(point) }
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard presentation == .selection, !isHidden, alpha > 0,
              isUserInteractionEnabled, handle(at: point) != nil else {
            return nil
        }
        return self
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext(), !highlightRects.isEmpty else { return }

        switch presentation {
        case .selection:
            context.setFillColor(accentColor.withAlphaComponent(0.30).cgColor)
            highlightRects.forEach { context.fill($0) }
        case .searchHighlight:
            context.setFillColor(accentColor.withAlphaComponent(Self.searchHighlightAlpha).cgColor)
            highlightRects.forEach { rect in
                context.addPath(UIBezierPath(roundedRect: rect, cornerRadius: Self.searchHighlightCornerRadius).cgPath)
                context.fillPath()
            }
            return
        }

        context.setStrokeColor(accentColor.cgColor)
        context.setFillColor(accentColor.cgColor)
        context.setLineWidth(stemWidth)
        context.setLineCap(.round)

        for handle in [Handle.start, .end] {
            guard let geometry = handleGeometry(for: handle) else { continue }
            context.move(to: geometry.anchor)
            context.addLine(to: geometry.knobCenter)
            context.strokePath()
            context.fillEllipse(in: knobFrame(center: geometry.knobCenter))
        }
    }

    @objc private func handlePan(_ recognizer: UIPanGestureRecognizer) {
        let point = recognizer.location(in: self)
        switch recognizer.state {
        case .began:
            activeHandle = handle(at: point)
        case .ended, .cancelled, .failed:
            defer { activeHandle = nil }
            guard let activeHandle else { return }
            onHandlePan?(activeHandle, recognizer.state, point)
        default:
            guard let activeHandle else { return }
            onHandlePan?(activeHandle, recognizer.state, point)
        }

        if recognizer.state == .began, let activeHandle {
            onHandlePan?(activeHandle, recognizer.state, point)
        }
    }

    private func handle(at point: CGPoint) -> Handle? {
        for handle in [Handle.start, .end] {
            guard let geometry = handleGeometry(for: handle) else { continue }
            if knobFrame(center: geometry.knobCenter)
                .insetBy(dx: -handleHitInset, dy: -handleHitInset)
                .contains(point) {
                return handle
            }
        }
        return nil
    }

    private func handleGeometry(for handle: Handle) -> (anchor: CGPoint, knobCenter: CGPoint)? {
        guard let first = highlightRects.first, let last = highlightRects.last else { return nil }
        let offset = knobDiameter

        switch (writingMode, handle) {
        case (.horizontalTb, .start):
            let anchor = CGPoint(x: first.minX, y: first.minY)
            return (anchor, CGPoint(x: anchor.x, y: anchor.y - offset))
        case (.horizontalTb, .end):
            let anchor = CGPoint(x: last.maxX, y: last.maxY)
            return (anchor, CGPoint(x: anchor.x, y: anchor.y + offset))
        case (.verticalRl, .start):
            let anchor = CGPoint(x: first.maxX, y: first.minY)
            return (anchor, CGPoint(x: anchor.x + offset, y: anchor.y))
        case (.verticalRl, .end):
            let anchor = CGPoint(x: last.minX, y: last.maxY)
            return (anchor, CGPoint(x: anchor.x - offset, y: anchor.y))
        }
    }

    private func knobFrame(center: CGPoint) -> CGRect {
        CGRect(
            x: center.x - knobDiameter / 2,
            y: center.y - knobDiameter / 2,
            width: knobDiameter,
            height: knobDiameter
        )
    }
}
