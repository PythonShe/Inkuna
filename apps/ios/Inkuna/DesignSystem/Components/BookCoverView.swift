import ImageIO
import UIKit

/// Book cover (design-system `BookCover`): a 2:3 tile with the book
/// shadow. It renders the cover art the core extracted at import
/// (`Publication.coverPath`), falling back to the design system's generated
/// placeholder — a seeded ink-and-paper palette with the title set in serif
/// — while the art decodes and for books that ship without any.
final class BookCoverView: UIView {
    /// Seeded placeholder palettes from the design system (`BookCover.jsx`).
    private static let palettes: [(background: UInt32, foreground: UInt32)] = [
        (0x2E3440, 0xD8DEE9),
        (0x4A3B2A, 0xEFE7D9),
        (0x5C3A38, 0xF2E4DC),
        (0x33463C, 0xE4EDE6),
        (0x3A3A55, 0xE4E4F0),
        (0x6B5537, 0xF6EDD9),
    ]

    /// The one generated-cover seed in the app.
    ///
    /// Every screen that draws a book must reach the same palette for it, so
    /// the reduction lives here rather than being re-derived per screen —
    /// the library row, the contents sheet, and the debug harness once had
    /// three different hashes and three different colours for one book.
    ///
    /// FNV-1a, deliberately not `hashValue`: Swift seeds string hashing per
    /// process, so a book would change colour on every launch.
    static func coverSeed(for id: String) -> Int {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in id.utf8 {
            hash ^= UInt64(byte)
            hash &*= 0x0000_0100_0000_01b3
        }
        return Int(hash % UInt64(Int.max))
    }

    /// Decoded covers, keyed by path and the size bucket they were decoded
    /// for. The list, search and shelf screens re-create their rows — and
    /// with them these views — on every render; the cache is what keeps
    /// that from re-decoding the same art each time.
    private static let artCache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.countLimit = 128
        return cache
    }()

    private let content = UIView()
    private let titleLabel = InkLabel()
    private let authorLabel = InkLabel()

    /// The cover art's path, decoded once the tile's width is known — the
    /// decode is sized to the tile, and every screen sizes it differently.
    private let coverPath: String?

    /// The in-flight decode. `nonisolated(unsafe)` so the nonisolated
    /// `deinit` can cancel it; only ever touched on the main actor.
    nonisolated(unsafe) private var coverTask: Task<Void, Never>?

    /// The cover art on top of the generated cover, once it has decoded.
    private var artView: UIImageView?

    /// `coverPath` is the absolute path of the core-extracted cover image,
    /// or nil for a book without art (the generated cover stays).
    init(title: String, author: String, seed: Int, coverPath: String?) {
        self.coverPath = coverPath
        super.init(frame: .zero)
        let palette = Self.palettes[abs(seed) % Self.palettes.count]

        installInkShadow(.book)

        content.backgroundColor = UIColor(ink: palette.background)
        content.layer.cornerRadius = InkRadius.xs
        content.layer.masksToBounds = true
        content.layer.borderWidth = 1
        content.layer.borderColor = UIColor.white.withAlphaComponent(0.06).cgColor
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)

        titleLabel.text = title
        titleLabel.textColor = UIColor(ink: palette.foreground)
        titleLabel.numberOfLines = 3

        authorLabel.text = author
        authorLabel.textColor = UIColor(ink: palette.foreground, alpha: 0.8)
        authorLabel.numberOfLines = 1

        content.addSubview(titleLabel)
        content.addSubview(authorLabel)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        authorLabel.translatesAutoresizingMaskIntoConstraints = false

        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
            // Books stay rectangular: 2:3, like the design's covers.
            heightAnchor.constraint(equalTo: widthAnchor, multiplier: 1.5),

            titleLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
            titleLabel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -10),
            titleLabel.topAnchor.constraint(equalTo: content.topAnchor, constant: 10),
            authorLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
            authorLabel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -10),
            authorLabel.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -10),
        ])

    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    deinit {
        coverTask?.cancel()
    }

    /// Decodes the cover off the main thread, at the size this tile draws
    /// it, and fades it in over the generated cover. Decode failure (a
    /// deleted or corrupt file) simply leaves the generated cover standing —
    /// it is always presentable.
    private func loadCoverArt(at path: String, tileWidth: CGFloat) {
        // Covers are 2:3, so the height is the edge the thumbnail must
        // cover. Bucketed so the few cover sizes in the app share cache
        // entries rather than taking one per pixel width.
        let scale = (window?.screen ?? UIScreen.main).scale
        let pixels = max(tileWidth * 1.5 * scale, 1)
        let bucket = Int((pixels / 64).rounded(.up)) * 64
        let key = "\(path)#\(bucket)" as NSString
        if let cached = Self.artCache.object(forKey: key) {
            showCoverArt(cached, animated: false)
            return
        }
        coverTask?.cancel()
        coverTask = Task { [weak self] in
            let image = await Task.detached(priority: .userInitiated) {
                Self.thumbnail(at: path, maxPixelSize: bucket)
            }.value
            guard !Task.isCancelled, let self, let image else { return }
            Self.artCache.setObject(image, forKey: key)
            self.showCoverArt(image, animated: true)
        }
    }

    /// Thumbnail-decodes through ImageIO: a full-raster decode of a large
    /// cover would allocate hundreds of megabytes to draw a 52pt row.
    nonisolated private static func thumbnail(at path: String, maxPixelSize: Int) -> UIImage? {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let source = CGImageSourceCreateWithURL(url, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        return UIImage(cgImage: image)
    }

    private func showCoverArt(_ image: UIImage, animated: Bool) {
        artView?.removeFromSuperview()
        let artView = UIImageView(image: image)
        self.artView = artView
        artView.contentMode = .scaleAspectFill
        artView.clipsToBounds = true
        artView.alpha = animated ? 0 : 1
        artView.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(artView)
        NSLayoutConstraint.activate([
            artView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            artView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            artView.topAnchor.constraint(equalTo: content.topAnchor),
            artView.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
        guard animated else { return }
        InkMotion.runQuiet {
            artView.alpha = 1
        }
    }

    private var lastLayoutWidth: CGFloat = 0

    override func layoutSubviews() {
        super.layoutSubviews()
        // Rasterizing the shadow along the rounded rect avoids an offscreen
        // blur pass per cover per frame.
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: InkRadius.xs).cgPath
        // The placeholder cover's type scales with the tile, mirroring the
        // design system's `max(11, w * .13)` rules. Only on width changes —
        // setting fonts every pass would churn layout from inside layout.
        let width = bounds.width
        guard width > 0, width != lastLayoutWidth else { return }
        lastLayoutWidth = width
        // The art is decoded to the tile's size, so the first real width is
        // the earliest the decode can start.
        if let coverPath {
            loadCoverArt(at: coverPath, tileWidth: width)
        }
        titleLabel.font = InkFont.serif(max(11, width * 0.13), weight: .medium, style: .caption1)
        authorLabel.font = InkFont.sans(max(8, width * 0.085), weight: .regular, style: .caption2)
    }
}
