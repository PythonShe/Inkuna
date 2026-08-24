import ImageIO
import UIKit

/// Caches decoded page images while sharing one resource request per href.
@MainActor
final class PageImageProvider {
    private let session: ReaderSession
    private let imageCache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 32 * 1024 * 1024
        return cache
    }()
    private var inFlight: [String: Task<Void, Never>] = [:]
    private var pendingCallbacks: [String: [() -> Void]] = [:]
    private var permanentlyMissing: Set<String> = []

    private let maxPixelSize: CGFloat
    private let maxPixelArea: CGFloat

    init(session: ReaderSession) {
        self.session = session
        (self.maxPixelSize, self.maxPixelArea) = Self.displayPixelCaps()
    }

    /// The maximum downsample factor a source may need before it is
    /// rejected outright — the same absurd-size bound the Android shell
    /// enforces (its power-of-2 `inSampleSize` stops at 32), so both
    /// shells drop the same pathological images.
    private nonisolated static let maxSampleFactor = 32

    /// The two decode caps, both derived from the largest connected
    /// display and shared as a contract with the Android shell:
    /// - edge: 2x the display's longest dimension in pixels;
    /// - area: (2x display width) x (2x display height) total pixels,
    ///   so a permitted-edge but near-square bitmap cannot still blow
    ///   far past the cache budget.
    /// Each display dimension is floored at 1024, so a capless context
    /// yields Android's 2048 edge cap and a 2048x2048 area floor. A page
    /// image never needs more resolution than the screen can show, and an
    /// iOS memory overshoot is uncatchable (jetsam) — so oversized images
    /// decode downsampled while anything within both caps keeps full
    /// resolution.
    private static func displayPixelCaps() -> (edge: CGFloat, area: CGFloat) {
        let bounds = UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.screen }
            .map { ($0.nativeBounds.width, $0.nativeBounds.height) }
            .max { $0.0 * $0.1 < $1.0 * $1.1 }
        let width = max(bounds?.0 ?? 0, 1024) * 2
        let height = max(bounds?.1 ?? 0, 1024) * 2
        return (edge: max(width, height), area: width * height)
    }

    func image(for href: String, onReady: @escaping () -> Void) -> UIImage? {
        let key = href as NSString
        if let image = imageCache.object(forKey: key) {
            return image
        }

        guard !permanentlyMissing.contains(href) else { return nil }

        pendingCallbacks[href, default: []].append(onReady)
        guard inFlight[href] == nil else { return nil }

        let session = session
        let maxPixelSize = maxPixelSize
        let maxPixelArea = maxPixelArea
        inFlight[href] = Task { @MainActor [weak self, session] in
            do {
                let data = try await session.resource(href: href)
                guard let image = await Self.decodeImage(
                    data,
                    maxPixelSize: maxPixelSize,
                    maxPixelArea: maxPixelArea
                ) else {
                    self?.finish(href: href, image: nil)
                    return
                }
                self?.finish(href: href, image: image)
            } catch {
                self?.finish(href: href, image: nil)
            }
        }

        return nil
    }

    private func finish(href: String, image: UIImage?) {
        inFlight[href] = nil

        guard let image else {
            permanentlyMissing.insert(href)
            pendingCallbacks[href] = nil
            return
        }

        imageCache.setObject(image, forKey: href as NSString, cost: Self.decodedByteCost(of: image))
        let callbacks = pendingCallbacks.removeValue(forKey: href) ?? []
        callbacks.forEach { $0() }
    }

    /// Decodes through `CGImageSourceCreateThumbnailAtIndex` so an oversized
    /// image never materializes at full resolution (`UIImage(data:)` +
    /// `preparingForDisplay()` would). Both caps exceed any on-screen need,
    /// so images within them keep full resolution; the transform option
    /// bakes EXIF orientation into the decoded bitmap and the
    /// immediate-cache option decodes eagerly off the main thread.
    private nonisolated static func decodeImage(
        _ data: Data,
        maxPixelSize: CGFloat,
        maxPixelArea: CGFloat
    ) async -> UIImage? {
        await Task.detached(priority: .userInitiated) {
            let sourceOptions = [kCGImageSourceShouldCache: false] as CFDictionary
            guard let source = CGImageSourceCreateWithData(data as CFData, sourceOptions) else {
                return nil
            }
            guard
                let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, sourceOptions) as? [CFString: Any],
                let pixelWidth = properties[kCGImagePropertyPixelWidth] as? Int,
                let pixelHeight = properties[kCGImagePropertyPixelHeight] as? Int,
                pixelWidth > 0, pixelHeight > 0
            else {
                return nil
            }
            // The effective longest-edge bound satisfies BOTH caps: the
            // edge cap directly, and the area cap because at a longest
            // edge of sqrt(area x long/short) the short edge measures
            // sqrt(area x short/long) — total pixels exactly `area`. A
            // near-square bitmap within the edge cap can otherwise still
            // decode to ~4x the intended byte budget.
            let longEdge = CGFloat(max(pixelWidth, pixelHeight))
            let shortEdge = CGFloat(min(pixelWidth, pixelHeight))
            let areaBound = (maxPixelArea * longEdge / shortEdge).squareRoot().rounded(.down)
            let pixelCap = min(maxPixelSize, areaBound)
            // Same contract as the Android shell: a source that would still
            // exceed the cap at the maximum downsample factor is absurd —
            // reject it instead of materializing anything.
            guard CGFloat((max(pixelWidth, pixelHeight) + Self.maxSampleFactor - 1) / Self.maxSampleFactor) <= pixelCap
            else {
                return nil
            }
            let thumbnailOptions = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: pixelCap,
            ] as [CFString: Any] as CFDictionary
            guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnailOptions) else {
                return nil
            }
            return UIImage(cgImage: cgImage)
        }.value
    }

    private nonisolated static func decodedByteCost(of image: UIImage) -> Int {
        let pixelWidth = (image.size.width * image.scale).rounded(.up)
        let pixelHeight = (image.size.height * image.scale).rounded(.up)
        let byteCount = pixelWidth * pixelHeight * 4
        return byteCount >= CGFloat(Int.max) ? Int.max : Int(byteCount)
    }
}
