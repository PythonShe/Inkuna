import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    /// The launch read of the settings record, cancelled if the scene goes
    /// away before it lands.
    private var launchTask: Task<Void, Never>?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        self.window = window
        // Settings live in the core; the launch screen holds for the one
        // local read before the first frame commits.
        let urlContexts = connectionOptions.urlContexts
        launchTask = Task { @MainActor in
            await AppSettings.shared.load()
            guard !Task.isCancelled else { return }
            let settings = AppSettings.shared
            // The in-app day/night side follows the reading theme, not the
            // system appearance.
            window.overrideUserInterfaceStyle = settings.readingTheme.isNight ? .dark : .light
            window.rootViewController = settings.hasCompletedOnboarding
                ? MainTabBarController()
                : RootNavigationController(rootViewController: WelcomeViewController())
            window.makeKeyAndVisible()
            #if DEBUG
            self.debugRoute()
            #endif
            // A book handed to us while the app was not running ("Open with
            // Inkuna" from Files, Mail, Safari, a share sheet) arrives here.
            self.openIncomingDocuments(urlContexts, in: scene)
        }
    }

    func sceneDidDisconnect(_ scene: UIScene) {
        launchTask?.cancel()
    }

    /// The same books, when the app is already running.
    func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
        openIncomingDocuments(URLContexts, in: scene)
    }

    /// Routes inbound documents into the import flow. It presents from the
    /// topmost screen and waits for the window to be up, so this is safe
    /// to call before the scene has finished connecting.
    private func openIncomingDocuments(_ contexts: Set<UIOpenURLContext>, in scene: UIScene) {
        #if DEBUG
        // `-inkuna.debugScreen import` opens the import harness. It hangs
        // off this method rather than `debugShow` so the import feature
        // owns every line it added to this file.
        ImportDebugLaunch.routeIfRequested(in: scene)
        #endif
        guard !contexts.isEmpty else { return }
        ImportFlow.handle(contexts, presentingFrom: ImportFlow.presenter(in: scene))
    }

    #if DEBUG
    /// Deep-launches a screen for development and screenshot runs:
    /// `xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen reader`
    /// (launch arguments land in the UserDefaults argument domain).
    ///
    /// `-inkuna.debugImportDocuments YES` first imports every `.epub` found
    /// in the app's Documents directory straight through the core, bypassing
    /// the import UI — a scriptable way to seed a library for reader runs
    /// (push files in with `xcrun simctl` and the container path from
    /// `simctl get_app_container`). `-inkuna.debugScreen import` exercises
    /// the import UI itself, from its own `ImportFixtures/` directory.
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
                // The detail screen needs a real publication; deep-launch
                // onto the newest import (fixture runs seed one first).
                Task {
                    guard
                        let bookshelf = try? await LibraryStore.shared.library(),
                        let publication = try? await bookshelf.library().list(shelf: .all, sort: .recentlyAdded).first
                    else { return }
                    let detail = BookDetailViewController(publication: publication)
                    detail.hidesBottomBarWhenPushed = true
                    navigation.pushViewController(detail, animated: false)
                }
            case "reader":
                guard let navigation = main.selectedViewController as? UINavigationController else { return }
                ReaderLauncher.push(on: navigation)
            case "settings":
                main.present(SettingsViewController(), animated: false)
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
            let outcomes = try await bookshelf.importer().importBatch(paths: epubs, listener: nil)
            print("[inkuna.debug] imported fixtures: \(outcomes)")
        } catch {
            print("[inkuna.debug] fixture import failed: \(error)")
        }
    }
    #endif
}
