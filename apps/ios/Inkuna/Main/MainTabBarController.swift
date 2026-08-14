import UIKit

/// Root tab container on the native tab bar: on iOS 26 that means the
/// floating Liquid Glass capsule that minimizes on scroll and the split
/// native search tab; earlier systems keep the standard bar. Each tab owns
/// a chromeless navigation controller so screens draw their own headers.
final class MainTabBarController: UITabBarController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp
        tabBar.tintColor = InkColor.accent
        tabBar.unselectedItemTintColor = InkColor.textSecondary
        if #available(iOS 26.0, *) {
            tabBarMinimizeBehavior = .onScrollDown
        }

        tabs = MainTab.allCases.map { tab in
            switch tab {
            case .search:
                return UISearchTab { _ in Self.screen(for: tab) }
            default:
                return UITab(
                    title: tab.title,
                    image: UIImage(systemName: tab.symbol),
                    identifier: tab.rawValue
                ) { _ in Self.screen(for: tab) }
            }
        }
    }

    private static func screen(for tab: MainTab) -> UIViewController {
        let screen: UIViewController
        switch tab {
        case .tonight: screen = TonightViewController()
        case .library: screen = LibraryViewController()
        case .search: screen = SearchViewController()
        case .stats: screen = StatsViewController()
        }
        return RootNavigationController(rootViewController: screen)
    }

    /// Programmatic tab selection (deep links, debug routes).
    func select(_ tab: MainTab) {
        loadViewIfNeeded()
        if let index = MainTab.allCases.firstIndex(of: tab) {
            selectedIndex = index
        }
    }
}
