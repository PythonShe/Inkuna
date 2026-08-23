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

    init(session: ReaderSession) {
        self.session = session
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
        inFlight[href] = Task { @MainActor [weak self, session] in
            do {
                let data = try await session.resource(href: href)
                guard let image = await Self.decodeImage(data) else {
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

    private nonisolated static func decodeImage(_ data: Data) async -> UIImage? {
        await Task.detached(priority: .userInitiated) {
            guard let image = UIImage(data: data) else { return nil }
            let normalizedImage = normalizedOrientation(of: image)
            return normalizedImage.preparingForDisplay() ?? normalizedImage
        }.value
    }

    private nonisolated static func normalizedOrientation(of image: UIImage) -> UIImage {
        guard image.imageOrientation != .up else { return image }

        let size: CGSize
        switch image.imageOrientation {
        case .left, .right, .leftMirrored, .rightMirrored:
            size = CGSize(width: image.size.height, height: image.size.width)
        default:
            size = image.size
        }

        let format = UIGraphicsImageRendererFormat()
        format.scale = image.scale
        format.opaque = false
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    private nonisolated static func decodedByteCost(of image: UIImage) -> Int {
        let pixelWidth = (image.size.width * image.scale).rounded(.up)
        let pixelHeight = (image.size.height * image.scale).rounded(.up)
        let byteCount = pixelWidth * pixelHeight * 4
        return byteCount >= CGFloat(Int.max) ? Int.max : Int(byteCount)
    }
}
