import UIKit

/// Root container for the four tab screens with the floating capsule
/// tab bar. Tab switches cross-fade on the quiet curve; screens keep
/// their state between visits.
final class MainViewController: UIViewController {
    private lazy var tabBar = InkTabBar(
        items: MainTab.allCases.map { InkTabBar.Item(id: $0.rawValue, symbol: $0.symbol, title: $0.title) },
        selectedID: MainTab.tonight.rawValue
    )
    private var screens: [MainTab: UIViewController] = [:]
    private var currentTab: MainTab?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        tabBar.onSelect = { [weak self] id in
            guard let tab = MainTab(rawValue: id) else { return }
            self?.show(tab)
        }
        tabBar.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(tabBar)
        NSLayoutConstraint.activate([
            tabBar.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            tabBar.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -6),
        ])

        show(.tonight)
    }

    private func screen(for tab: MainTab) -> UIViewController {
        if let existing = screens[tab] { return existing }
        let controller: UIViewController
        switch tab {
        case .tonight: controller = TonightViewController()
        case .library: controller = LibraryViewController()
        case .search: controller = SearchViewController()
        case .stats: controller = StatsViewController()
        }
        screens[tab] = controller
        return controller
    }

    private func show(_ tab: MainTab) {
        guard tab != currentTab else { return }
        let incoming = screen(for: tab)
        let outgoing = currentTab.flatMap { screens[$0] }
        currentTab = tab

        addChild(incoming)
        incoming.view.frame = view.bounds
        incoming.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        incoming.view.alpha = 0
        view.insertSubview(incoming.view, belowSubview: tabBar)
        incoming.didMove(toParent: self)

        InkMotion.runQuiet {
            incoming.view.alpha = 1
            outgoing?.view.alpha = 0
        } completion: {
            outgoing?.willMove(toParent: nil)
            outgoing?.view.removeFromSuperview()
            outgoing?.removeFromParent()
        }
    }
}
