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

    private var displayList: PageDisplayList?

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

    func present(_ list: PageDisplayList?, spineIdx _: UInt32, pageIdx _: UInt32, session _: ReaderSession) {
        displayList = list
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
                let font = ReaderFontStore.shared.font(id: run.fontId, size: CGFloat(run.size))
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

    private func accessibilityElement(for block: A11yBlock) -> UIAccessibilityElement {
        let element = UIAccessibilityElement(accessibilityContainer: self)
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
