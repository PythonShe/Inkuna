import UIKit

/// The card shown while books are being added.
///
/// Two details carry the feel:
///
/// - It does not appear for fast work. The card only reveals itself after
///   ``revealDelay``; a single small EPUB imports well inside that, so the
///   common case is a toast and no modal flash at all.
/// - Once revealed it stays for ``minimumVisible``, so a card that did
///   appear is never a blink.
///
/// Progress is honest about what the core can tell us: a determinate bar
/// for a multi-file selection, where "3 of 12" is a real count, and a
/// spinner for a single book, where the core reports nothing until it is
/// done.
@MainActor
final class ImportProgressOverlay: UIView {
    private static let revealDelay: Duration = .milliseconds(350)
    private static let minimumVisible: Duration = .milliseconds(450)

    private let card = UIView()
    private let titleLabel = InkLabel()
    private let statusLabel = InkLabel()
    private let countLabel = InkLabel()
    private let progressBar = InkProgressBar()
    private let spinner = UIActivityIndicatorView(style: .medium)
    private let total: Int
    private let onCancel: (@MainActor () -> Void)?

    private var revealTask: Task<Void, Never>?
    private var shownAt: ContinuousClock.Instant?

    /// The caller waiting on ``dismiss()``. Non-nil only while the fade is
    /// in flight, which is also the single-resume guard: whoever gets here
    /// first — the animator or its fallback — takes it and leaves nil.
    private var dismissContinuation: CheckedContinuation<Void, Never>?
    /// Belt to the animator's braces; cancelled by whichever finishes first.
    private var dismissFallback: Task<Void, Never>?

