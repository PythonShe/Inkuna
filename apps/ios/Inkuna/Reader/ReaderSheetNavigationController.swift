import UIKit

/// A page hosted by the reader's sheet stack: each page names the detents
/// it wants, and the stack animates the sheet between them on push/pop.
@MainActor
protocol ReaderSheetPage: UIViewController {
    var sheetDetents: [UISheetPresentationController.Detent] { get }
    var preferredDetentIdentifier: UISheetPresentationController.Detent.Identifier? { get }
}

/// The navigation shell around the Theme & type sheet: level 1 keeps its
/// content-fitted detent, pushing the Customize panel grows the sheet to
/// large, and popping shrinks it back — with the system push/pop
/// transition in between. The navigation bar stays hidden; every page
/// draws the design's own header row.
final class ReaderSheetNavigationController: UINavigationController, UINavigationControllerDelegate {
    init(root: ReaderSheetPage) {
        super.init(nibName: nil, bundle: nil)
        viewControllers = [root]
        delegate = self
        isNavigationBarHidden = true
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            sheet.detents = root.sheetDetents
            sheet.selectedDetentIdentifier = root.preferredDetentIdentifier
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.lg
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func navigationController(
        _ navigationController: UINavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    ) {
        guard let page = viewController as? ReaderSheetPage,
              let sheet = sheetPresentationController else { return }
        sheet.animateChanges {
            sheet.detents = page.sheetDetents
            sheet.selectedDetentIdentifier = page.preferredDetentIdentifier
        }
    }
}
