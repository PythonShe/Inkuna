import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        let settings = AppSettings.shared
        // The in-app day/night side follows the reading theme, not the
        // system appearance.
        window.overrideUserInterfaceStyle = settings.readingTheme.isNight ? .dark : .light
        window.rootViewController = settings.hasCompletedOnboarding
            ? MainTabBarController()
            : RootNavigationController(rootViewController: WelcomeViewController())
        window.makeKeyAndVisible()
        self.window = window
        #if DEBUG
        debugRoute()
        #endif
    }

    #if DEBUG
    /// Deep-launches a screen for development and screenshot runs:
    /// `xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen reader`
    /// (launch arguments land in the UserDefaults argument domain).
    ///
    /// `-inkuna.debugImportDocuments YES` first imports every `.epub` found
    /// in the app's Documents directory through the core — the fixture path
    /// for exercising the reader until import UI ships (push files in with
    /// `xcrun simctl` and the container path from `simctl get_app_container`).
    private func debugRoute() {
        let wantsFixtureImport = UserDefaults.standard.bool(forKey: "inkuna.debugImportDocuments")
        let screen = UserDefaults.standard.string(forKey: "inkuna.debugScreen")
        guard wantsFixtureImport || screen != nil else { return }
        Task { @MainActor in
            if wantsFixtureImport {
                await Self.debugImportDocuments()
            }
            guard let screen else { return }
            self.debugShow(screen)
        }
    }

    private func debugShow(_ screen: String) {
        if let main = window?.rootViewController as? MainTabBarController {
            switch screen {
            case "library": main.select(.library)
            case "search": main.select(.search)
            case "stats": main.select(.stats)
            case "detail":
                guard let navigation = main.selectedViewController as? UINavigationController else { return }
                let detail = BookDetailViewController(book: PlaceholderLibrary.heroBook)
                detail.hidesBottomBarWhenPushed = true
                navigation.pushViewController(detail, animated: false)
            case "reader":
                guard let navigation = main.selectedViewController as? UINavigationController else { return }
                ReaderLauncher.push(on: navigation)
            default:
                break
            }
        } else if let navigation = window?.rootViewController as? UINavigationController,
                  screen == "themepick" {
            navigation.pushViewController(ThemePickViewController(), animated: false)
        }
    }

    private static func debugImportDocuments() async {
        guard let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else { return }
        let epubs = ((try? FileManager.default.contentsOfDirectory(at: documents, includingPropertiesForKeys: nil)) ?? [])
            .filter { $0.pathExtension.lowercased() == "epub" }
            .map(\.path)
            .sorted()
        guard !epubs.isEmpty else { return }
        do {
            let bookshelf = try await LibraryStore.shared.library()
            let outcomes = try await bookshelf.importBatch(paths: epubs)
            print("[inkuna.debug] imported fixtures: \(outcomes)")
        } catch {
            print("[inkuna.debug] fixture import failed: \(error)")
        }
    }
    #endif
}
