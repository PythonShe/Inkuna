import UIKit

/// Draws core-provided selection geometry and routes only handle touches.
@MainActor
final class SelectionOverlayView: UIView {
    enum Handle: Hashable {
        case start
        case end
    }

    var onHandlePan: ((Handle, UIGestureRecognizer.State, CGPoint) -> Void)?

    private let knobDiameter: CGFloat = 12
    private let stemWidth: CGFloat = 2
    private let handleHitInset: CGFloat = 16
    private var highlightRects: [CGRect] = []
    private var writingMode: WritingMode = .horizontalTb
    private var accentColor = UIColor(ink: 0xB4863B)
    private var activeHandle: Handle?

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
        highlightRects = rects.compactMap { selectionRect in
            let rect = selectionRect.rect
            guard rect.width > 0, rect.height > 0 else { return nil }
            let pageLocalRect = CGRect(
                x: rect.x,
                y: bounds.height - rect.y - rect.height,
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
        highlightRects.removeAll()
        isHidden = true
        setNeedsDisplay()
    }

    func containsHighlight(_ point: CGPoint) -> Bool {
        highlightRects.contains { $0.contains(point) }
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard !isHidden, alpha > 0, isUserInteractionEnabled, handle(at: point) != nil else {
            return nil
        }
        return self
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext(), !highlightRects.isEmpty else { return }

        context.setFillColor(accentColor.withAlphaComponent(0.30).cgColor)
        highlightRects.forEach { context.fill($0) }

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
