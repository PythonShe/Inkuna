import UIKit

extension ReaderViewController {
    func applyBrightness(_ brightness: Double) {
        dimView.alpha = max(0, 0.78 - brightness) / 1.7
        dimView.isHidden = dimView.alpha == 0
    }

    func pageInfoText() -> String {
        guard let readerSession, let coordinate = currentAnchor() else {
            let format = NSLocalizedString("reader_percent", comment: "")
            return String.localizedStringWithFormat(format, Int64((publication.progression * 100).rounded()))
        }
        let position = readerSession.positionOf(coordinate: coordinate)
        let count = readerSession.positionCount()
        let format = NSLocalizedString("reader_page_info", comment: "")
        return String.localizedStringWithFormat(
            format,
            Int64(position),
            Int64(count),
            Int64((Double(position) / Double(max(count, 1)) * 100).rounded())
        )
    }

    func updatePageInfo() {
        pageInfoLabel.text = pageInfoText()
        let percent: Int64
        if let readerSession, let coordinate = currentAnchor() {
            percent = Int64((Double(readerSession.positionOf(coordinate: coordinate)) / Double(max(readerSession.positionCount(), 1)) * 100).rounded())
        } else {
            percent = Int64((publication.progression * 100).rounded())
        }
        let format = NSLocalizedString("reader_menu_contents", comment: "")
        menuView.contentsPill.text = String.localizedStringWithFormat(format, percent)
    }

    override var canBecomeFirstResponder: Bool { true }

    func takeKeyCommandChain() {
        guard presentedViewController == nil, searchPanel?.isEditing != true else { return }
        becomeFirstResponder()
    }

    override var keyCommands: [UIKeyCommand]? {
        guard searchPanel?.isEditing != true else { return nil }
        let commands = [
            UIKeyCommand(input: UIKeyCommand.inputLeftArrow, modifierFlags: [], action: #selector(keyTurnLeft)),
            UIKeyCommand(input: UIKeyCommand.inputRightArrow, modifierFlags: [], action: #selector(keyTurnRight)),
            UIKeyCommand(input: " ", modifierFlags: [], action: #selector(keyTurnForward)),
        ]
        commands.forEach { $0.wantsPriorityOverSystemBehavior = true }
        return commands
    }

    @objc func keyTurnLeft() { _ = pager?.turnLeft() }
    @objc func keyTurnRight() { _ = pager?.turnRight() }
    @objc func keyTurnForward() { _ = pager?.turnForward() }

    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        guard let pager else { return false }
        let turned: Bool = switch direction {
        case .left: pager.turnRight()
        case .right: pager.turnLeft()
        case .next, .down: pager.turnForward()
        case .previous, .up: pager.turnBackward()
        default: false
        }
        guard turned else { return false }
        announcePageWhenSettled = true
        return true
    }

    func showLinkNotFollowed() {
        InkToastView.show(
            symbol: "link",
            text: String(localized: "reader_link_failed", defaultValue: "This link could not be opened."),
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }
}

extension ReaderViewController: UIAdaptivePresentationControllerDelegate {
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        takeKeyCommandChain()
    }
}

#if DEBUG
extension ReaderViewController {
    private static var didRunDebugRoute = false

    func runDebugRouteIfNeeded() {
        guard !Self.didRunDebugRoute else { return }
        Self.didRunDebugRoute = true
        switch UserDefaults.standard.string(forKey: "inkuna.debugReaderUI") {
        case "menu": setMenu(visible: true)
        case "search": showSearch()
        case "contents": presentContents()
        case "theme": presentThemeSheet()
        case "immersed": setChrome(visible: false)
        default: break
        }
    }
}
#endif
