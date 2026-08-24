import CoreText
import UIKit

/// A static Core Text rendering surface for one engine page display list.
final class PageView: UIView {
    var theme: ReadingTheme {
        didSet {
            backgroundColor = theme.background
            setNeedsDisplay()
        }
    }

    var imageProvider: PageImageProvider?
    var onDidDraw: ((UInt32, UInt32) -> Void)?
    /// Set by the canvas; accessibility link activation enters the same
    /// page-local path a tap takes.
    var onLinkActivated: ((UInt32, UInt32, CGFloat, CGFloat) -> Void)?

    private var displayList: PageDisplayList?
    private var spineIdx: UInt32?
    private var pageIdx: UInt32?
    /// The session whose display list is mounted: the font store's cache is
    /// bound to a session, so a stale page of the previous book can never
    /// draw the next book's face under a colliding publisher font id.
    private weak var ownerSession: AnyObject?

    override init(frame: CGRect) {
        theme = .paper
        super.init(frame: frame)
        configure()
    }

    required init?(coder: NSCoder) {
        theme = .paper
        super.init(coder: coder)
        configure()
    }

    func present(_ list: PageDisplayList?, spineIdx: UInt32, pageIdx: UInt32, session: ReaderSession) {
        displayList = list
        ownerSession = session
        self.spineIdx = spineIdx
        self.pageIdx = pageIdx
        accessibilityElements = []

        guard let list else {
            setNeedsDisplay()
            return
        }

        accessibilityElements = list.a11y.map {
            accessibilityElement(for: $0)
        }

        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }

        context.setFillColor(theme.background.cgColor)
        context.fill(bounds)

        guard let displayList else { return }

        context.saveGState()
        context.translateBy(x: 0, y: bounds.height)
        context.scaleBy(x: 1, y: -1)
        context.textMatrix = .identity

        drawImages(displayList.images, in: context)
        drawGlyphRuns(displayList.glyphRuns, in: context)
        drawDecorations(displayList.decorations, in: context)