    init(total: Int, onCancel: (@MainActor () -> Void)? = nil) {
        self.total = total
        self.onCancel = onCancel
        super.init(frame: .zero)
        backgroundColor = InkColor.scrim
        alpha = 0
        accessibilityViewIsModal = true
        buildCard()
        update(ImportProgress(completed: 0, total: total, fileName: nil, phase: .preparing))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    // MARK: Presentation

    /// Installs the overlay and schedules its reveal. Returns immediately:
    /// work that finishes before ``revealDelay`` never shows a card.
    func present(in host: UIView) {
        translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(self)
        NSLayoutConstraint.activate([
            leadingAnchor.constraint(equalTo: host.leadingAnchor),
            trailingAnchor.constraint(equalTo: host.trailingAnchor),
            topAnchor.constraint(equalTo: host.topAnchor),
            bottomAnchor.constraint(equalTo: host.bottomAnchor),
        ])

        card.transform = CGAffineTransform(scaleX: 0.96, y: 0.96)
        revealTask = Task { [weak self] in
            try? await Task.sleep(for: Self.revealDelay)
            guard !Task.isCancelled, let self, superview != nil else { return }
            shownAt = ContinuousClock.now
            spinner.startAnimating()
            let animator = InkMotion.pageAnimator(duration: InkMotion.medium)
            animator.addAnimations {
                self.alpha = 1
                self.card.transform = .identity
            }
            animator.startAnimation()
            UIAccessibility.post(notification: .layoutChanged, argument: titleLabel)
        }
    }

    /// Fades the overlay out, honoring the minimum visible time, and
    /// returns once it is gone so the caller can present its own feedback
    /// into a clear screen.
    func dismiss() async {
        revealTask?.cancel()
        revealTask = nil
        guard let shownAt else {
            removeFromSuperview()
            return
        }
        let visible = ContinuousClock.now - shownAt
        if visible < Self.minimumVisible {
            try? await Task.sleep(for: Self.minimumVisible - visible)
        }
        spinner.stopAnimating()
        await withCheckedContinuation { continuation in
            dismissContinuation = continuation
            let animator = InkMotion.quietAnimator(duration: InkMotion.fast)
            animator.addAnimations {
                self.alpha = 0
                self.card.transform = CGAffineTransform(scaleX: 0.98, y: 0.98)
            }
            animator.addCompletion { _ in
                self.finishDismissal()
            }
            animator.startAnimation()
            // An animation completion that never fires would strand the
            // import flow's serialized chain — every later import would wait
            // on it forever behind a scrim that keeps eating touches. The
            // fade is 200ms-ish; well past that, the overlay leaves anyway.
            dismissFallback = Task {
                try? await Task.sleep(for: .milliseconds(600))
                self.finishDismissal()
            }
        }
    }

    /// Ends the dismissal exactly once, whichever path gets here first.
    private func finishDismissal() {
        dismissFallback?.cancel()
        dismissFallback = nil
        guard let continuation = dismissContinuation else { return }
        dismissContinuation = nil
        removeFromSuperview()
        continuation.resume()
    }

    // MARK: State

    func update(_ progress: ImportProgress) {
        titleLabel.text = ImportCopy.progressTitle(total: progress.total)
        statusLabel.text = progress.fileName
            ?? (progress.phase == .preparing ? ImportCopy.preparing : nil)
        statusLabel.isHidden = statusLabel.text?.isEmpty != false
        guard total > 1 else { return }
        countLabel.text = ImportCopy.progressCount(completed: progress.completed, total: progress.total)
        progressBar.setProgress(progress.fraction, animated: window != nil)
    }

    // MARK: Layout

    private func buildCard() {
        card.backgroundColor = InkColor.bgRaised
        card.layer.cornerRadius = InkRadius.xl
        card.layer.cornerCurve = .continuous
        card.installInkShadow(.lg)
        card.translatesAutoresizingMaskIntoConstraints = false
        addSubview(card)

        let glyph = UIImageView(
            image: UIImage(
                systemName: "books.vertical",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 26, weight: .light)
            )
        )
        glyph.tintColor = InkColor.accentText
        glyph.contentMode = .scaleAspectFit

        titleLabel.font = InkFont.heading
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.textAlignment = .center
        titleLabel.numberOfLines = 0

        // A file name is the one string here that can be long and is worth
        // reading whole; two lines with middle truncation keeps the
        // extension visible on a CJK name that fills both.
        statusLabel.font = InkFont.label
        statusLabel.textColor = InkColor.textSecondary
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 2
        statusLabel.lineBreakMode = .byTruncatingMiddle

        countLabel.font = InkFont.caption
        countLabel.textColor = InkColor.textTertiary
        countLabel.textAlignment = .center

        spinner.color = InkColor.accentText

        let stack = UIStackView(arrangedSubviews: [glyph, titleLabel, statusLabel])
        stack.axis = .vertical
        stack.alignment = .fill
        stack.spacing = InkSpacing.space3
        stack.setCustomSpacing(InkSpacing.space4, after: glyph)

        if total > 1 {
            let bar = UIView()
            progressBar.translatesAutoresizingMaskIntoConstraints = false
            bar.addSubview(progressBar)
            NSLayoutConstraint.activate([
                progressBar.leadingAnchor.constraint(equalTo: bar.leadingAnchor),
                progressBar.trailingAnchor.constraint(equalTo: bar.trailingAnchor),
                progressBar.topAnchor.constraint(equalTo: bar.topAnchor),
                progressBar.bottomAnchor.constraint(equalTo: bar.bottomAnchor),
            ])
            stack.addArrangedSubview(bar)
            stack.setCustomSpacing(InkSpacing.space2, after: bar)
            stack.addArrangedSubview(countLabel)
            if let onCancel {
                let cancel = InkButton(ImportCopy.cancel, variant: .ghost, size: .small) { onCancel() }
                stack.addArrangedSubview(cancel)
            }
        } else {
            stack.addArrangedSubview(spinner)
        }

        stack.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(stack)

        let width = card.widthAnchor.constraint(equalToConstant: 288)
        width.priority = .defaultHigh
        NSLayoutConstraint.activate([
            width,
            card.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -2 * InkSpacing.space8),
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.centerYAnchor.constraint(equalTo: centerYAnchor),
            stack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: InkSpacing.space6),
            stack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -InkSpacing.space6),
            stack.topAnchor.constraint(equalTo: card.topAnchor, constant: InkSpacing.space6),
            stack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -InkSpacing.space6),
        ])
    }
}
