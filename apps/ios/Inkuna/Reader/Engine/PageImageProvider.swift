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
                    self?.finish(href: href, image: nil, byteCount: 0)
                    return
                }
                self?.finish(href: href, image: image, byteCount: data.count)
            } catch {
                self?.finish(href: href, image: nil, byteCount: 0)
            }
        }

        return nil
    }

    private func finish(href: String, image: UIImage?, byteCount: Int) {
        inFlight[href] = nil

        guard let image else {
            permanentlyMissing.insert(href)
            pendingCallbacks[href] = nil
            return
        }

        imageCache.setObject(image, forKey: href as NSString, cost: byteCount)
        let callbacks = pendingCallbacks.removeValue(forKey: href) ?? []
        callbacks.forEach { $0() }
    }

    private nonisolated static func decodeImage(_ data: Data) async -> UIImage? {
        await Task.detached(priority: .userInitiated) {
            guard let image = UIImage(data: data) else { return nil }
            return image.preparingForDisplay() ?? image
        }.value
    }
}