        context.restoreGState()
        if let spineIdx, let pageIdx {
            onDidDraw?(spineIdx, pageIdx)
        }
    }

    private func configure() {
        isOpaque = true
        backgroundColor = theme.background
        contentMode = .redraw
        isAccessibilityElement = false
    }

    private func drawGlyphRuns(_ runs: [GlyphRun], in context: CGContext) {
        for run in runs {
            guard
                !run.glyphIds.isEmpty,
                run.positions.count == run.glyphIds.count * 2,
                let font = ReaderFontStore.shared.font(id: run.fontId, size: CGFloat(run.size), owner: ownerSession)
            else {
                continue
            }

            context.setFillColor(color(for: run.colorRole).cgColor)
            context.setTextDrawingMode(.fill)

            switch run.orientation {
            case .upright:
                context.textMatrix = .identity
                drawGlyphs(run, with: font, in: context)
            case .sidewaysRotated:
                context.textMatrix = CGAffineTransform(rotationAngle: -.pi / 2)
                drawGlyphs(run, with: font, in: context)
                context.textMatrix = .identity
            }
        }
    }

    private func drawGlyphs(
        _ run: GlyphRun,
        with font: CTFont,
        in context: CGContext
    ) {
        let glyphs = run.glyphIds.map { CGGlyph($0) }
        let points = stride(from: 0, to: run.positions.count, by: 2).map { index in
            CGPoint(x: CGFloat(run.positions[index]), y: bounds.height - CGFloat(run.positions[index + 1]))
        }

        glyphs.withUnsafeBufferPointer { glyphBuffer in
            points.withUnsafeBufferPointer { pointBuffer in
                CTFontDrawGlyphs(
                    font,
                    glyphBuffer.baseAddress!,
                    pointBuffer.baseAddress!,
                    glyphBuffer.count,
                    context
                )
            }
        }
    }

    private func drawDecorations(_ decorations: [Decoration], in context: CGContext) {
        for decoration in decorations {
            context.setFillColor(color(for: decoration.colorRole).cgColor)
            context.fill(pageRect(for: decoration.rect))
        }
    }

    private func drawImages(_ images: [ImagePlacement], in context: CGContext) {
        for placement in images {
            let rect = pageRect(for: placement.rect)
            guard let imageProvider else {
                drawImagePlaceholder(in: rect, context: context)
                continue
            }

            guard let image = imageProvider.image(for: placement.href, onReady: { [weak self] in
                self?.setNeedsDisplay()
            }) else {
                drawImagePlaceholder(in: rect, context: context)
                continue
            }

            guard let cgImage = image.cgImage else {
                drawImagePlaceholder(in: rect, context: context)
                continue
            }

            context.draw(cgImage, in: aspectFitRect(for: image.size, in: rect))
        }
    }

    private func drawImagePlaceholder(in rect: CGRect, context: CGContext) {
        let secondary = color(for: .secondary)
        context.setFillColor(secondary.withAlphaComponent(0.08).cgColor)
        context.fill(rect)
        context.setStrokeColor(secondary.withAlphaComponent(0.20).cgColor)
        context.setLineWidth(1)
        context.stroke(rect.insetBy(dx: 0.5, dy: 0.5))
    }

    /// A link block's element: VoiceOver's double-tap (and any other
    /// activation) reaches the same link path a sighted tap takes.
    internal func activateLink(_ block: A11yBlock) -> Bool {
        guard block.isLink, let spineIdx, let pageIdx, let onLinkActivated else { return false }
        onLinkActivated(
            spineIdx,
            pageIdx,
            CGFloat(block.rect.x + block.rect.width / 2),
            CGFloat(block.rect.y + block.rect.height / 2)
        )
        return true
    }

    private func accessibilityElement(for block: A11yBlock) -> UIAccessibilityElement {
        let element = block.isLink
            ? PageLinkAccessibilityElement(accessibilityContainer: self, activate: { [weak self] in
                self?.activateLink(block) ?? false
            })
            : UIAccessibilityElement(accessibilityContainer: self)
        element.accessibilityFrameInContainerSpace = CGRect(
            x: block.rect.x,
            y: block.rect.y,
            width: block.rect.width,
            height: block.rect.height
        )

        let label = NSMutableAttributedString(string: block.text)
        if let language = block.lang {
            label.addAttribute(
                .accessibilitySpeechLanguage,
                value: language,
                range: NSRange(location: 0, length: label.length)
            )
        }
        element.accessibilityAttributedLabel = label

        switch block.role {
        case .body:
            element.accessibilityTraits = .staticText
        case .heading:
            element.accessibilityTraits = .header
        case .link:
            element.accessibilityTraits = .link
        }
        if block.isLink {
            element.accessibilityTraits.insert(.link)
        }

        return element
    }

    private func color(for role: ColorRole) -> UIColor {
        switch role {
        case .text:
            theme.foreground
        case .secondary:
            theme.dimmedForeground
        case .link:
            UIColor(ink: theme.isNight ? 0xD9AE63 : 0xB4863B)
        }
    }

    private func pageRect(for rect: Rect) -> CGRect {
        CGRect(
            x: rect.x,
            y: bounds.height - rect.y - rect.height,
            width: rect.width,
            height: rect.height
        )
    }

    private func aspectFitRect(for imageSize: CGSize, in rect: CGRect) -> CGRect {
        guard imageSize.width > 0, imageSize.height > 0 else { return rect }

        let scale = min(rect.width / imageSize.width, rect.height / imageSize.height)
        let size = CGSize(width: imageSize.width * scale, height: imageSize.height * scale)
        return CGRect(
            x: rect.midX - size.width / 2,
            y: rect.midY - size.height / 2,
            width: size.width,
            height: size.height
        )
    }
}

/// An accessibility element for a link block, so VoiceOver's activation
/// gesture actually follows the link instead of announcing "link" and
/// doing nothing.
final class PageLinkAccessibilityElement: UIAccessibilityElement {
    private let activate: () -> Bool

    init(accessibilityContainer: Any, activate: @escaping () -> Bool) {
        self.activate = activate
        super.init(accessibilityContainer: accessibilityContainer)
    }

    override func accessibilityActivate() -> Bool {
        activate()
    }
}
