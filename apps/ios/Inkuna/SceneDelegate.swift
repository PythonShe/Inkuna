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
    private func debugRoute() {
        guard let screen = UserDefaults.standard.string(forKey: "inkuna.debugScreen") else { return }
        if let main = window?.rootViewController as? MainTabBarController {
            switch screen {
            case "library": main.select(.library)
            case "search": main.select(.search)
            case "stats": main.select(.stats)
            case "detail", "reader":
                guard let navigation = main.selectedViewController as? UINavigationController else { return }
                let book = PlaceholderLibrary.heroBook
                let destination: UIViewController = screen == "detail"
                    ? BookDetailViewController(book: book)
                    : ReaderViewController(book: book)
                destination.hidesBottomBarWhenPushed = true
                navigation.pushViewController(destination, animated: false)
            default:
                break
            }
        } else if let navigation = window?.rootViewController as? UINavigationController,
                  screen == "themepick" {
            navigation.pushViewController(ThemePickViewController(), animated: false)
        }
    }
    #endif
}
