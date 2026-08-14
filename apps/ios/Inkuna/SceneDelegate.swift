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
        let root: UIViewController = settings.hasCompletedOnboarding
            ? MainViewController()
            : WelcomeViewController()
        window.rootViewController = RootNavigationController(rootViewController: root)
        window.makeKeyAndVisible()
        self.window = window
    }
}
