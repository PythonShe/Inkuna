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

    init(session: ReaderSession) {
        self.session = session
        self.maxPixelSize = Self.displayPixelCap()
    }

    /// 2x the largest connected display dimension in pixels. A page image
    /// never needs more resolution than the screen can show, and an iOS
    /// memory overshoot is uncatchable (jetsam) — so oversized images decode
    /// downsampled while anything within the cap keeps full resolution.
    private static func displayPixelCap() -> CGFloat {
        let largest = UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.screen }
            .map { max($0.nativeBounds.width, $0.nativeBounds.height) }
            .max() ?? 2796
        return largest * 2
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
        inFlight[href] = Task { @MainActor [weak self, session] in
            do {
                let data = try await session.resource(href: href)
                guard let image = await Self.decodeImage(data, maxPixelSize: maxPixelSize) else {
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
    /// `preparingForDisplay()` would). The max-pixel-size cap exceeds any
    /// on-screen need, so images within it keep full resolution; the
    /// transform option bakes EXIF orientation into the decoded bitmap and
    /// the immediate-cache option decodes eagerly off the main thread.
    private nonisolated static func decodeImage(
        _ data: Data,
        maxPixelSize: CGFloat
    ) async -> UIImage? {
        await Task.detached(priority: .userInitiated) {
            let sourceOptions = [kCGImageSourceShouldCache: false] as CFDictionary
            guard let source = CGImageSourceCreateWithData(data as CFData, sourceOptions) else {
                return nil
            }
            let thumbnailOptions = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
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
